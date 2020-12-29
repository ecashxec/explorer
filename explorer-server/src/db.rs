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

impl Db {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let db = sled::open(path.as_ref())?;
        Ok(Db { db })
    }

    pub fn block_meta(&self, block_hash: &[u8]) -> Result<Option<BlockMeta>> {
        let block_meta_key = [
            b"blk:".as_ref(),
            block_hash,
        ].concat();
        db_get_option(&self.db, &block_meta_key)
    }

    pub fn put_block_meta(&self, block_hash: &[u8], block_meta: &BlockMeta) -> Result<()> {
        let block_meta_key = [
            b"blk:".as_ref(),
            block_hash,
        ].concat();
        let block_meta = bincode::serialize(block_meta)?;
        self.db.insert(block_meta_key, block_meta)?;
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
