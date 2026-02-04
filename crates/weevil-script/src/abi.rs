use std::fmt;

use serde::{Deserialize, Serialize};

use crate::model::PluginError;

pub const ABI_VERSION: u32 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum OpCode {
    Describe = 1,
    Scrape = 2,
    SubmitInput = 3,
}

impl TryFrom<u32> for OpCode {
    type Error = PluginError;

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(OpCode::Describe),
            2 => Ok(OpCode::Scrape),
            3 => Ok(OpCode::SubmitInput),
            _ => Err(PluginError::with_code(
                format!("unsupported opcode {value}"),
                "unsupported_opcode",
            )),
        }
    }
}

impl From<OpCode> for u32 {
    fn from(op: OpCode) -> Self {
        op as u32
    }
}

impl fmt::Display for OpCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let label = match self {
            OpCode::Describe => "describe",
            OpCode::Scrape => "scrape",
            OpCode::SubmitInput => "submit_input",
        };
        write!(f, "{label}")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AbiRequest<T> {
    pub version: u32,
    pub payload: T,
}

impl<T> AbiRequest<T> {
    pub fn new(payload: T) -> Self {
        Self {
            version: ABI_VERSION,
            payload,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "status", content = "payload", rename_all = "snake_case")]
pub enum AbiResult<T> {
    Ok(T),
    Err(PluginError),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AbiResponse<T> {
    pub version: u32,
    pub result: AbiResult<T>,
}

impl<T> AbiResponse<T> {
    pub fn new_ok(payload: T) -> Self {
        Self {
            version: ABI_VERSION,
            result: AbiResult::Ok(payload),
        }
    }

    pub fn new_err(error: PluginError) -> Self {
        Self {
            version: ABI_VERSION,
            result: AbiResult::Err(error),
        }
    }
}

pub fn pack_u32_pair(ptr: u32, len: u32) -> u64 {
    u64::from(ptr) | (u64::from(len) << 32)
}

pub fn unpack_u32_pair(value: u64) -> (u32, u32) {
    let ptr = value as u32;
    let len = (value >> 32) as u32;
    (ptr, len)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pack_unpack_roundtrip() {
        let ptr = 42_u32;
        let len = 9001_u32;
        let packed = pack_u32_pair(ptr, len);
        let (decoded_ptr, decoded_len) = unpack_u32_pair(packed);
        assert_eq!(decoded_ptr, ptr);
        assert_eq!(decoded_len, len);
    }

    #[test]
    fn response_roundtrip() {
        let response = AbiResponse::new_ok("ready".to_string());
        let json = serde_json::to_string(&response).expect("serialize response");
        let parsed: AbiResponse<String> =
            serde_json::from_str(&json).expect("deserialize response");
        assert_eq!(parsed, response);
    }
}
