use std::path::PathBuf;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum LuaPluginError {
    #[error("Lua error: {0}")]
    Lua(#[from] mlua::Error),
    #[error("script did not return a value")]
    ScriptReturnMissing,
    #[error("script returned {kind}, expected a table")]
    ScriptReturnNotTable { kind: String },
    #[error("script table is missing alias")]
    MissingAlias,
    #[error("alias must be a string, got {kind}")]
    InvalidAliasType { kind: String },
    #[error("alias is not valid UTF-8")]
    AliasNotUtf8,
    #[error("alias cannot be empty")]
    EmptyAlias,
    #[error("script table is missing trusted_urls")]
    MissingTrustedUrls,
    #[error("trusted_urls must be an array of strings, got {kind}")]
    InvalidTrustedUrlsType { kind: String },
    #[error("trusted_urls entry {index} must be a string, got {kind}")]
    InvalidTrustedUrlEntry { index: usize, kind: String },
    #[error("trusted_urls entry {index} is not valid UTF-8")]
    TrustedUrlEntryNotUtf8 { index: usize },
    #[error("trusted url must be absolute http/https, got {value}")]
    InvalidTrustedUrl { value: String },
    #[error("trusted url must include a host, got {value}")]
    TrustedUrlMissingHost { value: String },
    #[error("trusted url has unsupported scheme {scheme} for {value}")]
    TrustedUrlUnsupportedScheme { scheme: String, value: String },
    #[error("HTTP url must be absolute http/https, got {value}")]
    InvalidHttpUrl { value: String },
    #[error("HTTP url must include a host, got {value}")]
    HttpUrlMissingHost { value: String },
    #[error("HTTP url has unsupported scheme {scheme} for {value}")]
    HttpUrlUnsupportedScheme { scheme: String, value: String },
    #[error("URL {url} is not in the trusted list")]
    UntrustedUrl { url: String },
    #[error("HTTP header name must be a string, got {kind}")]
    HttpHeaderNameNotString { kind: String },
    #[error("HTTP header name is not valid UTF-8")]
    HttpHeaderNameNotUtf8,
    #[error("HTTP header {name} must be a string value, got {kind}")]
    HttpHeaderValueNotString { name: String, kind: String },
    #[error("HTTP header {name} value is not valid UTF-8")]
    HttpHeaderValueNotUtf8 { name: String },
    #[error("HTTP header name is invalid: {name}")]
    HttpHeaderInvalidName { name: String },
    #[error("HTTP header {name} has invalid value: {value}")]
    HttpHeaderInvalidValue { name: String, value: String },
    #[error("HTTP options must be a table, got {kind}")]
    HttpOptionsNotTable { kind: String },
    #[error("HTTP options headers must be a table, got {kind}")]
    HttpOptionsHeadersNotTable { kind: String },
    #[error("HTTP version must be a string, got {kind}")]
    HttpVersionNotString { kind: String },
    #[error("HTTP version is not valid UTF-8")]
    HttpVersionNotUtf8,
    #[error("HTTP version {value} is not supported (use \"1.1\" or \"2\")")]
    HttpVersionUnsupported { value: String },
    #[error("HTTP request failed for {url}: {source}")]
    HttpRequest {
        url: String,
        #[source]
        source: reqwest::Error,
    },
    #[error("HTTP request returned status {status} for {url}")]
    HttpStatus { url: String, status: u16 },
    #[error("HTTP is disabled in script check mode")]
    HttpDisabled,
    #[error("browser automation is disabled in script check mode")]
    BrowserDisabled,
    #[error("browser automation feature is not enabled")]
    BrowserFeatureDisabled,
    #[error("browser options must be a table, got {kind}")]
    BrowserOptionsNotTable { kind: String },
    #[error("browser option {name} must be a boolean, got {kind}")]
    BrowserOptionNotBoolean { name: String, kind: String },
    #[error("browser option {name} must be a string, got {kind}")]
    BrowserOptionNotString { name: String, kind: String },
    #[error("browser option {name} is not valid UTF-8")]
    BrowserOptionNotUtf8 { name: String },
    #[error("browser option args must be an array of strings, got {kind}")]
    BrowserOptionArgsNotArray { kind: String },
    #[error("browser option args entry {index} must be a string, got {kind}")]
    BrowserOptionArgNotString { index: usize, kind: String },
    #[error("browser option args entry {index} is not valid UTF-8")]
    BrowserOptionArgNotUtf8 { index: usize },
    #[error("browser cookies must be an array, got {kind}")]
    BrowserCookiesNotArray { kind: String },
    #[error("browser cookie entry {index} must be a table, got {kind}")]
    BrowserCookieNotTable { index: usize, kind: String },
    #[error("browser cookie entry {index} is missing field {field}")]
    BrowserCookieMissingField { index: usize, field: String },
    #[error("browser cookie entry {index} field {field} must be a string, got {kind}")]
    BrowserCookieFieldNotString {
        index: usize,
        field: String,
        kind: String,
    },
    #[error("browser cookie entry {index} field {field} is not valid UTF-8")]
    BrowserCookieFieldNotUtf8 { index: usize, field: String },
    #[error("browser cookie entry {index} field {field} must be a boolean, got {kind}")]
    BrowserCookieFieldNotBoolean {
        index: usize,
        field: String,
        kind: String,
    },
    #[error("browser cookie entry {index} field {field} must be a number, got {kind}")]
    BrowserCookieFieldNotNumber {
        index: usize,
        field: String,
        kind: String,
    },
    #[error("browser endpoint must be ws/wss/http/https URL, got {value}")]
    BrowserEndpointInvalid { value: String },
    #[error("browser operation failed while {context}: {message}")]
    BrowserOperation { context: String, message: String },
    #[error("script table is missing run function")]
    MissingRunFunction,
    #[error("failed to read script file {path:?}: {source}")]
    ScriptIo {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}
