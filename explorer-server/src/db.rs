use std::path::Path;

use anyhow::{anyhow, Result};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use sled::transaction::{
    ConflictableTransactionError, ConflictableTransactionResult, TransactionError,
    TransactionalTree,
};

pub struct Db {
    db: sled::Db,
}

#[derive(Serialize, Deserialize)]
pub struct BlockMeta {
    pub median_time: i64,
    pub size: u64,
    pub num_txs: u64,
    pub coinbase_data: Vec<u8>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct TxMeta {
    pub block_height: i32,
    pub is_coinbase: bool,
    pub size: i32,
    pub num_inputs: u32,
    pub num_outputs: u32,
    pub sats_input: i64,
    pub sats_output: i64,
    pub variant: TxMetaVariant,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum TxMetaVariant {
    Normal,
    Slp {
        action: SlpAction,
        token_input: u64,
        token_output: u64,
        token_id: [u8; 32],
    },
    InvalidSlp {
        token_id: [u8; 32],
        token_input: u64,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct TokenMeta {
    pub token_type: u32,
    pub token_ticker: Vec<u8>,
    pub token_name: Vec<u8>,
    pub token_document_url: Vec<u8>,
    pub token_document_hash: Vec<u8>,
    pub decimals: u32,
    pub group_id: Option<[u8; 32]>,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug)]
pub enum SlpAction {
    SlpV1Genesis,
    SlpV1Mint,
    SlpV1Send,
    SlpNft1GroupGenesis,
    SlpNft1GroupMint,
    SlpNft1GroupSend,
    SlpNft1UniqueChildGenesis,
    SlpNft1UniqueChildSend,
}

impl Db {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let db = sled::open(path.as_ref())?;
        Ok(Db { db })
    }

    pub fn block_meta(&self, block_hash: &[u8]) -> Result<Option<BlockMeta>> {
        let block_meta_key = [b"blk:".as_ref(), block_hash].concat();
        db_get_option(&self.db, &block_meta_key)
    }

    pub fn put_block_meta(&self, block_hash: &[u8], block_meta: &BlockMeta) -> Result<()> {
        let block_meta_key = [b"blk:".as_ref(), block_hash].concat();
        let block_meta = bincode::serialize(block_meta)?;
        self.db.insert(block_meta_key, block_meta)?;
        Ok(())
    }

    pub fn tx_meta(&self, tx_hash: &[u8]) -> Result<Option<TxMeta>> {
        let tx_meta_key = [b"tx:".as_ref(), tx_hash].concat();
        db_get_option(&self.db, &tx_meta_key)
    }

    pub fn put_tx_meta(&self, tx_hash: &[u8], tx_meta: &TxMeta) -> Result<()> {
        let tx_meta_key = [b"tx:".as_ref(), tx_hash].concat();
        let tx_meta = bincode::serialize(tx_meta)?;
        self.db.insert(tx_meta_key, tx_meta)?;
        Ok(())
    }

    pub fn token_meta(&self, token_id: &[u8]) -> Result<Option<TokenMeta>> {
        let token_meta_key = [b"token:".as_ref(), token_id].concat();
        db_get_option(&self.db, &token_meta_key)
    }

    pub fn put_token_meta(&self, token_id: &[u8], token_meta: &TokenMeta) -> Result<()> {
        let token_meta_key = [b"token:".as_ref(), token_id].concat();
        let token_meta = bincode::serialize(token_meta)?;
        self.db.insert(token_meta_key, token_meta)?;
        Ok(())
    }
}

fn _db_get<T: DeserializeOwned>(db: &sled::Db, key: &[u8]) -> Result<T> {
    let item = db
        .get(key)?
        .ok_or_else(|| anyhow!("No entry for {}", String::from_utf8_lossy(key)))?;
    Ok(bincode::deserialize(&item)?)
}

fn db_get_option<T: DeserializeOwned>(db: &sled::Db, key: &[u8]) -> Result<Option<T>> {
    let item = db.get(key)?;
    Ok(item
        .map(|item| bincode::deserialize(&item))
        .transpose()?)
}

fn _db_tx_get<T: DeserializeOwned>(
    db: &TransactionalTree,
    key: &[u8],
) -> ConflictableTransactionResult<T, anyhow::Error> {
    _db_tx_get_option(db, key).and_then(|item| {
        item.ok_or_else(|| abort_tx(anyhow!("No entry for {}", String::from_utf8_lossy(key))))
    })
}

fn _db_tx_get_option<T: DeserializeOwned>(
    db: &TransactionalTree,
    key: &[u8],
) -> ConflictableTransactionResult<Option<T>, anyhow::Error> {
    let item = db.get(key)?;
    Ok(item
        .map(|item| bincode::deserialize(&item).map_err(abort_tx))
        .transpose()?)
}

fn abort_tx(err: impl Into<anyhow::Error>) -> ConflictableTransactionError<anyhow::Error> {
    ConflictableTransactionError::Abort(err.into())
}

fn tx_error(err: TransactionError<anyhow::Error>) -> anyhow::Error {
    match err {
        TransactionError::Abort(err) => {
            eprintln!("{}", err.to_string().as_str());
            err
        }
        TransactionError::Storage(err) => {
            eprintln!("Storage error: {}", err);
            err.into()
        }
    }
}
