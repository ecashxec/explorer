use anyhow::{anyhow, Result};
use bchrpc::bchrpc_client::BchrpcClient;
use futures::future::try_join_all;
use tonic::transport::{Certificate, Channel, ClientTlsConfig, Endpoint};
use std::convert::TryInto;

pub mod bchrpc {
    tonic::include_proto!("pb");
}

use bchrpc::BlockInfo;

use crate::db::{BlockMeta, Db, SlpAction, TokenMeta, TxMeta, TxMetaVariant};

pub struct Bchd {
    client: BchrpcClient<Channel>,
    db: Db,
}

impl Bchd {
    pub async fn connect(db: Db) -> Result<Self> {
        use std::fs;
        use std::io::Read;
        let mut cert_file = fs::File::open("cert.crt")?;
        let mut cert = Vec::new();
        cert_file.read_to_end(&mut cert)?;
        let tls_config = ClientTlsConfig::new().ca_certificate(Certificate::from_pem(&cert));
        let endpoint = Endpoint::from_static("https://api2.be.cash:8345").tls_config(tls_config)?;
        let client = BchrpcClient::connect(endpoint).await?;
        Ok(Bchd { client, db })
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
            .map(|(tx_idx, tx_hash)| self.fetch_tx_meta(tx_idx, block_info, tx_hash));
        let results = try_join_all(futures).await?;
        Ok(results)
    }

    async fn fetch_tx_meta(&self, tx_idx: usize, block_info: &BlockInfo, tx_hash: &[u8]) -> Result<(Vec<u8>, TxMeta)> {
        use bchrpc::{GetTransactionRequest};
        match self.db.tx_meta(&tx_hash)? {
            Some(tx_meta) => Ok((tx_hash.to_vec(), tx_meta)),
            None => {
                let mut bchd = self.client.clone();
                let tx = bchd.get_transaction(GetTransactionRequest {
                    hash: tx_hash.to_vec(),
                    include_token_metadata: false,
                }).await?;
                let tx = tx.get_ref();
                let tx_data = tx.transaction.as_ref()
                    .ok_or_else(|| anyhow!("Tx not found"))?;
                let tx_meta = TxMeta {
                    is_coinbase: tx_idx == 0,
                    block_height: block_info.height,
                    num_inputs: tx_data.inputs.len() as u32,
                    num_outputs: tx_data.outputs.len() as u32,
                    sats_input: tx_data.inputs.iter().map(|input| input.value).sum(),
                    sats_output: tx_data.outputs.iter().map(|output| output.value).sum(),
                    size: tx_data.size,
                    variant: self.tx_meta_variant(tx_data),
                };
                self.db.put_tx_meta(&tx_hash, &tx_meta)?;
                Ok((tx_hash.to_vec(), tx_meta))
            }
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
                            token_id: slp.token_id.as_slice().try_into().unwrap(),
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
