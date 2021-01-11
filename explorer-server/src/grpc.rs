use anyhow::{anyhow, Result};
use bchrpc::bchrpc_client::BchrpcClient;
use bitcoin_cash::{Address, Hashed};
use futures::future::try_join_all;
use tonic::{Status, transport::{Certificate, Channel, ClientTlsConfig, Endpoint}};
use std::{collections::{HashMap, HashSet}, convert::TryInto};

pub mod bchrpc {
    tonic::include_proto!("pb");
}

use bchrpc::BlockInfo;

use crate::{blockchain::{Destination, destination_from_script, from_le_hex, is_coinbase}, db::{BlockMeta, ConfirmedAddressTx, Db, SlpAction, TokenMeta, TxMeta, TxMetaVariant, TxOutSpend}};

pub struct Bchd {
    client: BchrpcClient<Channel>,
    db: Db,
    satoshi_addr_prefix: &'static str,
}

impl Bchd {
    pub async fn connect(db: Db, satoshi_addr_prefix: &'static str) -> Result<Self> {
        use std::fs;
        use std::io::Read;
        let mut cert_file = fs::File::open("cert.crt")?;
        let mut cert = Vec::new();
        cert_file.read_to_end(&mut cert)?;
        let tls_config = ClientTlsConfig::new().ca_certificate(Certificate::from_pem(&cert));
        let endpoint = Endpoint::from_static("https://api2.be.cash:8445").tls_config(tls_config)?;
        let client = BchrpcClient::connect(endpoint).await?;
        Ok(Bchd { client, db, satoshi_addr_prefix })
    }

    pub async fn block_at_height(&self, height: i32) -> Result<BlockInfo> {
        use bchrpc::{GetBlockInfoRequest, get_block_info_request::HashOrHeight};
        let mut bchd = self.client.clone();
        let block_info = bchd.get_block_info(GetBlockInfoRequest {
            hash_or_height: Some(HashOrHeight::Height(height))
        }).await?;
        let block_info = block_info.get_ref();
        let block_info = block_info.info.as_ref()
            .ok_or_else(|| anyhow!("No block info"))?;
        return Ok(block_info.clone())
    }

    pub async fn blockchain_info(&self) -> Result<bchrpc::GetBlockchainInfoResponse> {
        use bchrpc::GetBlockchainInfoRequest;
        let mut bchd = self.client.clone();
        let blockchain_info = bchd.get_blockchain_info(GetBlockchainInfoRequest {}).await?;
        let blockchain_info = blockchain_info.get_ref();
        Ok(blockchain_info.clone())
    }
}

pub struct BlockMetaInfo {
    pub block_meta: BlockMeta,
    pub block_info: BlockInfo,
}

impl Bchd {
    /// Returns 2000 blocks or less
    pub async fn blocks_above(&self, height: i32) -> Result<Vec<BlockMetaInfo>> {
        use bchrpc::GetHeadersRequest;
        let mut bchd = self.client.clone();
        let first_block_info = self.block_at_height(height).await?;
        let block_infos = bchd.get_headers(GetHeadersRequest {
            block_locator_hashes: vec![first_block_info.hash.clone()],
            stop_hash: vec![],
        }).await?;
        let block_infos = block_infos.get_ref();
        let block_infos = block_infos.headers.clone();
        let futures = block_infos.into_iter().map(|block_info| self.fetch_block_meta_info(block_info));
        let results = try_join_all(futures).await?;
        Ok(results)
    }

    pub async fn block_meta_info(&self, block_hash: &[u8]) -> Result<BlockMetaInfo> {
        use bchrpc::{GetBlockInfoRequest, get_block_info_request::HashOrHeight};
        let mut bchd = self.client.clone();
        let block_info = bchd.get_block_info(GetBlockInfoRequest {
            hash_or_height: Some(HashOrHeight::Hash(block_hash.to_vec())),
        }).await?;
        let block_info = block_info.get_ref().info.as_ref().ok_or_else(|| anyhow!("No block info"))?.clone();
        self.fetch_block_meta_info(block_info).await
    }

    async fn fetch_block_meta_info(&self, block_info: BlockInfo) -> Result<BlockMetaInfo> {
        use bchrpc::{GetBlockRequest, GetTransactionRequest, get_block_request::HashOrHeight, block::transaction_data::TxidsOrTxs};
        let block_meta = match self.db.block_meta(&block_info.hash)? {
            Some(block_meta) => block_meta,
            None => {
                let mut bchd = self.client.clone();
                let block = bchd.get_block(GetBlockRequest {
                    full_transactions: false,
                    hash_or_height: Some(HashOrHeight::Hash(block_info.hash.clone()))
                }).await?;
                let block = block.get_ref().block.as_ref().ok_or_else(|| anyhow!("Block not found"))?;
                let block_info = block.info.as_ref().ok_or_else(|| anyhow!("No block info"))?;
                let coinbase_tx_hash = block.transaction_data[0].txids_or_txs.as_ref()
                    .ok_or_else(|| anyhow!("No txs in block"))?;
                let coinbase_tx = bchd.get_transaction(GetTransactionRequest {
                    hash: match coinbase_tx_hash {
                        TxidsOrTxs::TransactionHash(hash) => hash.clone(),
                        _ => unreachable!(),
                    },
                    include_token_metadata: false,
                }).await?;
                let coinbase_tx = coinbase_tx.get_ref().transaction.as_ref()
                    .ok_or_else(|| anyhow!("Coinbase tx not found"))?;
                let coinbase_data = coinbase_tx.inputs[0].signature_script.clone();
                let block_meta = BlockMeta {
                    num_txs: block.transaction_data.len() as u64,
                    size: block_info.size as u64,
                    coinbase_data,
                    median_time: block_info.median_time,
                };
                self.db.put_block_meta(&block_info.hash, &block_meta)?;
                block_meta
            }
        };
        Ok(BlockMetaInfo {
            block_info,
            block_meta,
        })
    }
}

impl Bchd {
    pub async fn block_txs(&self, block_hash: &[u8]) -> Result<Vec<(Vec<u8>, TxMeta)>> {
        use bchrpc::{GetBlockRequest, get_block_request::HashOrHeight, block::transaction_data::TxidsOrTxs};
        let mut bchd = self.client.clone();
        let block = bchd.get_block(GetBlockRequest {
            full_transactions: false,
            hash_or_height: Some(HashOrHeight::Hash(block_hash.to_vec()))
        }).await?;
        let block = block.get_ref().block.as_ref().ok_or_else(|| anyhow!("Block not found"))?;
        let mut tx_hashes = Vec::with_capacity(block.transaction_data.len());
        for tx in block.transaction_data.iter() {
            let tx_hash = tx.txids_or_txs.as_ref()
                .ok_or_else(|| anyhow!("No txs in block"))?;
            let tx_hash = match tx_hash {
                TxidsOrTxs::TransactionHash(hash) => hash,
                _ => unreachable!(),
            };
            tx_hashes.push(tx_hash);
        }
        let block_info = block.info.as_ref().ok_or_else(|| anyhow!("No block info"))?;
        let futures = tx_hashes
            .into_iter()
            .enumerate()
            .map(|(tx_idx, tx_hash)| async move {
                self.fetch_tx_meta(tx_idx == 0, block_info.height, tx_hash).await.map(|tx_meta| {
                    (tx_hash.to_vec(), tx_meta)
                })
            });
        let results = try_join_all(futures).await?;
        Ok(results)
    }

    async fn fetch_tx_meta(&self, is_coinbase: bool, block_height: i32, tx_hash: &[u8]) -> Result<TxMeta> {
        use bchrpc::{GetTransactionRequest};
        match self.db.tx_meta(&tx_hash)? {
            Some(tx_meta) => Ok(tx_meta),
            None => {
                let mut bchd = self.client.clone();
                let tx = bchd.get_transaction(GetTransactionRequest {
                    hash: tx_hash.to_vec(),
                    include_token_metadata: false,
                }).await?;
                let tx = tx.get_ref();
                let tx = tx.transaction.as_ref()
                    .ok_or_else(|| anyhow!("Tx not found"))?;
                let tx_meta = self.extract_tx_meta(is_coinbase, block_height, tx);
                self.db.put_tx_meta(&tx_hash, &tx_meta)?;
                Ok(tx_meta)
            }
        }
    }

    fn extract_tx_meta(&self, is_coinbase: bool, block_height: i32, tx: &bchrpc::Transaction) -> TxMeta {
        TxMeta {
            is_coinbase,
            block_height,
            num_inputs: tx.inputs.len() as u32,
            num_outputs: tx.outputs.len() as u32,
            sats_input: tx.inputs.iter().map(|input| input.value).sum(),
            sats_output: tx.outputs.iter().map(|output| output.value).sum(),
            size: tx.size,
            variant: self.tx_meta_variant(tx),
        }
    }

    fn tx_meta_variant(&self, tx: &bchrpc::Transaction) -> TxMetaVariant {
        use bchrpc::{slp_transaction_info::ValidityJudgement};
        match &tx.slp_transaction_info {
            Some(slp) => {
                let input_sum: u64 = tx.inputs
                    .iter()
                    .map(|input| input.slp_token.as_ref().map(|token| token.amount).unwrap_or_default())
                    .sum();
                let output_sum: u64 = tx.outputs
                    .iter()
                    .map(|output| output.slp_token.as_ref().map(|token| token.amount).unwrap_or_default())
                    .sum();
                if slp.validity_judgement() == ValidityJudgement::UnknownOrInvalid {
                    if input_sum == 0 {
                        return TxMetaVariant::Normal;
                    } else {
                        return TxMetaVariant::InvalidSlp {
                            token_id: slp.token_id.clone(),
                            token_input: input_sum,
                        };
                    }
                }
                TxMetaVariant::Slp {
                    action: {
                        use bchrpc::SlpAction::*;
                        match slp.slp_action() {
                            NonSlp => return TxMetaVariant::Normal,
                            NonSlpBurn | SlpParseError | SlpUnsupportedVersion => return TxMetaVariant::InvalidSlp {
                                token_id: slp.token_id.as_slice().try_into().unwrap(),
                                token_input: input_sum,
                            },
                            SlpV1Genesis => SlpAction::SlpV1Genesis,
                            SlpV1Mint => SlpAction::SlpV1Mint,
                            SlpV1Send => SlpAction::SlpV1Send,
                            SlpNft1GroupGenesis => SlpAction::SlpNft1GroupGenesis,
                            SlpNft1GroupMint => SlpAction::SlpNft1GroupMint,
                            SlpNft1GroupSend => SlpAction::SlpNft1GroupSend,
                            SlpNft1UniqueChildGenesis => SlpAction::SlpNft1UniqueChildGenesis,
                            SlpNft1UniqueChildSend => SlpAction::SlpNft1UniqueChildSend,
                        }
                    },
                    token_input: input_sum,
                    token_output: output_sum,
                    token_id: slp.token_id.as_slice().try_into().unwrap(),
                }
            }
            None => TxMetaVariant::Normal
        }
    }
}

impl Bchd {
    pub async fn tokens(&self, token_ids: impl ExactSizeIterator<Item=&[u8]>) -> Result<Vec<TokenMeta>> {
        use bchrpc::{GetTokenMetadataRequest, token_metadata::TypeMetadata};
        let mut result = vec![None; token_ids.len()];
        let mut tokens_to_fetch = Vec::new();
        for (idx, token_id) in token_ids.enumerate() {
            match self.db.token_meta(token_id)? {
                Some(token_meta) => result[idx] = Some(token_meta),
                None => tokens_to_fetch.push((idx, token_id)),
            }
        }
        if !tokens_to_fetch.is_empty() {
            let mut bchd = self.client.clone();
            let token_metadata = bchd.get_token_metadata(GetTokenMetadataRequest {
                token_ids: tokens_to_fetch.iter().map(|(_, token)| token.to_vec()).collect(),
            }).await?;
            let token_metadata = &token_metadata.get_ref().token_metadata;
            for (&(idx, token_id), token_metadata) in tokens_to_fetch.iter().zip(token_metadata) {
                let type_metadata =  token_metadata.type_metadata.as_ref().ok_or_else(|| anyhow!("No token metadata"))?;
                let token_meta = match type_metadata {
                    TypeMetadata::Type1(meta) => TokenMeta {
                        token_type: token_metadata.token_type,
                        token_ticker: meta.token_ticker.clone(),
                        token_name: meta.token_name.clone(),
                        token_document_url: meta.token_document_url.clone(),
                        token_document_hash: meta.token_document_hash.clone(),
                        decimals: meta.decimals,
                        group_id: None,
                    },
                    TypeMetadata::Nft1Group(meta) => TokenMeta {
                        token_type: token_metadata.token_type,
                        token_ticker: meta.token_ticker.clone(),
                        token_name: meta.token_name.clone(),
                        token_document_url: meta.token_document_url.clone(),
                        token_document_hash: meta.token_document_hash.clone(),
                        decimals: meta.decimals,
                        group_id: None,
                    },
                    TypeMetadata::Nft1Child(meta) => TokenMeta {
                        token_type: token_metadata.token_type,
                        token_ticker: meta.token_ticker.clone(),
                        token_name: meta.token_name.clone(),
                        token_document_url: meta.token_document_url.clone(),
                        token_document_hash: meta.token_document_hash.clone(),
                        decimals: 0,
                        group_id: Some(meta.group_id.as_slice().try_into()?),
                    },
                };
                self.db.put_token_meta(token_id, &token_meta)?;
                result[idx] = Some(token_meta);
            }
        }
        Ok(result.into_iter().map(|token_meta| token_meta.unwrap()).collect())
    }
}

pub struct Tx {
    pub transaction: bchrpc::Transaction,
    pub tx_meta: TxMeta,
    pub token_meta: Option<TokenMeta>,
    pub raw_tx: Vec<u8>,
    pub tx_out_spends: HashMap<u32, Option<TxOutSpend>>,
}

impl Bchd {
    pub async fn tx(&self, tx_hash: &[u8]) -> Result<Option<Tx>> {
        use bchrpc::{GetTransactionRequest, GetRawTransactionRequest};
        let mut bchd1 = self.client.clone();
        let mut bchd2= self.client.clone();
        let (tx, raw_tx) = tokio::try_join!(
            bchd1.get_transaction(GetTransactionRequest {
                hash: tx_hash.to_vec(),
                include_token_metadata: false,
            }),
            bchd2.get_raw_transaction(GetRawTransactionRequest {
                hash: tx_hash.to_vec(),
            }),
        )?;
        let tx = tx.get_ref();
        let tx = tx.transaction.as_ref().ok_or_else(|| anyhow!("No tx found"))?;
        let raw_tx = raw_tx.get_ref();
        let token_meta = match tx.slp_transaction_info.as_ref() {
            Some(slp_info) if !slp_info.token_id.is_empty() => {
                let tokens = self.tokens(std::iter::once(slp_info.token_id.as_slice())).await?;
                tokens.into_iter().next()
            }
            _ => None,
        };
        for input in &tx.inputs {
            let outpoint = input.outpoint.as_ref().ok_or_else(|| anyhow!("No outpoint"))?;
            self.db.put_tx_out_spend(
                &outpoint.hash,
                outpoint.index,
                &TxOutSpend {
                    by_tx_hash: tx_hash.try_into()?,
                    by_input_idx: input.index,
                },
            )?;
        }
        let tx_out_spends = self.fetch_tx_out_spends(&tx).await?;
        let is_coinbase = tx.inputs.get(0)
            .and_then(|input| input.outpoint.as_ref())
            .map(is_coinbase)
            .unwrap_or(false);
        let tx_meta = self.fetch_tx_meta(is_coinbase, tx.block_height, tx_hash).await?;
        Ok(Some(Tx {
            transaction: tx.clone(),
            tx_meta,
            token_meta,
            raw_tx: raw_tx.transaction.clone(),
            tx_out_spends,
        }))
    }

    async fn fetch_tx_out_spends(&self, tx: &bchrpc::Transaction) -> Result<HashMap<u32, Option<TxOutSpend>>> {
        let mut address_out_indices = HashMap::new();
        for output in &tx.outputs {
            if let Destination::Address(address) = destination_from_script(self.satoshi_addr_prefix, &output.pubkey_script) {
                let indices = address_out_indices.entry(address).or_insert(HashSet::new());
                indices.insert(output.index);
            }
        }
        let tx_out_spend_maps = try_join_all(address_out_indices.iter().map(|(address, tx_out_indices)| {
            self.fetch_tx_out_spend(&tx.hash, tx_out_indices.clone(), tx.block_height, address.cash_addr())
        })).await?;
        let mut result_map = HashMap::new();
        for tx_out_spend_map in tx_out_spend_maps {
            for (out_idx, spend) in tx_out_spend_map {
                result_map.insert(out_idx, spend);
            }
        }
        Ok(result_map)
    }

    async fn fetch_tx_out_spend(&self, tx_hash: &[u8], mut tx_out_indices: HashSet<u32>, height: i32, output_address: &str) -> Result<HashMap<u32, Option<TxOutSpend>>> {
        use bchrpc::{GetUnspentOutputRequest, GetAddressTransactionsRequest, get_address_transactions_request::StartBlock};
        let mut result_map = HashMap::new();
        for tx_out_idx in tx_out_indices.clone() {
            if let Some(tx_out_spend) = self.db.tx_out_spend(tx_hash, tx_out_idx)? {
                tx_out_indices.remove(&tx_out_idx);
                result_map.insert(tx_out_idx, Some(tx_out_spend));
            }
        }
        if tx_out_indices.is_empty() {
            return Ok(result_map);
        }
        let utxo_indices = try_join_all(tx_out_indices.iter().map(|&tx_out_idx | async move {
            let mut bchd = self.client.clone();
            match bchd.get_unspent_output(GetUnspentOutputRequest {
                hash: tx_hash.to_vec(),
                index: tx_out_idx,
                include_mempool: true,
                include_token_metadata: false,
            }).await {
                Ok(_) => return Result::<_, Status>::Ok(Some(tx_out_idx)),
                Err(status) => {
                    if status.message() != "utxo not found" && status.message() != "utxo spent in mempool" {
                        return Err(status.into())
                    }
                    return Ok(None)
                }
            }
        })).await?;
        for utxo_idx in utxo_indices {
            if let Some(utxo_idx) = utxo_idx {
                tx_out_indices.remove(&utxo_idx);
                result_map.insert(utxo_idx, None);
            }
        }
        if tx_out_indices.is_empty() {
            return Ok(result_map);
        }
        let mut num_skip = 0usize;
        let mut had_attempt = false;
        let num_batches = 10;
        let batch_size = 100usize;
        loop {
            let batches = try_join_all(
                (0..num_batches).into_iter().map(|batch_idx| async move {
                    let addr_txs = self.client.clone().get_address_transactions(GetAddressTransactionsRequest {
                        address: output_address.to_string(),
                        nb_skip: (num_skip + batch_idx * batch_size) as u32,
                        nb_fetch: 100,
                        start_block: Some(StartBlock::Height(height)),
                    }).await;
                    addr_txs.map(|resp| {
                        resp.get_ref().clone()
                    })
                })
            ).await?;
            for addr_txs in batches {
                if addr_txs.confirmed_transactions.is_empty() {
                    if tx_out_indices.is_empty() {
                        return Ok(result_map);
                    }
                    if had_attempt {
                        return Err(anyhow!("BCHD reports {}, outputs {:?} are spent but couldn't find tx spend", hex::encode(tx_hash), tx_out_indices));
                    }
                }
                had_attempt = true;
                num_skip += addr_txs.confirmed_transactions.len();
                println!("Searched through {} txs for {}", num_skip, output_address);
                let txs = addr_txs
                    .confirmed_transactions.iter()
                    .chain(
                        addr_txs.unconfirmed_transactions
                            .iter()
                            .filter_map(|tx| tx.transaction.as_ref())
                    );
                for tx in txs {
                    for input in &tx.inputs {
                        if let Some(outpoint) = &input.outpoint {
                            let tx_out_spend = TxOutSpend {
                                by_tx_hash: tx.hash.as_slice().try_into()?,
                                by_input_idx: input.index,
                            };
                            self.db.put_tx_out_spend(&outpoint.hash, outpoint.index, &tx_out_spend)?;
                            if outpoint.hash.as_slice() == tx_hash && tx_out_indices.remove(&outpoint.index) {
                                result_map.insert(outpoint.index, Some(tx_out_spend));
                            }
                        }
                    }
                }
            }
        }
    }
}

pub struct AddressTx {
    pub tx_hash: [u8; 32],
    pub timestamp: i64,
    pub block_height: Option<i32>,
    pub tx_meta: TxMeta,
    pub delta_sats: i64,
    pub delta_tokens: i64,
}

pub struct AddressTxs {
    pub txs: Vec<AddressTx>,
}

impl Bchd {
    pub async fn address(&self, sats_address: &Address<'_>) -> Result<AddressTxs> {
        use bchrpc::{GetAddressTransactionsRequest, get_address_transactions_request::StartBlock};
        let mut num_skip = 0usize;
        let mut addr_txs = Vec::new();
        let mut found_tx_hashes = HashSet::new();
        let db_txs = self.db.confirmed_address_txs(
            sats_address.addr_type() as u8,
            sats_address.hash().as_slice(),
        )?;
        let mut start_block = None::<i32>;
        for (tx_hash, confirmed_address_tx) in db_txs {
            addr_txs.push(AddressTx {
                tx_hash,
                timestamp: confirmed_address_tx.timestamp,
                block_height: Some(confirmed_address_tx.block_height),
                tx_meta: confirmed_address_tx.tx_meta,
                delta_sats: confirmed_address_tx.delta_sats,
                delta_tokens: confirmed_address_tx.delta_tokens,
            });
            found_tx_hashes.insert(tx_hash);
            let new_start_block = match start_block {
                Some(start_block) => start_block.max(confirmed_address_tx.block_height),
                None => confirmed_address_tx.block_height,
            };
            start_block = Some(new_start_block - 1);
        }
        let fetch_amount = 100;
        let num_batches = 10;
        loop {
            let batches = try_join_all(
                (0..num_batches).into_iter().map(|batch_idx| async move {
                    let addr_txs = self.client.clone().get_address_transactions(GetAddressTransactionsRequest {
                        address: sats_address.cash_addr().to_string(),
                        nb_skip: (num_skip + batch_idx * fetch_amount) as u32,
                        nb_fetch: fetch_amount as u32,
                        start_block: start_block.map(StartBlock::Height),
                    }).await;
                    addr_txs.map(|resp| {
                        resp.get_ref().clone()
                    })
                })
            ).await?;
            for batch_txs in batches {
                num_skip += batch_txs.confirmed_transactions.len();
                println!("fetched {} address txs", num_skip);
                for mempool_tx in &batch_txs.unconfirmed_transactions {
                    if let Some(tx) = &mempool_tx.transaction {
                        self.add_addr_txs(&mut found_tx_hashes, &mut addr_txs, tx, mempool_tx.added_time, None, sats_address).await?;
                    }
                }
                for tx in &batch_txs.confirmed_transactions {
                    self.add_addr_txs(&mut found_tx_hashes, &mut addr_txs, tx, tx.timestamp, Some(tx.block_height), sats_address).await?;
                }
                if batch_txs.confirmed_transactions.is_empty() {
                    addr_txs.sort_by_key(|tx| -tx.timestamp);
                    return Ok(AddressTxs { txs: addr_txs });
                }
            }
        }
    }

    async fn add_addr_txs(
        &self,
        found_tx_hashes: &mut HashSet<[u8; 32]>,
        addr_txs: &mut Vec<AddressTx>,
        tx: &bchrpc::Transaction,
        timestamp: i64,
        block_height: Option<i32>,
        sats_address: &Address<'_>,
    ) -> Result<()> {
        let tx_hash: [u8; 32] = tx.hash.as_slice().try_into().expect("Invalid tx hash");
        if !found_tx_hashes.contains(&tx_hash) {
            let is_coinbase = tx.inputs.get(0)
                .and_then(|input| input.outpoint.as_ref())
                .map(is_coinbase)
                .unwrap_or(false);
            let address_input = tx.inputs.iter()
                .filter_map(|input| {
                    let token_amount = if let Some(slp) = &input.slp_token {
                        slp.amount as i64
                    } else {
                        0
                    };
                    if let Destination::Address(addr) = destination_from_script(sats_address.prefix_str(), &input.previous_script) {
                        Some((input.value, token_amount)).filter(|_| addr.cash_addr() == sats_address.cash_addr())
                    } else {
                        None
                    }
                })
                .fold((0, 0), |(a_sats, a_tokens), (b_sats, b_tokens)| (a_sats + b_sats, a_tokens + b_tokens));
            let address_output = tx.outputs.iter()
                .filter_map(|output| {
                    let token_amount = if let Some(slp) = &output.slp_token {
                        slp.amount as i64
                    } else {
                        0
                    };
                    if let Destination::Address(addr) = destination_from_script(sats_address.prefix_str(), &output.pubkey_script) {
                        Some((output.value, token_amount)).filter(|_| addr.cash_addr() == sats_address.cash_addr())
                    } else {
                        None
                    }
                })
                .fold((0, 0), |(a_sats, a_tokens), (b_sats, b_tokens)| (a_sats + b_sats, a_tokens + b_tokens));
            let tx_meta = self.extract_tx_meta(is_coinbase, tx.block_height, &tx);
            let delta_sats = address_output.0 - address_input.0;
            let delta_tokens = address_output.1 - address_input.1;
            let tx_meta = if let Some(block_height) = block_height {
                let confirmed_address_tx = ConfirmedAddressTx {
                    timestamp,
                    block_height,
                    tx_meta,
                    delta_sats,
                    delta_tokens,
                };
                self.db.add_confirmed_address_tx(
                    sats_address.addr_type() as u8,
                    sats_address.hash().as_slice(),
                    &tx_hash,
                    &confirmed_address_tx,
                )?;
                confirmed_address_tx.tx_meta
            } else {
                tx_meta
            };
            addr_txs.push(AddressTx {
                timestamp,
                block_height,
                tx_hash,
                tx_meta,
                delta_sats,
                delta_tokens,
            });
            found_tx_hashes.insert(tx_hash);
        }
        Ok(())
    }

    pub async fn search(&self, query: &str) -> Result<Option<String>> {
        use bchrpc::{GetRawTransactionRequest, GetBlockInfoRequest, get_block_info_request::HashOrHeight};
        match Address::from_cash_addr(query) {
            Ok(address) => return Ok(Some(format!("/address/{}", address.cash_addr()))),
            _ => {},
        }
        let bytes = from_le_hex(query)?;
        let mut bchd = self.client.clone();
        match bchd.get_raw_transaction(GetRawTransactionRequest {
            hash: bytes.clone(),
        }).await {
            Ok(_) => return Ok(Some(format!("/tx/{}", query))),
            _ => {},
        }
        match bchd.get_block_info(GetBlockInfoRequest {
            hash_or_height: Some(HashOrHeight::Hash(bytes)),
        }).await {
            Ok(_) => return Ok(Some(format!("/block/{}", query))),
            _ => {}
        }
        Ok(None)
    }
}

pub struct Utxo {
    pub tx_hash: [u8; 32],
    pub out_idx: u32,
    pub sats_amount: i64,
    pub token_amount: u64,
    pub is_coinbase: bool,
    pub block_height: i32,
}

pub struct AddressBalance {
    pub utxos: HashMap<Option<[u8; 32]>, Vec<Utxo>>,
    pub balances: HashMap<Option<[u8; 32]>, (i64, u64)>,
}

impl Bchd {
    pub async fn address_balance(&self, sats_address: &Address<'_>) -> Result<AddressBalance> {
        use bchrpc::GetAddressUnspentOutputsRequest;
        let mut bchd = self.client.clone();
        let unspents = bchd.get_address_unspent_outputs(GetAddressUnspentOutputsRequest {
            address: sats_address.cash_addr().to_string(),
            include_mempool: true,
            include_token_metadata: false,
        }).await?;
        let unspents = unspents.get_ref();
        println!("address_balance: {}", unspents.outputs.len());
        let mut utxos = HashMap::new();
        let mut balances = HashMap::new();
        utxos.insert(None, vec![]);
        balances.insert(None, (0, 0));
        for output in unspents.outputs.iter() {
            let token_id: Option<[u8; 32]> = output.slp_token.as_ref().and_then(|slp| slp.token_id.as_slice().try_into().ok());
            let token_amount = output.slp_token.as_ref().map(|slp| slp.amount).unwrap_or(0);
            let token_utxos = utxos.entry(token_id).or_insert(vec![]);
            let outpoint = output.outpoint.as_ref().ok_or_else(|| anyhow!("No outpoint"))?;
            token_utxos.push(Utxo {
                tx_hash: outpoint.hash.as_slice().try_into()?,
                out_idx: outpoint.index,
                sats_amount: output.value,
                token_amount,
                block_height: output.block_height,
                is_coinbase: output.is_coinbase,
            });
            let (balance_sats, balance_token) = balances.entry(token_id).or_insert((0, 0));
            *balance_sats += output.value;
            *balance_token += token_amount;
        }
        Ok(AddressBalance { utxos, balances })
    }
}
