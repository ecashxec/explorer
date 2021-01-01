use anyhow::Result;

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
