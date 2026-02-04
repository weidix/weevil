use crate::abi::{ABI_VERSION, AbiRequest, AbiResponse, AbiResult, unpack_u32_pair};
use crate::model::{HttpRequest, HttpResponse, PluginError};

use super::{free, read_bytes, write_bytes};

#[cfg(target_arch = "wasm32")]
#[link(wasm_import_module = "env")]
extern "C" {
    fn weevil_http(ptr: u32, len: u32) -> u64;
}

#[cfg(not(target_arch = "wasm32"))]
fn weevil_http(_ptr: u32, _len: u32) -> u64 {
    0
}

pub fn request(request: HttpRequest) -> Result<HttpResponse, PluginError> {
    let payload = AbiRequest::new(request);
    let bytes = serde_json::to_vec(&payload).map_err(|err| {
        PluginError::with_code(
            format!("serialize http request failed: {err}"),
            "serialize_failed",
        )
    })?;
    let packed = write_bytes(&bytes);
    let (ptr, len) = unpack_u32_pair(packed);
    if len == 0 {
        return Err(PluginError::with_code(
            "http request buffer is empty",
            "empty_payload",
        ));
    }
    let response_packed = weevil_http(ptr, len);
    free(ptr, len);

    let (res_ptr, res_len) = unpack_u32_pair(response_packed);
    if res_len == 0 {
        return Err(PluginError::with_code(
            "http response buffer is empty",
            "empty_payload",
        ));
    }
    let response_bytes = read_bytes(res_ptr, res_len)?;
    free(res_ptr, res_len);

    let response: AbiResponse<HttpResponse> =
        serde_json::from_slice(&response_bytes).map_err(|err| {
            PluginError::with_code(
                format!("deserialize http response failed: {err}"),
                "invalid_json",
            )
        })?;
    if response.version != ABI_VERSION {
        return Err(PluginError::with_code(
            format!(
                "abi version mismatch: {} != {}",
                response.version, ABI_VERSION
            ),
            "abi_version_mismatch",
        ));
    }
    match response.result {
        AbiResult::Ok(payload) => Ok(payload),
        AbiResult::Err(error) => Err(error),
    }
}
