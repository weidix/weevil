use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::abi::{ABI_VERSION, AbiRequest, AbiResponse, OpCode, pack_u32_pair};
use crate::model::{InputResponse, PluginDescriptor, PluginError, ScrapeContext, ScrapeOutcome};

pub mod http;

#[cfg(all(test, not(target_arch = "wasm32")))]
mod test_memory {
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::{Mutex, OnceLock};

    static NEXT_PTR: AtomicU32 = AtomicU32::new(1);
    static HEAP: OnceLock<Mutex<HashMap<u32, Vec<u8>>>> = OnceLock::new();

    fn heap() -> &'static Mutex<HashMap<u32, Vec<u8>>> {
        HEAP.get_or_init(|| Mutex::new(HashMap::new()))
    }

    pub fn alloc(len: u32) -> u32 {
        if len == 0 {
            return 0;
        }
        let len = match usize::try_from(len) {
            Ok(len) => len,
            Err(_) => return 0,
        };
        let ptr = NEXT_PTR.fetch_add(1, Ordering::Relaxed);
        let mut heap = heap().lock().expect("test heap poisoned");
        heap.insert(ptr, vec![0_u8; len]);
        ptr
    }

    pub fn free(ptr: u32) {
        if ptr == 0 {
            return;
        }
        let mut heap = heap().lock().expect("test heap poisoned");
        heap.remove(&ptr);
    }

    pub fn write(ptr: u32, bytes: &[u8]) -> bool {
        if ptr == 0 {
            return false;
        }
        let mut heap = heap().lock().expect("test heap poisoned");
        let buffer = match heap.get_mut(&ptr) {
            Some(buffer) => buffer,
            None => return false,
        };
        if bytes.len() > buffer.len() {
            return false;
        }
        buffer[..bytes.len()].copy_from_slice(bytes);
        true
    }

    pub fn read(ptr: u32, len: u32) -> Option<Vec<u8>> {
        if ptr == 0 {
            return None;
        }
        let len = usize::try_from(len).ok()?;
        let heap = heap().lock().expect("test heap poisoned");
        let buffer = heap.get(&ptr)?;
        if len > buffer.len() {
            return None;
        }
        Some(buffer[..len].to_vec())
    }
}

pub trait Plugin {
    fn new() -> Self
    where
        Self: Sized;

    fn describe(&self) -> PluginDescriptor;

    fn scrape(&mut self, context: ScrapeContext) -> Result<ScrapeOutcome, PluginError>;

    fn submit_input(&mut self, _input: InputResponse) -> Result<ScrapeOutcome, PluginError> {
        Err(PluginError::with_code(
            "input is not supported",
            "input_not_supported",
        ))
    }
}

pub fn alloc(len: u32) -> u32 {
    #[cfg(all(test, not(target_arch = "wasm32")))]
    {
        return test_memory::alloc(len);
    }
    #[cfg(not(all(test, not(target_arch = "wasm32"))))]
    {
        if len == 0 {
            return 0;
        }
        let len = match usize::try_from(len) {
            Ok(len) => len,
            Err(_) => return 0,
        };
        let mut buf = vec![0_u8; len];
        let ptr = buf.as_mut_ptr();
        std::mem::forget(buf);
        ptr as u32
    }
}

pub fn free(ptr: u32, len: u32) {
    #[cfg(all(test, not(target_arch = "wasm32")))]
    {
        let _ = len;
        test_memory::free(ptr);
        return;
    }
    #[cfg(not(all(test, not(target_arch = "wasm32"))))]
    {
        if ptr == 0 || len == 0 {
            return;
        }
        let len = match usize::try_from(len) {
            Ok(len) => len,
            Err(_) => return,
        };
        unsafe {
            let _ = Vec::from_raw_parts(ptr as *mut u8, len, len);
        }
    }
}

pub fn dispatch<P: Plugin>(plugin: &mut P, op: u32, ptr: u32, len: u32) -> u64 {
    let op = match OpCode::try_from(op) {
        Ok(op) => op,
        Err(err) => return encode_error(err),
    };

    match op {
        OpCode::Describe => {
            let request: AbiRequest<()> = match read_request(ptr, len) {
                Ok(request) => request,
                Err(err) => return encode_error(err),
            };
            if let Err(err) = ensure_version(request.version) {
                return encode_error(err);
            }
            let payload = plugin.describe();
            encode_ok(payload)
        }
        OpCode::Scrape => {
            let request: AbiRequest<ScrapeContext> = match read_request(ptr, len) {
                Ok(request) => request,
                Err(err) => return encode_error(err),
            };
            if let Err(err) = ensure_version(request.version) {
                return encode_error(err);
            }
            let result = plugin.scrape(request.payload);
            encode_result(result)
        }
        OpCode::SubmitInput => {
            let request: AbiRequest<InputResponse> = match read_request(ptr, len) {
                Ok(request) => request,
                Err(err) => return encode_error(err),
            };
            if let Err(err) = ensure_version(request.version) {
                return encode_error(err);
            }
            let result = plugin.submit_input(request.payload);
            encode_result(result)
        }
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
pub(crate) fn read_bytes(ptr: u32, len: u32) -> Result<Vec<u8>, PluginError> {
    if len == 0 {
        return Err(PluginError::with_code("empty payload", "empty_payload"));
    }
    if ptr == 0 {
        return Err(PluginError::with_code("null pointer", "null_pointer"));
    }
    test_memory::read(ptr, len).ok_or_else(|| {
        PluginError::with_code(
            format!("invalid memory: ptr {ptr} len {len}"),
            "invalid_memory",
        )
    })
}

#[cfg(all(test, not(target_arch = "wasm32")))]
fn read_request<T: DeserializeOwned>(ptr: u32, len: u32) -> Result<AbiRequest<T>, PluginError> {
    let bytes = read_bytes(ptr, len)?;
    serde_json::from_slice(&bytes).map_err(|err| {
        PluginError::with_code(format!("invalid request json: {err}"), "invalid_json")
    })
}

#[cfg(not(all(test, not(target_arch = "wasm32"))))]
pub(crate) fn read_bytes(ptr: u32, len: u32) -> Result<Vec<u8>, PluginError> {
    if len == 0 {
        return Err(PluginError::with_code("empty payload", "empty_payload"));
    }
    if ptr == 0 {
        return Err(PluginError::with_code("null pointer", "null_pointer"));
    }
    let len = usize::try_from(len)
        .map_err(|_| PluginError::with_code(format!("invalid length {len}"), "invalid_length"))?;
    let bytes = unsafe { std::slice::from_raw_parts(ptr as *const u8, len) };
    Ok(bytes.to_vec())
}

#[cfg(not(all(test, not(target_arch = "wasm32"))))]
fn read_request<T: DeserializeOwned>(ptr: u32, len: u32) -> Result<AbiRequest<T>, PluginError> {
    let bytes = read_bytes(ptr, len)?;
    serde_json::from_slice(&bytes).map_err(|err| {
        PluginError::with_code(format!("invalid request json: {err}"), "invalid_json")
    })
}

fn ensure_version(version: u32) -> Result<(), PluginError> {
    if version != ABI_VERSION {
        return Err(PluginError::with_code(
            format!("abi version mismatch: {version} != {ABI_VERSION}"),
            "abi_version_mismatch",
        ));
    }
    Ok(())
}

fn encode_ok<T: Serialize>(payload: T) -> u64 {
    encode_response(&AbiResponse::new_ok(payload))
}

fn encode_result(result: Result<ScrapeOutcome, PluginError>) -> u64 {
    match result {
        Ok(outcome) => encode_ok(outcome),
        Err(err) => encode_error(err),
    }
}

fn encode_error(error: PluginError) -> u64 {
    encode_response(&AbiResponse::<()>::new_err(error))
}

fn encode_response<T: Serialize>(response: &AbiResponse<T>) -> u64 {
    let bytes = match serde_json::to_vec(response) {
        Ok(bytes) => bytes,
        Err(err) => {
            let fallback = AbiResponse::<()>::new_err(PluginError::with_code(
                format!("serialize response failed: {err}"),
                "serialize_failed",
            ));
            serde_json::to_vec(&fallback).unwrap_or_default()
        }
    };
    write_bytes(&bytes)
}

#[cfg(all(test, not(target_arch = "wasm32")))]
pub(crate) fn write_bytes(bytes: &[u8]) -> u64 {
    if bytes.is_empty() {
        return pack_u32_pair(0, 0);
    }
    let len = match u32::try_from(bytes.len()) {
        Ok(len) => len,
        Err(_) => return pack_u32_pair(0, 0),
    };
    if len == 0 {
        return pack_u32_pair(0, 0);
    }
    let ptr = alloc(len);
    if ptr == 0 {
        return pack_u32_pair(0, 0);
    }
    if !test_memory::write(ptr, bytes) {
        free(ptr, len);
        return pack_u32_pair(0, 0);
    }
    pack_u32_pair(ptr, len)
}

#[cfg(not(all(test, not(target_arch = "wasm32"))))]
pub(crate) fn write_bytes(bytes: &[u8]) -> u64 {
    if bytes.is_empty() {
        return pack_u32_pair(0, 0);
    }
    let len_u32 = match u32::try_from(bytes.len()) {
        Ok(len) => len,
        Err(_) => return pack_u32_pair(0, 0),
    };
    if len_u32 == 0 {
        return pack_u32_pair(0, 0);
    }
    let ptr = alloc(len_u32);
    if ptr == 0 {
        return pack_u32_pair(0, 0);
    }
    let len = match usize::try_from(len_u32) {
        Ok(len) => len,
        Err(_) => return pack_u32_pair(0, 0),
    };
    unsafe {
        let out = std::slice::from_raw_parts_mut(ptr as *mut u8, len);
        out.copy_from_slice(bytes);
    }
    pack_u32_pair(ptr, len_u32)
}

#[macro_export]
macro_rules! export_plugin {
    ($plugin:ty) => {
        #[unsafe(no_mangle)]
        pub extern "C" fn weevil_alloc(len: u32) -> u32 {
            $crate::guest::alloc(len)
        }

        #[unsafe(no_mangle)]
        pub extern "C" fn weevil_free(ptr: u32, len: u32) {
            $crate::guest::free(ptr, len)
        }

        #[unsafe(no_mangle)]
        pub extern "C" fn weevil_call(op: u32, ptr: u32, len: u32) -> u64 {
            static mut PLUGIN: Option<$plugin> = None;
            let plugin = unsafe {
                if PLUGIN.is_none() {
                    PLUGIN = Some(<$plugin as $crate::guest::Plugin>::new());
                }
                PLUGIN.as_mut().expect("plugin initialized")
            };
            $crate::guest::dispatch(plugin, op, ptr, len)
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::abi::{AbiRequest, AbiResult, unpack_u32_pair};
    use crate::model::{Record, ScrapeResponse};
    use serde_json::Value;

    struct Demo;

    impl Plugin for Demo {
        fn new() -> Self {
            Self
        }

        fn describe(&self) -> PluginDescriptor {
            PluginDescriptor::new("demo", "0.1.0")
        }

        fn scrape(&mut self, context: ScrapeContext) -> Result<ScrapeOutcome, PluginError> {
            let mut response = ScrapeResponse::new();
            let mut record = Record::new("demo");
            record.fields.insert(
                "context_len".to_string(),
                Value::Number(context.context.len().into()),
            );
            response.records.push(record);
            Ok(ScrapeOutcome::Completed { response })
        }
    }

    fn write_request<T: Serialize>(payload: T) -> (u32, u32) {
        let request = AbiRequest::new(payload);
        let bytes = serde_json::to_vec(&request).expect("serialize request");
        let len = u32::try_from(bytes.len()).expect("len fits in u32");
        let ptr = alloc(len);
        #[cfg(all(test, not(target_arch = "wasm32")))]
        {
            let wrote = super::test_memory::write(ptr, &bytes);
            assert!(wrote, "write request failed for ptr {ptr} len {len}");
        }
        #[cfg(not(all(test, not(target_arch = "wasm32"))))]
        {
            let len = usize::try_from(len).expect("len fits in usize");
            unsafe {
                let out = std::slice::from_raw_parts_mut(ptr as *mut u8, len);
                out.copy_from_slice(&bytes);
            }
        }
        (ptr, len)
    }

    fn read_response<T: DeserializeOwned>(packed: u64) -> AbiResponse<T> {
        let (ptr, len) = unpack_u32_pair(packed);
        assert!(len > 0, "response length is non-zero");
        #[cfg(all(test, not(target_arch = "wasm32")))]
        let bytes = super::test_memory::read(ptr, len).expect("read response from test memory");
        #[cfg(not(all(test, not(target_arch = "wasm32"))))]
        let bytes = {
            let len = usize::try_from(len).expect("len fits in usize");
            unsafe { std::slice::from_raw_parts(ptr as *const u8, len) }.to_vec()
        };
        let response = serde_json::from_slice(&bytes).expect("deserialize response");
        free(ptr, len);
        response
    }

    #[test]
    fn dispatch_describe() {
        let mut plugin = Demo::new();
        let (ptr, len) = write_request(());
        let packed = dispatch(&mut plugin, OpCode::Describe as u32, ptr, len);
        free(ptr, len);
        let response: AbiResponse<PluginDescriptor> = read_response(packed);
        match response.result {
            AbiResult::Ok(payload) => {
                assert_eq!(payload.name, "demo");
            }
            AbiResult::Err(err) => panic!("unexpected error: {err:?}"),
        }
    }

    #[test]
    fn dispatch_scrape() {
        let mut plugin = Demo::new();
        let (ptr, len) = write_request(ScrapeContext::new());
        let packed = dispatch(&mut plugin, OpCode::Scrape as u32, ptr, len);
        free(ptr, len);
        let response: AbiResponse<ScrapeOutcome> = read_response(packed);
        match response.result {
            AbiResult::Ok(ScrapeOutcome::Completed { response }) => {
                assert_eq!(response.records.len(), 1);
            }
            _ => panic!("unexpected response"),
        }
    }
}
