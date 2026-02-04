use std::collections::BTreeMap;
use std::fmt;
use std::time::Duration;

use serde::Serialize;
use serde::de::DeserializeOwned;
use ureq::SendBody;
use wasmtime::{Caller, Engine, Linker, Memory, Module, Store, TypedFunc};

use crate::abi::{
    ABI_VERSION, AbiRequest, AbiResponse, AbiResult, OpCode, pack_u32_pair, unpack_u32_pair,
};
use crate::model::{
    HttpRequest, HttpResponse, InputResponse, PluginDescriptor, PluginError, ScrapeContext,
    ScrapeOutcome,
};

mod whitelist;
use whitelist::UrlWhitelist;

pub trait HttpClient {
    fn send(&mut self, request: &HttpRequest) -> Result<HttpResponse, HostError>;
}

#[derive(Debug, Default)]
pub struct BlockingHttpClient;

impl BlockingHttpClient {
    pub fn new() -> Self {
        Self
    }
}

impl HttpClient for BlockingHttpClient {
    fn send(&mut self, request: &HttpRequest) -> Result<HttpResponse, HostError> {
        let method = request
            .method
            .parse::<ureq::http::Method>()
            .map_err(|err| {
                HostError::Http(format!("invalid http method {}: {err}", request.method))
            })?;
        let uri = request
            .url
            .parse::<ureq::http::Uri>()
            .map_err(|err| HostError::Http(format!("invalid http url {}: {err}", request.url)))?;

        let mut builder = ureq::http::Request::builder().method(method).uri(uri);
        for (key, value) in &request.headers {
            builder = builder.header(key, value);
        }

        let body = match request.body.as_ref() {
            Some(body) => SendBody::from_owned_reader(std::io::Cursor::new(body.clone())),
            None => SendBody::none(),
        };
        let http_request = builder
            .body(body)
            .map_err(|err| HostError::Http(format!("build http request failed: {err}")))?;

        let agent = ureq::Agent::new_with_defaults();
        let mut config = agent
            .configure_request(http_request)
            .http_status_as_error(false);
        if let Some(timeout_ms) = request.timeout_ms {
            config = config.timeout_per_call(Some(Duration::from_millis(timeout_ms)));
        }
        let http_request = config.build();
        let response = agent
            .run(http_request)
            .map_err(|err| HostError::Http(format!("http request failed: {err}")))?;

        let (parts, mut body) = response.into_parts();
        let status = parts.status.as_u16();
        let mut headers = BTreeMap::new();
        for (name, value) in parts.headers.iter() {
            let value = String::from_utf8_lossy(value.as_bytes()).to_string();
            headers.insert(name.as_str().to_string(), value);
        }
        let body = body
            .read_to_vec()
            .map_err(|err| HostError::Http(format!("read http body failed: {err}")))?;

        Ok(HttpResponse {
            status,
            headers,
            body,
        })
    }
}

struct HostState {
    http: Box<dyn HttpClient>,
    whitelist: UrlWhitelist,
}

#[derive(Debug)]
pub enum HostError {
    Wasm(String),
    Abi(String),
    Json(String),
    Plugin(PluginError),
    Memory(String),
    Http(String),
    Policy(String),
}

impl fmt::Display for HostError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HostError::Wasm(message) => write!(f, "wasm error: {message}"),
            HostError::Abi(message) => write!(f, "abi error: {message}"),
            HostError::Json(message) => write!(f, "json error: {message}"),
            HostError::Plugin(error) => write!(f, "plugin error: {error:?}"),
            HostError::Memory(message) => write!(f, "memory error: {message}"),
            HostError::Http(message) => write!(f, "http error: {message}"),
            HostError::Policy(message) => write!(f, "policy error: {message}"),
        }
    }
}

impl std::error::Error for HostError {}

pub struct WasmPlugin {
    store: Store<HostState>,
    memory: Memory,
    alloc: TypedFunc<u32, u32>,
    free: TypedFunc<(u32, u32), ()>,
    call: TypedFunc<(u32, u32, u32), u64>,
    descriptor: PluginDescriptor,
}

impl WasmPlugin {
    pub fn load(engine: &Engine, wasm: impl AsRef<[u8]>) -> Result<Self, HostError> {
        Self::load_with_http(engine, wasm, Box::new(BlockingHttpClient::new()))
    }

    pub fn load_with_http(
        engine: &Engine,
        wasm: impl AsRef<[u8]>,
        http: Box<dyn HttpClient>,
    ) -> Result<Self, HostError> {
        let module = Module::new(engine, wasm.as_ref())
            .map_err(|err| HostError::Wasm(format!("compile wasm failed: {err}")))?;
        let mut linker = Linker::new(engine);
        linker
            .func_wrap("env", "weevil_http", host_http)
            .map_err(|err| HostError::Abi(format!("link weevil_http failed: {err}")))?;

        let whitelist = UrlWhitelist::new(Vec::new())?;
        let mut store = Store::new(engine, HostState { http, whitelist });
        let instance = linker
            .instantiate(&mut store, &module)
            .map_err(|err| HostError::Wasm(format!("instantiate wasm failed: {err}")))?;
        let memory = instance
            .get_memory(&mut store, "memory")
            .ok_or_else(|| HostError::Abi("missing exported memory".to_string()))?;
        let alloc = instance
            .get_typed_func(&mut store, "weevil_alloc")
            .map_err(|err| HostError::Abi(format!("missing weevil_alloc: {err}")))?;
        let free = instance
            .get_typed_func(&mut store, "weevil_free")
            .map_err(|err| HostError::Abi(format!("missing weevil_free: {err}")))?;
        let call = instance
            .get_typed_func(&mut store, "weevil_call")
            .map_err(|err| HostError::Abi(format!("missing weevil_call: {err}")))?;

        let mut plugin = Self {
            store,
            memory,
            alloc,
            free,
            call,
            descriptor: PluginDescriptor::new("unknown", "0.0.0"),
        };
        let descriptor = plugin.describe()?;
        let whitelist = UrlWhitelist::new(descriptor.http_whitelist.clone())?;
        plugin.store.data_mut().whitelist = whitelist;
        plugin.descriptor = descriptor;
        Ok(plugin)
    }

    pub fn descriptor(&self) -> &PluginDescriptor {
        &self.descriptor
    }

    pub fn describe(&mut self) -> Result<PluginDescriptor, HostError> {
        self.call(OpCode::Describe, ())
    }

    pub async fn describe_async(&mut self) -> Result<PluginDescriptor, HostError> {
        self.describe()
    }

    pub fn run(&mut self, context: &ScrapeContext) -> Result<ScrapeOutcome, HostError> {
        self.call(OpCode::Scrape, context)
    }

    pub async fn run_async(&mut self, context: &ScrapeContext) -> Result<ScrapeOutcome, HostError> {
        self.run(context)
    }

    pub fn submit_input(&mut self, input: &InputResponse) -> Result<ScrapeOutcome, HostError> {
        self.call(OpCode::SubmitInput, input)
    }

    pub async fn submit_input_async(
        &mut self,
        input: &InputResponse,
    ) -> Result<ScrapeOutcome, HostError> {
        self.submit_input(input)
    }

    fn call<T: Serialize, R: DeserializeOwned>(
        &mut self,
        op: OpCode,
        payload: T,
    ) -> Result<R, HostError> {
        let request = AbiRequest::new(payload);
        let bytes = serde_json::to_vec(&request)
            .map_err(|err| HostError::Json(format!("serialize request failed: {err}")))?;
        let (ptr, len) = self.write_bytes(&bytes)?;

        let op_value = u32::from(op);
        let packed = self
            .call
            .call(&mut self.store, (op_value, ptr, len))
            .map_err(|err| HostError::Wasm(format!("call {op} failed: {err}")))?;
        self.free_if_needed(ptr, len)?;

        let (res_ptr, res_len) = unpack_u32_pair(packed);
        if res_len == 0 {
            return Err(HostError::Abi(format!(
                "empty response from plugin for {op}"
            )));
        }
        let response_bytes = self.read_bytes(res_ptr, res_len)?;
        self.free_if_needed(res_ptr, res_len)?;

        let response: AbiResponse<R> = serde_json::from_slice(&response_bytes).map_err(|err| {
            HostError::Json(format!("deserialize response for {op} failed: {err}"))
        })?;
        if response.version != ABI_VERSION {
            return Err(HostError::Abi(format!(
                "abi version mismatch: {} != {}",
                response.version, ABI_VERSION
            )));
        }
        match response.result {
            AbiResult::Ok(payload) => Ok(payload),
            AbiResult::Err(error) => Err(HostError::Plugin(error)),
        }
    }

    fn write_bytes(&mut self, bytes: &[u8]) -> Result<(u32, u32), HostError> {
        let len = u32::try_from(bytes.len()).map_err(|_| {
            HostError::Memory(format!(
                "request too large: {} bytes exceeds u32::MAX",
                bytes.len()
            ))
        })?;
        if len == 0 {
            return Ok((0, 0));
        }
        let ptr = self
            .alloc
            .call(&mut self.store, len)
            .map_err(|err| HostError::Wasm(format!("alloc failed: {err}")))?;
        if ptr == 0 {
            return Err(HostError::Memory("alloc returned null pointer".to_string()));
        }
        let start = ptr as usize;
        let end = start + bytes.len();
        let memory = self.memory.data_mut(&mut self.store);
        if end > memory.len() {
            return Err(HostError::Memory(format!(
                "write out of bounds: end {end} > memory {}",
                memory.len()
            )));
        }
        memory[start..end].copy_from_slice(bytes);
        Ok((ptr, len))
    }

    fn read_bytes(&mut self, ptr: u32, len: u32) -> Result<Vec<u8>, HostError> {
        if ptr == 0 {
            return Err(HostError::Memory("response pointer is null".to_string()));
        }
        let len = len as usize;
        let start = ptr as usize;
        let end = start + len;
        let memory = self.memory.data(&self.store);
        if end > memory.len() {
            return Err(HostError::Memory(format!(
                "read out of bounds: end {end} > memory {}",
                memory.len()
            )));
        }
        Ok(memory[start..end].to_vec())
    }

    fn free_if_needed(&mut self, ptr: u32, len: u32) -> Result<(), HostError> {
        if ptr == 0 || len == 0 {
            return Ok(());
        }
        self.free
            .call(&mut self.store, (ptr, len))
            .map_err(|err| HostError::Wasm(format!("free failed: {err}")))
    }
}

fn host_http(mut caller: Caller<'_, HostState>, ptr: u32, len: u32) -> u64 {
    let result = handle_http(&mut caller, ptr, len);
    match result {
        Ok(response) => encode_response(&mut caller, &AbiResponse::new_ok(response)),
        Err(error) => encode_response(&mut caller, &AbiResponse::<()>::new_err(error)),
    }
}

fn handle_http(
    caller: &mut Caller<'_, HostState>,
    ptr: u32,
    len: u32,
) -> Result<HttpResponse, PluginError> {
    let bytes = read_guest_bytes(caller, ptr, len)?;
    let request: AbiRequest<HttpRequest> = serde_json::from_slice(&bytes).map_err(|err| {
        PluginError::with_code(format!("invalid http request json: {err}"), "invalid_json")
    })?;
    if request.version != ABI_VERSION {
        return Err(PluginError::with_code(
            format!(
                "abi version mismatch: {} != {}",
                request.version, ABI_VERSION
            ),
            "abi_version_mismatch",
        ));
    }
    let url = request.payload.url.as_str();
    if !caller.data().whitelist.allows(url) {
        return Err(PluginError::with_code(
            format!("http url not allowed: {url}"),
            "http_not_allowed",
        ));
    }
    caller
        .data_mut()
        .http
        .send(&request.payload)
        .map_err(|err| PluginError::with_code(format!("http request failed: {err}"), "http_failed"))
}

fn read_guest_bytes(
    caller: &mut Caller<'_, HostState>,
    ptr: u32,
    len: u32,
) -> Result<Vec<u8>, PluginError> {
    if len == 0 {
        return Err(PluginError::with_code(
            "empty request payload",
            "empty_payload",
        ));
    }
    if ptr == 0 {
        return Err(PluginError::with_code(
            "null request pointer",
            "null_pointer",
        ));
    }
    let len = usize::try_from(len).map_err(|_| {
        PluginError::with_code(format!("invalid request length {len}"), "invalid_length")
    })?;
    let memory = get_memory(caller)?;
    let data = memory.data(&*caller);
    let start = ptr as usize;
    let end = start + len;
    if end > data.len() {
        return Err(PluginError::with_code(
            format!(
                "request read out of bounds: end {end} > memory {}",
                data.len()
            ),
            "invalid_memory",
        ));
    }
    Ok(data[start..end].to_vec())
}

fn encode_response<T: Serialize>(
    caller: &mut Caller<'_, HostState>,
    response: &AbiResponse<T>,
) -> u64 {
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
    match write_guest_bytes(caller, &bytes) {
        Ok((ptr, len)) => pack_u32_pair(ptr, len),
        Err(_) => pack_u32_pair(0, 0),
    }
}

fn write_guest_bytes(
    caller: &mut Caller<'_, HostState>,
    bytes: &[u8],
) -> Result<(u32, u32), PluginError> {
    let len = u32::try_from(bytes.len()).map_err(|_| {
        PluginError::with_code(
            format!("response too large: {} bytes exceeds u32::MAX", bytes.len()),
            "response_too_large",
        )
    })?;
    if len == 0 {
        return Ok((0, 0));
    }
    let ptr = alloc_in_guest(caller, len)?;
    let start = ptr as usize;
    let end = start + bytes.len();
    let memory = get_memory(caller)?;
    let data_len = memory.data(&*caller).len();
    if end > data_len {
        let _ = free_in_guest(caller, ptr, len);
        return Err(PluginError::with_code(
            format!("response write out of bounds: end {end} > memory {data_len}"),
            "invalid_memory",
        ));
    }
    let data = memory.data_mut(&mut *caller);
    data[start..end].copy_from_slice(bytes);
    Ok((ptr, len))
}

fn alloc_in_guest(caller: &mut Caller<'_, HostState>, len: u32) -> Result<u32, PluginError> {
    let func = caller
        .get_export("weevil_alloc")
        .and_then(|export| export.into_func())
        .ok_or_else(|| PluginError::with_code("missing weevil_alloc export", "missing_export"))?;
    let alloc = func.typed::<u32, u32>(&*caller).map_err(|err| {
        PluginError::with_code(
            format!("invalid weevil_alloc signature: {err}"),
            "abi_error",
        )
    })?;
    let ptr = alloc.call(caller, len).map_err(|err| {
        PluginError::with_code(format!("weevil_alloc failed: {err}"), "alloc_failed")
    })?;
    if ptr == 0 {
        return Err(PluginError::with_code(
            "weevil_alloc returned null pointer",
            "alloc_failed",
        ));
    }
    Ok(ptr)
}

fn free_in_guest(
    caller: &mut Caller<'_, HostState>,
    ptr: u32,
    len: u32,
) -> Result<(), PluginError> {
    let func = caller
        .get_export("weevil_free")
        .and_then(|export| export.into_func())
        .ok_or_else(|| PluginError::with_code("missing weevil_free export", "missing_export"))?;
    let free = func.typed::<(u32, u32), ()>(&*caller).map_err(|err| {
        PluginError::with_code(format!("invalid weevil_free signature: {err}"), "abi_error")
    })?;
    free.call(caller, (ptr, len))
        .map_err(|err| PluginError::with_code(format!("weevil_free failed: {err}"), "free_failed"))
}

fn get_memory(caller: &mut Caller<'_, HostState>) -> Result<Memory, PluginError> {
    caller
        .get_export("memory")
        .and_then(|export| export.into_memory())
        .ok_or_else(|| PluginError::with_code("missing exported memory", "missing_memory"))
}
