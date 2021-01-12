use std::{collections::HashMap, convert::TryInto, sync::{Arc, atomic::{AtomicUsize, Ordering}}, time::Instant};

use anyhow::{Result, anyhow, bail};
use tonic::transport::{Certificate, Channel, ClientTlsConfig, Endpoint};
use tokio::sync::mpsc;
use crate::{grpc::bchrpc, indexdb::{BlockBatches, IndexDb, TxOutSpend}, primitives::{TokenMeta, TxMeta}};
use crate::grpc::bchrpc::bchrpc_client::BchrpcClient;

pub struct Indexer {
    db: IndexDb,
    bchd: BchrpcClient<Channel>,
}

pub struct Tx {
    pub transaction: bchrpc::Transaction,
    pub tx_meta: TxMeta,
    pub token_meta: Option<TokenMeta>,
    pub raw_tx: Vec<u8>,
    pub tx_out_spends: HashMap<u32, Option<TxOutSpend>>,
}

impl Indexer {
    pub async fn connect(db: IndexDb) -> Result<Self> {
        use std::fs;
        use std::io::Read;
        let mut cert_file = fs::File::open("cert.crt")?;
        let mut cert = Vec::new();
        cert_file.read_to_end(&mut cert)?;
        let tls_config = ClientTlsConfig::new().ca_certificate(Certificate::from_pem(&cert));
        let endpoint = Endpoint::from_static("https://api2.be.cash:8445").tls_config(tls_config)?;
        let bchd = BchrpcClient::connect(endpoint).await?;
        Ok(Indexer { bchd, db })
    }

    pub fn db(&self) -> &IndexDb {
        &self.db
    }

    pub async fn block_txs(&self, block_hash: &[u8]) -> Result<Vec<([u8; 32], TxMeta)>> {
        use bchrpc::{GetBlockRequest, get_block_request::HashOrHeight, block::transaction_data::TxidsOrTxs};
        let mut bchd = self.bchd.clone();
        let block = bchd.get_block(GetBlockRequest {
            full_transactions: false,
            hash_or_height: Some(HashOrHeight::Hash(block_hash.to_vec()))
        }).await?;
        let block = block.get_ref().block.as_ref().ok_or_else(|| anyhow!("Block not found"))?;
        let txs = block.transaction_data.iter().map(|tx_data| -> Result<_> {
            match &tx_data.txids_or_txs {
                Some(TxidsOrTxs::TransactionHash(tx_hash)) => {
                    let tx_hash: [u8; 32] = tx_hash.as_slice().try_into()?;
                    let tx_meta = self.db().tx_meta(&tx_hash)?.ok_or_else(|| anyhow!("Unindexed txs"))?;
                    Ok((tx_hash, tx_meta))
                }
                _ => bail!("Invalid tx hash"),
            }
        }).collect::<Result<Vec<_>, _>>()?;
        Ok(txs)
    }

    pub async fn tx(&self, tx_hash: &[u8]) -> Result<Tx> {
        use bchrpc::{GetTransactionRequest, GetRawTransactionRequest};
        let mut bchd1 = self.bchd.clone();
        let mut bchd2 = self.bchd.clone();
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
        let tx_meta = self.db.tx_meta(tx_hash)?.ok_or_else(|| anyhow!("No tx meta for tx"))?;
        let tx_out_spends = self.db.tx_out_spends(tx_hash)?;
        let token_meta = match tx.slp_transaction_info.as_ref() {
            Some(slp_info) if !slp_info.token_id.is_empty() => {
                self.db.token_meta(&slp_info.token_id)?
            }
            _ => None,
        };
        Ok(Tx {
            transaction: tx.clone(),
            tx_meta,
            token_meta,
            raw_tx: raw_tx.transaction.clone(),
            tx_out_spends,
        })
    }

    pub async fn run_indexer(self: Arc<Self>) {
        match self.run_indexer_inner().await {
            Ok(()) => {},
            Err(err) => eprintln!("Index error: {}", err),
        }
    }

    async fn run_indexer_inner(self: Arc<Self>) -> Result<()> {
        let last_height = self.db.last_block_height().unwrap() as usize;
        let current_height = Arc::new(AtomicUsize::new(last_height));
        let num_threads = 100;
        let (send_batches, mut receive_batches) = mpsc::channel(num_threads * 2);
        let mut join_handles = Vec::with_capacity(num_threads);
        for _ in 0..num_threads {
            let indexer = Arc::clone(&self);
            let current_height = Arc::clone(&current_height);
            let send_batches = send_batches.clone();
            let join_handle = tokio::spawn(async move {
                indexer.index_thread(current_height, send_batches).await
            });
            join_handles.push(join_handle);
        }
        std::mem::drop(send_batches);
        let mut current_height = last_height;
        let mut block_shelf = HashMap::new();
        let mut last_update_time = Instant::now();
        let mut last_update_blocks = 0;
        while let Some(block_batches) = receive_batches.recv().await {
            block_shelf.insert(block_batches.block_height as usize, block_batches);
            while block_shelf.contains_key(&current_height) {
                let block_batches = block_shelf.remove(&current_height).unwrap();
                self.db.apply_block_batches(&block_batches)?;
                last_update_blocks += 1;
                let elapsed = last_update_time.elapsed().as_millis();
                if elapsed > 10_000 {
                    println!(
                        "Added {} blocks in {:.1}s, to block height {}",
                        last_update_blocks, elapsed as f64 / 1000.0, current_height,
                    );
                    let flush_start = Instant::now();
                    self.db.flush_async().await?;
                    println!("Flush took {:.2}s", flush_start.elapsed().as_secs_f64());
                    last_update_blocks = 0;
                    last_update_time = Instant::now();
                }
                current_height += 1;
            }
        }
        for handle in join_handles {
            handle.await??;
        }
        Ok(())
    }

    async fn index_thread(&self, current_height: Arc<AtomicUsize>, mut send_batches: mpsc::Sender<BlockBatches>) -> Result<()> {
        use bchrpc::{GetBlockRequest, get_block_request::HashOrHeight};
        let mut bchd = self.bchd.clone();
        loop {
            let block_height = current_height.fetch_add(1, Ordering::SeqCst);
            let result = bchd.get_block(GetBlockRequest {
                full_transactions: true,
                hash_or_height: Some(HashOrHeight::Height(block_height as i32)),
            }).await;
            match result {
                Ok(block) => {
                    if let Some(block) = &block.get_ref().block {
                        let batches = IndexDb::make_block_batches(block)?;
                        send_batches.send(batches).await?;
                    }
                }
                Err(err) => {
                    println!("Error message ({}): {}", block_height, err.message());
                    println!("Error detail ({}): {}", block_height, String::from_utf8_lossy(&err.details()));
                    return Ok(());
                }
            }
        }
    }
}