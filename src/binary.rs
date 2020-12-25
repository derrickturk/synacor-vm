use std::convert::TryInto;

use super::vm::{Error, Result};

pub fn read_binary(binary: &[u8]) -> Result<Vec<u16>> {
    if binary.len() % 2 != 0 {
        Err(Error::BadBinary)
    } else {
        Ok(binary.chunks(2)
          .map(|c| u16::from_le_bytes(c.try_into().unwrap()))
          .collect())
    }
}
