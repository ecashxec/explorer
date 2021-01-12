use std::{collections::HashMap, path::Path};
use std::convert::TryInto;

use anyhow::{Result, anyhow, bail};
use serde::de::DeserializeOwned;
use sled::{Batch, transaction::{
    ConflictableTransactionError, ConflictableTransactionResult, TransactionError, Transactional,
    TransactionalTree,
}};
use sled::Tree;
use zerocopy::{AsBytes, FromBytes, U32, Unaligned};
use bitcoin_cash::{Address, Hashed};
use byteorder::BE;

use crate::{blockchain::{Destination, destination_from_script, is_coinbase}, grpc::bchrpc, primitives::{AddressTx, BlockMeta, SlpAction, TokenMeta, Tx, TxInput, TxMeta, TxMetaVariant, TxOutput, Utxo}};

pub struct IndexDb {
    db: sled::Db,
    db_block_height_idx: Tree,
    db_block_meta: Tree,
    db_tx_meta: Tree,
    db_addr_tx_meta: Tree,
    db_addr_utxo_set: Tree,
    db_utxo_set: Tree,
    db_tx_out_spend: Tree,
    db_token_meta: Tree,
}

#[derive(Debug, Clone)]
pub struct BlockBatches {
    pub block_height: i32,
    batch_block_height_idx: Batch,
    batch_block_meta: Batch,
    batch_tx_meta: Batch,
    batch_addr_tx_meta: Batch,
    batch_addr_utxo_set: Batch,
    batch_utxo_set: Batch,
    batch_tx_out_spend: Batch,
    batch_token_meta: Batch,
}

#[derive(FromBytes, AsBytes, Unaligned, Debug, Default, Clone, Copy)]
#[repr(C)]
pub struct AddrTxKey {
    pub addr_type: u8,
    pub addr_hash: [u8; 20],
    pub block_height: U32<BE>,
    pub tx_hash: [u8; 32],
}

#[derive(FromBytes, AsBytes, Unaligned, Debug, Default, Clone, Copy)]
#[repr(C)]
pub struct AddrKeyPrefix {
    pub addr_type: u8,
    pub addr_hash: [u8; 20],
}

#[derive(FromBytes, AsBytes, Unaligned, Debug, Default, Clone, Copy)]
#[repr(C)]
pub struct UtxoKey {
    pub tx_hash: [u8; 32],
    pub out_idx: U32<BE>,
}

#[derive(FromBytes, AsBytes, Unaligned, Debug, Default, Clone, Copy)]
#[repr(C)]
pub struct AddrUtxoKey {
    pub addr: AddrKeyPrefix,
    pub utxo_key: UtxoKey,
}

#[derive(FromBytes, AsBytes, Unaligned, Debug, Default)]
#[repr(C)]
pub struct TxOutSpend {
    pub by_tx_hash: [u8; 32],
    pub by_tx_input_idx: U32<BE>,
}

pub struct AddressBalance {
    pub utxos: HashMap<Option<[u8; 32]>, Vec<(UtxoKey, Utxo)>>,
    pub balances: HashMap<Option<[u8; 32]>, (i64, u64)>,
}

impl IndexDb {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let db = sled::open(path.as_ref())?;
        let db_block_height_idx = db.open_tree(b"block_height_idx")?;
        let db_block_meta = db.open_tree(b"block_meta")?;
        let db_tx_meta = db.open_tree(b"tx_meta")?;
        let db_addr_tx_meta = db.open_tree(b"addr_tx_meta")?;
        let db_addr_utxo_set = db.open_tree(b"addr_utxo")?;
        let db_utxo_set = db.open_tree(b"utxo_set")?;
        let db_tx_out_spend = db.open_tree(b"tx_out_spend")?;
        let db_token_meta = db.open_tree(b"token_meta")?;
        Ok(IndexDb {
            db,
            db_block_height_idx,
            db_block_meta,
            db_tx_meta,
            db_addr_tx_meta,
            db_addr_utxo_set,
            db_utxo_set,
            db_tx_out_spend,
            db_token_meta,
        })
    }

    pub fn last_block_height(&self) -> Result<u32> {
        match self.db_block_height_idx.iter().last() {
            Some(pair) => {
                let (key, _) = pair?;
                let key: [u8; 4] = key[..].try_into()?;
                let height = u32::from_be_bytes(key);
                Ok(height)
            }
            None => Ok(0)
        }
    }

    pub fn block_range(&self, start_height: u32, num_blocks: u32) -> Result<Vec<([u8; 32], BlockMeta)>> {
        let start_key = start_height.to_be_bytes();
        let end_key = (start_height + num_blocks).to_be_bytes();
        let mut block_metas = Vec::with_capacity(num_blocks as usize);
        for pair in self.db_block_height_idx.range(start_key.as_ref()..end_key.as_ref()) {
            let (_, hash) = pair?;
            let hash: [u8; 32] = hash[..].try_into()?;
            let block_meta: BlockMeta = db_get(&self.db_block_meta, &hash)?;
            block_metas.push((hash, block_meta));
        }
        Ok(block_metas)
    }

    pub fn block_meta(&self, block_hash: &[u8]) -> Result<Option<BlockMeta>> {
        db_get_option(&self.db_block_meta, block_hash)
    }

    pub fn tx_meta(&self, tx_hash: &[u8]) -> Result<Option<TxMeta>> {
        db_get_option(&self.db_tx_meta, tx_hash)
    }

    pub fn token_meta(&self, token_id: &[u8]) -> Result<Option<TokenMeta>> {
        db_get_option(&self.db_token_meta, token_id)
    }

    pub fn tx_out_spends(&self, tx_hash: &[u8]) -> Result<HashMap<u32, Option<TxOutSpend>>> {
        let mut spends = HashMap::new();
        for pair in self.db_tx_out_spend.scan_prefix(tx_hash) {
            let (key, value) = pair?;
            let mut utxo_key = UtxoKey::default();
            utxo_key.as_bytes_mut().copy_from_slice(&key);
            let mut tx_out_spend = TxOutSpend::default();
            tx_out_spend.as_bytes_mut().copy_from_slice(&value);
            spends.insert(utxo_key.out_idx.get(), Some(tx_out_spend));
        }
        for pair in self.db_utxo_set.scan_prefix(tx_hash) {
            let (key, _) = pair?;
            let mut utxo_key = UtxoKey::default();
            utxo_key.as_bytes_mut().copy_from_slice(&key);
            spends.insert(utxo_key.out_idx.get(), None);
        }
        Ok(spends)
    }

    pub fn address(&self, address: &Address<'_>, skip: usize, take: usize) -> Result<Vec<([u8; 32], AddressTx, TxMeta)>> {
        let addr_prefix = AddrKeyPrefix {
            addr_type: address.addr_type() as u8,
            addr_hash: address.hash().as_slice().try_into().unwrap(),
        };
        let mut entries = Vec::new();
        for pair in self.db_addr_tx_meta.scan_prefix(addr_prefix.as_bytes()).rev().skip(skip).take(take) {
            let (key, value) = pair?;
            let mut addr_tx_key = AddrTxKey::default();
            addr_tx_key.as_bytes_mut().copy_from_slice(&key);
            let address_tx: AddressTx = bincode::deserialize(&value)?;
            let tx_meta = self.tx_meta(&addr_tx_key.tx_hash)?.ok_or_else(|| anyhow!("No tx meta"))?;
            entries.push((addr_tx_key.tx_hash, address_tx, tx_meta));
        }
        Ok(entries)
    }

    pub fn utxo(&self, utxo_key: &UtxoKey) -> Result<Option<Utxo>> {
        db_get_option(&self.db_utxo_set, utxo_key.as_bytes())
    }

    pub fn address_balance(&self, sats_address: &Address<'_>, skip: usize, take: usize) -> Result<AddressBalance> {
        let mut utxos = HashMap::new();
        let mut balances = HashMap::new();
        let addr_prefix = AddrKeyPrefix {
            addr_type: sats_address.addr_type() as u8,
            addr_hash: sats_address.hash().as_slice().try_into().unwrap(),
        };
        utxos.insert(None, vec![]);
        balances.insert(None, (0, 0));
        for pair in self.db_addr_utxo_set.scan_prefix(addr_prefix.as_bytes()) {
            let (key, _) = pair?;
            let mut addr_utxo_key = AddrUtxoKey::default();
            addr_utxo_key.as_bytes_mut().copy_from_slice(&key);
            let utxo = self.utxo(&addr_utxo_key.utxo_key)?.ok_or_else(|| anyhow!("No utxo"))?;
            let token_utxos = utxos.entry(utxo.token_id).or_insert(vec![]);
            let (balance_sats, balance_token) = balances.entry(utxo.token_id).or_insert((0, 0));
            *balance_sats += utxo.sats_amount;
            *balance_token += utxo.token_amount;
            token_utxos.push((addr_utxo_key.utxo_key, utxo));
        }
        Ok(AddressBalance { utxos, balances })
    }

    pub fn apply_block_batches(&self, block_batches: &BlockBatches) -> Result<()> {
        (
            &self.db_block_meta,
            &self.db_block_height_idx,
            &self.db_tx_meta,
            &self.db_addr_tx_meta,
            &self.db_addr_utxo_set,
            &self.db_utxo_set,
            &self.db_tx_out_spend,
            &self.db_token_meta,
        ).transaction(|(db_block_meta,
                        db_block_height_idx,
                        db_tx_meta,
                        db_addr_tx_meta,
                        db_addr_utxo_set,
                        db_utxo_set,
                        db_tx_out_spend,
                        db_token_meta)| {
            db_block_meta.apply_batch(&block_batches.batch_block_meta).map_err(abort_tx)?;
            db_block_height_idx.apply_batch(&block_batches.batch_block_height_idx).map_err(abort_tx)?;
            db_tx_meta.apply_batch(&block_batches.batch_tx_meta).map_err(abort_tx)?;
            db_addr_tx_meta.apply_batch(&block_batches.batch_addr_tx_meta).map_err(abort_tx)?;
            db_addr_utxo_set.apply_batch(&block_batches.batch_addr_utxo_set).map_err(abort_tx)?;
            db_utxo_set.apply_batch(&block_batches.batch_utxo_set).map_err(abort_tx)?;
            db_tx_out_spend.apply_batch(&block_batches.batch_tx_out_spend).map_err(abort_tx)?;
            db_token_meta.apply_batch(&block_batches.batch_token_meta).map_err(abort_tx)?;
            Ok(())
        }).map_err(tx_error)
    }

    pub fn make_block_batches(block: &bchrpc::Block) -> Result<BlockBatches> {
        use bchrpc::block::transaction_data::TxidsOrTxs;
        let block_info = block.info.as_ref().ok_or_else(|| anyhow!("No block info"))?;
        let txs = block.transaction_data.iter()
            .map(|tx_data| {
                match &tx_data.txids_or_txs {
                    Some(TxidsOrTxs::Transaction(tx)) => Ok(tx),
                    _ => bail!("Invalid tx in handle_block"),
                }
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(BlockBatches {
            block_height: block_info.height,
            batch_block_height_idx: Self::make_batch_block_height_idx(block_info),
            batch_block_meta: Self::make_batch_block_meta(block_info, &txs)?,
            batch_tx_meta: Self::make_batch_tx_meta(&txs)?,
            batch_addr_tx_meta: Self::make_batch_addr_tx_meta(&txs)?,
            batch_addr_utxo_set: Self::make_batch_addr_utxo_set(&txs)?,
            batch_utxo_set: Self::make_batch_utxo_set(&txs)?,
            batch_tx_out_spend: Self::make_batch_tx_out_spend(&txs)?,
            batch_token_meta: Self::make_batch_token_meta(&txs)?,
        })
    }

    fn make_batch_block_height_idx(block_info: &bchrpc::BlockInfo) -> Batch {
        let mut batch = Batch::default();
        Self::add_block_height_idx(&mut batch, block_info);
        batch
    }

    fn make_batch_block_meta(block_info: &bchrpc::BlockInfo, txs: &[&bchrpc::Transaction]) -> Result<Batch> {
        let mut batch = Batch::default();
        Self::add_block_meta(&mut batch, block_info, &txs)?;
        Ok(batch)
    }

    fn make_batch_tx_meta(txs: &[&bchrpc::Transaction]) -> Result<Batch> {
        let mut batch = Batch::default();
        for tx in txs {
            Self::add_tx_meta(&mut batch, tx)?;
        }
        Ok(batch)
    }

    fn make_batch_addr_tx_meta(txs: &[&bchrpc::Transaction]) -> Result<Batch> {
        let mut batch = Batch::default();
        for tx in txs {
            Self::add_addr_tx_meta(&mut batch, tx)?;
        }
        Ok(batch)
    }

    fn make_batch_addr_utxo_set(txs: &[&bchrpc::Transaction]) -> Result<Batch> {
        let mut batch = Batch::default();
        Self::update_addr_utxo_set(&mut batch, txs)?;
        Ok(batch)
    }

    fn make_batch_utxo_set(txs: &[&bchrpc::Transaction]) -> Result<Batch> {
        let mut batch = Batch::default();
        Self::update_utxo_set(&mut batch, txs)?;
        Ok(batch)
    }

    fn make_batch_tx_out_spend(txs: &[&bchrpc::Transaction]) -> Result<Batch> {
        let mut batch = Batch::default();
        for tx in txs {
            Self::add_tx_out_spend(&mut batch, tx)?;
        }
        Ok(batch)
    }

    fn make_batch_token_meta(txs: &[&bchrpc::Transaction]) -> Result<Batch> {
        let mut batch = Batch::default();
        for tx in txs {
            Self::add_token_meta(&mut batch, tx)?;
        }
        Ok(batch)
    }

    pub fn flush(&self) -> Result<()> {
        self.db.flush()?;
        Ok(())
    }

    pub async fn flush_async(&self) -> Result<()> {
        self.db.flush_async().await?;
        Ok(())
    }

    fn add_block_height_idx(batch_block_height_idx: &mut Batch, block_info: &bchrpc::BlockInfo) {
        let block_height = block_info.height as u32;
        batch_block_height_idx.insert(block_height.to_be_bytes().as_ref(), block_info.hash.clone());
    }

    fn add_block_meta(batch_block_meta: &mut Batch, block_info: &bchrpc::BlockInfo, txs: &[&bchrpc::Transaction]) -> Result<()> {
        let mut total_sats_input = 0;
        let mut total_sats_output = 0;
        for tx in txs {
            for input in &tx.inputs {
                total_sats_input += input.value;
            }
            for output in &tx.outputs {
                total_sats_output += output.value;
            }
        }
        let coinbase_data = txs[0].inputs[0].signature_script.clone();
        let block_meta = BlockMeta {
            height: block_info.height,
        
            version: block_info.version,
            previous_block: block_info.previous_block.as_slice().try_into()?,
            merkle_root: block_info.merkle_root.as_slice().try_into()?,
            timestamp: block_info.timestamp,
            bits: block_info.bits,
            nonce: block_info.nonce,
        
            total_sats_input,
            total_sats_output,
        
            difficulty: block_info.difficulty,
            median_time: block_info.median_time,
            size: block_info.size as u64,
            num_txs: txs.len() as u64,
            coinbase_data,
        };
        batch_block_meta.insert(block_info.hash.clone(), bincode::serialize(&block_meta)?);
        Ok(())
    }

    fn add_tx_meta(batch_tx_meta: &mut Batch, tx: &bchrpc::Transaction) -> Result<()> {
        let outpoint = tx.inputs.get(0).ok_or_else(|| anyhow!("No input"))?.outpoint.as_ref().ok_or_else(|| anyhow!("No outpoint"))?;
        let tx_meta = TxMeta {
            block_height: tx.block_height,
            timestamp: tx.timestamp,
            is_coinbase: is_coinbase(outpoint),
            size: tx.size,
            num_inputs: tx.inputs.len() as u32,
            num_outputs: tx.outputs.len() as u32,
            sats_input: tx.inputs.iter().map(|input| input.value).sum(),
            sats_output: tx.outputs.iter().map(|output| output.value).sum(),
            variant: Self::tx_meta_variant(tx),
        };
        batch_tx_meta.insert(tx.hash.as_slice(), bincode::serialize(&tx_meta)?);
        Ok(())
    }

    fn tx_meta_variant(tx: &bchrpc::Transaction) -> TxMetaVariant {
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
                        return TxMetaVariant::SatsOnly;
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
                            NonSlp => return TxMetaVariant::SatsOnly,
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
            None => TxMetaVariant::SatsOnly
        }
    }

    pub fn extract_tx(tx: &bchrpc::Transaction) -> Result<Tx> {
        let inputs = tx.inputs.iter()
            .map(|input| -> Result<_> {
                let outpoint = input.outpoint.as_ref().ok_or_else(|| anyhow!("No outpoint"))?;
                let slp_token = input.slp_token.as_ref();
                Ok(TxInput {
                    outpoint_tx_hash: outpoint.hash.as_slice().try_into()?,
                    outpoint_out_idx: outpoint.index,
                    signature_script: input.signature_script.clone(),
                    sequence: input.sequence,
                    sats_value: input.value,
                    previous_script: input.previous_script.clone(),
                
                    token_value: slp_token.map(|slp| slp.amount).unwrap_or(0),
                    is_mint_baton: slp_token.map(|slp| slp.is_mint_baton).unwrap_or(false),
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        let outputs = tx.outputs.iter()
            .map(|output| -> Result<_> {
                let slp_token = output.slp_token.as_ref();
                Ok(TxOutput {
                    sats_value: output.value,
                    pubkey_script: output.pubkey_script.clone(),
                    token_value: slp_token.map(|slp| slp.amount).unwrap_or(0),
                    is_mint_baton: slp_token.map(|slp| slp.is_mint_baton).unwrap_or(false),
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        let tx = Tx {
            version: tx.version,
            inputs,
            outputs,
            lock_time: tx.lock_time,
        
            size: tx.size as u64,
            timestamp: tx.timestamp,
            block_height: tx.block_height,
            block_hash: tx.block_hash.as_slice().try_into()?,
        };
        Ok(tx)
    }

    pub fn add_tx(&self, tx_hash: &[u8], tx: &Tx) -> Result<()> {
        let db_tx = self.db.open_tree(b"tx")?;
        db_tx.insert(tx_hash, bincode::serialize(&tx)?)?;
        Ok(())
    }

    fn add_addr_tx_meta(batch_addr_tx_meta: &mut Batch, tx: &bchrpc::Transaction) -> Result<()> {
        let mut address_delta = HashMap::new();
        for input in &tx.inputs {
            let (delta_sats, delta_tokens) = address_delta.entry(input.previous_script.as_slice()).or_default();
            *delta_sats += input.value;
            if let Some(slp) = &input.slp_token {
                *delta_tokens += slp.amount as i64;
            }
        }
        for output in &tx.outputs {
            let (delta_sats, delta_tokens) = address_delta.entry(output.pubkey_script.as_slice()).or_default();
            *delta_sats -= output.value;
            if let Some(slp) = &output.slp_token {
                *delta_tokens -= slp.amount as i64;
            }
        }
        for (pubkey_script, (delta_sats, delta_tokens)) in address_delta {
            let destination = destination_from_script("abc", pubkey_script);
            if let Destination::Address(address) = destination {
                let addr_tx_key = AddrTxKey {
                    addr_type: address.addr_type() as u8,
                    addr_hash: address.hash().as_slice().try_into()?,
                    block_height: U32::new(tx.block_height as u32),
                    tx_hash: tx.hash.as_slice().try_into()?,
                };
                let addr_tx = AddressTx {
                    timestamp: tx.timestamp,
                    block_height: tx.block_height,
                    delta_sats,
                    delta_tokens,
                };
             batch_addr_tx_meta.insert(addr_tx_key.as_bytes(), bincode::serialize(&addr_tx)?);
            }
        }
        Ok(())
    }

    fn update_utxo_set(batch_utxo_set: &mut Batch, txs: &[&bchrpc::Transaction]) -> Result<()> {
        for tx in txs {
            let tx_hash: [u8; 32] = tx.hash.as_slice().try_into()?;
            let token_id: Option<[u8; 32]> = match &tx.slp_transaction_info {
                Some(slp) if !slp.token_id.is_empty() => Some(slp.token_id[..].try_into()?),
                _ => None,
            };
            let outpoint = tx.inputs.get(0).ok_or_else(|| anyhow!("No input"))?.outpoint.as_ref().ok_or_else(|| anyhow!("No outpoint"))?;
            for (out_idx, output) in tx.outputs.iter().enumerate() {
                let utxo_key = UtxoKey {
                    tx_hash,
                    out_idx: U32::new(out_idx as u32),
                };
                let utxo = Utxo {
                    sats_amount: output.value,
                    token_amount: output.slp_token.as_ref().map(|slp| slp.amount).unwrap_or(0),
                    is_coinbase: is_coinbase(outpoint),
                    block_height: tx.block_height,
                    token_id,
                };
                batch_utxo_set.insert(utxo_key.as_bytes(), bincode::serialize(&utxo)?);
            }
        }
        for tx in txs {
            for input in &tx.inputs {
                if let Some(outpoint) = &input.outpoint {
                    let utxo_key = UtxoKey {
                        tx_hash: outpoint.hash.as_slice().try_into()?,
                        out_idx: U32::new(outpoint.index),
                    };
                    batch_utxo_set.remove(utxo_key.as_bytes());
                }
            }
        }
        Ok(())
    }

    fn update_addr_utxo_set(batch_addr_utxo_set: &mut Batch, txs: &[&bchrpc::Transaction]) -> Result<()> {
        for tx in txs {
            let tx_hash: [u8; 32] = tx.hash.as_slice().try_into()?;
            for (out_idx, output) in tx.outputs.iter().enumerate() {
                if let Destination::Address(address) = destination_from_script("abc", &output.pubkey_script) {
                    let key = AddrUtxoKey {
                        addr: AddrKeyPrefix {
                            addr_type: address.addr_type() as u8,
                            addr_hash: address.hash().as_slice().try_into()?,
                        },
                        utxo_key: UtxoKey {
                            tx_hash,
                            out_idx: U32::new(out_idx as u32),
                        },
                    };
                    batch_addr_utxo_set.insert(key.as_bytes(), b"");
                }
            }
        }
        for tx in txs {
            for input in &tx.inputs {
                if let Destination::Address(address) = destination_from_script("abc", &input.previous_script) {
                    if let Some(outpoint) = &input.outpoint {
                        let key = AddrUtxoKey {
                            addr: AddrKeyPrefix {
                                addr_type: address.addr_type() as u8,
                                addr_hash: address.hash().as_slice().try_into()?,
                            },
                            utxo_key: UtxoKey {
                                tx_hash: outpoint.hash.as_slice().try_into()?,
                                out_idx: U32::new(outpoint.index),
                            },
                        };
                        batch_addr_utxo_set.remove(key.as_bytes());
                    }
                }
            }
        }
        Ok(())
    }

    fn add_tx_out_spend(db_tx_out_spend: &mut Batch, tx: &bchrpc::Transaction) -> Result<()> {
        let by_tx_hash: [u8; 32] = tx.hash.as_slice().try_into()?;
        for (input_idx, input) in tx.inputs.iter().enumerate() {
            if let Some(outpoint) = &input.outpoint {
                let utxo_key = UtxoKey {
                    tx_hash: outpoint.hash.as_slice().try_into()?,
                    out_idx: U32::new(outpoint.index),
                };
                let spend = TxOutSpend {
                    by_tx_hash,
                    by_tx_input_idx: U32::new(input_idx as u32),
                };
                db_tx_out_spend.insert(utxo_key.as_bytes(), spend.as_bytes());
            }
        }
        Ok(())
    }

    fn add_token_meta(db_token_meta: &mut Batch, tx: &bchrpc::Transaction) -> Result<()> {
        use bchrpc::{SlpAction, slp_transaction_info::TxMetadata};
        let slp = match &tx.slp_transaction_info {
            Some(slp) if !slp.token_id.is_empty() => slp,
            _ => return Ok(()),
        };
        let token_meta = match (slp.slp_action(), &slp.tx_metadata) {
            (SlpAction::SlpV1Genesis, Some(TxMetadata::V1Genesis(genesis))) => {
                TokenMeta {
                    token_type: 0x01,
                    token_ticker: genesis.ticker.clone(),
                    token_name: genesis.name.clone(),
                    token_document_url: genesis.document_url.clone(),
                    token_document_hash: genesis.document_hash.clone(),
                    decimals: genesis.decimals,
                    group_id: None,
                }
            },
            (SlpAction::SlpNft1GroupGenesis, Some(TxMetadata::V1Genesis(genesis))) => {
                TokenMeta {
                    token_type: 0x81,
                    token_ticker: genesis.ticker.clone(),
                    token_name: genesis.name.clone(),
                    token_document_url: genesis.document_url.clone(),
                    token_document_hash: genesis.document_hash.clone(),
                    decimals: genesis.decimals,
                    group_id: None,
                }
            },
            (SlpAction::SlpNft1UniqueChildGenesis, Some(TxMetadata::Nft1ChildGenesis(genesis))) => {
                TokenMeta {
                    token_type: 0x41,
                    token_ticker: genesis.ticker.clone(),
                    token_name: genesis.name.clone(),
                    token_document_url: genesis.document_url.clone(),
                    token_document_hash: genesis.document_hash.clone(),
                    decimals: genesis.decimals,
                    group_id: Some(genesis.group_token_id.as_slice().try_into()?),
                }
            },
            _ => return Ok(()),
        };
        db_token_meta.insert(slp.token_id.as_slice(), bincode::serialize(&token_meta)?);
        Ok(())
    }
}

fn db_get<T: DeserializeOwned>(db: &sled::Tree, key: &[u8]) -> Result<T> {
    let item = db
        .get(key)?
        .ok_or_else(|| anyhow!("No entry for {}", String::from_utf8_lossy(key)))?;
    Ok(bincode::deserialize(&item)?)
}

fn db_get_option<T: DeserializeOwned>(db: &sled::Tree, key: &[u8]) -> Result<Option<T>> {
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
