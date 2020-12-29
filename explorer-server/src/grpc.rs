use anyhow::{anyhow, Result};
use bchrpc::bchrpc_client::BchrpcClient;
use futures::future::try_join_all;
use tonic::transport::{Certificate, Channel, ClientTlsConfig, Endpoint};

pub mod bchrpc {
    tonic::include_proto!("pb");
}

use bchrpc::BlockInfo;

use crate::db::{BlockMeta, Db};

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
        let futures = block_infos.into_iter().map(|block_info| self.fetch_meta_info(block_info));
        let results = try_join_all(futures).await?;
        Ok(results)
    }

    async fn fetch_meta_info(&self, block_info: BlockInfo) -> Result<BlockMetaInfo> {
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
