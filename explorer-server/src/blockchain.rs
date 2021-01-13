use std::str::FromStr;

use anyhow::Result;
use bitcoin_cash::{Address, AddressType, Hash160, Hashed, Op, Opcode, Ops, Script};

use crate::grpc::bchrpc;

#[derive(Default)]
#[repr(C, align(1))]
pub struct BlockHeader {
    pub version: i32,
    pub previous_block: [u8; 32],
    pub merkle_root: [u8; 32],
    pub timestamp: u32,
    pub bits: u32,
    pub nonce: u32,
}

unsafe impl plain::Plain for BlockHeader {}

impl BlockHeader {
    pub fn as_slice(&self) -> &[u8] {
        unsafe { plain::as_bytes(self) }
    }

    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        unsafe { plain::as_mut_bytes(self) }
    }
}

pub fn to_le_hex(slice: &[u8]) -> String {
    let mut vec = slice.to_vec();
    vec.reverse();
    hex::encode(&vec)
}

pub fn from_le_hex(string: &str) -> Result<Vec<u8>> {
    let mut decoded = hex::decode(string)?;
    decoded.reverse();
    Ok(decoded)
}

#[derive(Clone, Debug)]
pub enum Destination<'a> {
    Nulldata(Vec<Op>),
    Address(Address<'a>),
    P2PK(Vec<u8>),
    Unknown(Vec<u8>),
}

pub fn destination_from_script<'a>(prefix: &'a str, script: &[u8]) -> Destination<'a> {
    const OP_DUP: u8 = Opcode::OP_DUP as u8;
    const OP_HASH160: u8 = Opcode::OP_HASH160 as u8;
    const OP_EQUALVERIFY: u8 = Opcode::OP_EQUALVERIFY as u8;
    const OP_CHECKSIG: u8 = Opcode::OP_CHECKSIG as u8;
    const OP_EQUAL: u8 = Opcode::OP_EQUAL as u8;
    const OP_RETURN: u8 = Opcode::OP_RETURN as u8;
    match script {
        [OP_DUP, OP_HASH160, 20, hash @ .., OP_EQUALVERIFY, OP_CHECKSIG] => {
            Destination::Address(
                Address::from_hash(
                    prefix,
                    AddressType::P2PKH,
                    Hash160::from_slice(hash).expect("Invalid hash"),
                ),
            )
        }
        [OP_HASH160, 20, hash @ .., OP_EQUAL] => {
            Destination::Address(
                Address::from_hash(
                    prefix,
                    AddressType::P2SH,
                    Hash160::from_slice(hash).expect("Invalid hash"),
                )
            )
        }
        [33, pk @ .., OP_CHECKSIG] => Destination::P2PK(pk.to_vec()),
        [65, pk @ .., OP_CHECKSIG] => Destination::P2PK(pk.to_vec()),
        [OP_RETURN, data @ ..] => {
            let ops = Script::deser_ops(data.into()).unwrap_or(Script::new(vec![]));
            Destination::Nulldata(ops.ops().into_iter().map(|op| op.op.clone()).collect())
        }
        _ => Destination::Unknown(script.to_vec()),
    }
}

pub fn is_coinbase(outpoint: &bchrpc::transaction::input::Outpoint) -> bool {
    &outpoint.hash == &[0; 32] && outpoint.index == 0xffff_ffff
}

pub fn to_legacy_address(address: &Address<'_>) -> String {
    let hash_hex = address.hash().to_hex_be();
    let script = bitcoin::Script::new_p2pkh(
        &FromStr::from_str(&hash_hex).expect("Invalid pkh")
    );
    let address = bitcoin::Address::from_script(&script, bitcoin::Network::Bitcoin);
    let address = address.expect("Invalid address");
    address.to_string()
}
