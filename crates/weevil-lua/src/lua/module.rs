use std::sync::Arc;

use mlua::{Lua, Table, Value};
use reqwest::Version;
use weevil_core::{HtmlTree, Selector, XPath};

use crate::error::LuaPluginError;
use crate::http::{HttpClient, HttpRequestOptions};
use crate::lua::browser::{BrowserMode, build_browser_table};
#[cfg(feature = "json")]
use crate::lua::json::build_json_table;
use crate::lua::types::{LuaHtmlTree, LuaSelector, LuaXPath};

#[derive(Clone)]
pub enum HttpMode {
    Disabled,
    Enabled(Arc<HttpClient>),
}

#[derive(Clone, Debug)]
pub(crate) struct LogContext {
    pub(crate) task_id: String,
    pub(crate) task_type: String,
}

pub(crate) fn set_log_context(lua: &Lua, context: LogContext) {
    lua.set_app_data(context);
}

pub fn install_module(lua: &Lua, http_mode: HttpMode) -> Result<(), LuaPluginError> {
    let weevil = lua.create_table()?;
    weevil.set("html", build_html_table(lua)?)?;
    weevil.set("selector", build_selector_table(lua)?)?;
    weevil.set("xpath", build_xpath_table(lua)?)?;
    weevil.set("http", build_http_table(lua, http_mode)?)?;
    weevil.set("browser", build_browser_table(lua, BrowserMode::Disabled)?)?;
    weevil.set("log", build_log_table(lua)?)?;
    #[cfg(feature = "json")]
    weevil.set("json", build_json_table(lua)?)?;
    lua.globals().set("weevil", weevil)?;
    Ok(())
}

pub fn set_http(lua: &Lua, http_mode: HttpMode) -> Result<(), LuaPluginError> {
    let globals = lua.globals();
    let weevil: Table = globals.get("weevil")?;
    let http = build_http_table(lua, http_mode)?;
    weevil.set("http", http)?;
    Ok(())
}

pub fn set_browser(lua: &Lua, browser_mode: BrowserMode) -> Result<(), LuaPluginError> {
    let globals = lua.globals();
    let weevil: Table = globals.get("weevil")?;
    let browser = build_browser_table(lua, browser_mode)?;
    weevil.set("browser", browser)?;
    Ok(())
}

fn build_html_table(lua: &Lua) -> Result<Table, LuaPluginError> {
    let html = lua.create_table()?;
    html.set(
        "parse",
        lua.create_function(|_, input: String| Ok(LuaHtmlTree::new(HtmlTree::parse(&input))))?,
    )?;
    html.set(
        "parse_bytes",
        lua.create_function(|_, input: mlua::String| {
            Ok(LuaHtmlTree::new(HtmlTree::parse_bytes(
                input.as_bytes().as_ref(),
            )))
        })?,
    )?;
    html.set(
        "parse_checked",
        lua.create_function(|_, input: String| {
            HtmlTree::parse_checked(&input)
                .map(LuaHtmlTree::new)
                .map_err(mlua::Error::external)
        })?,
    )?;
    html.set(
        "parse_bytes_checked",
        lua.create_function(|_, input: mlua::String| {
            HtmlTree::parse_bytes_checked(input.as_bytes().as_ref())
                .map(LuaHtmlTree::new)
                .map_err(mlua::Error::external)
        })?,
    )?;
    html.set(
        "parse_with_errors",
        lua.create_function(|lua, input: String| {
            let output = HtmlTree::parse_with_errors(&input);
            let table = lua.create_table()?;
            table.set("tree", LuaHtmlTree::new(output.tree))?;
            let errors = output
                .errors
                .into_iter()
                .map(|issue| issue.message().to_string())
                .collect::<Vec<_>>();
            table.set("errors", errors)?;
            Ok(table)
        })?,
    )?;
    html.set(
        "parse_bytes_with_errors",
        lua.create_function(|lua, input: mlua::String| {
            let output = HtmlTree::parse_bytes_with_errors(input.as_bytes().as_ref());
            let table = lua.create_table()?;
            table.set("tree", LuaHtmlTree::new(output.tree))?;
            let errors = output
                .errors
                .into_iter()
                .map(|issue| issue.message().to_string())
                .collect::<Vec<_>>();
            table.set("errors", errors)?;
            Ok(table)
        })?,
    )?;
    Ok(html)
}

fn build_selector_table(lua: &Lua) -> Result<Table, LuaPluginError> {
    let selector = lua.create_table()?;
    selector.set(
        "parse",
        lua.create_function(|_, input: String| {
            Selector::parse(&input)
                .map(LuaSelector)
                .map_err(mlua::Error::external)
        })?,
    )?;
    Ok(selector)
}

fn build_xpath_table(lua: &Lua) -> Result<Table, LuaPluginError> {
    let xpath = lua.create_table()?;
    xpath.set(
        "parse",
        lua.create_function(|_, input: String| {
            XPath::parse(&input)
                .map(LuaXPath)
                .map_err(mlua::Error::external)
        })?,
    )?;
    Ok(xpath)
}

fn build_http_table(lua: &Lua, http_mode: HttpMode) -> Result<Table, LuaPluginError> {
    let http = lua.create_table()?;
    match http_mode {
        HttpMode::Disabled => {
            #[cfg(feature = "async")]
            {
                http.set(
                    "get",
                    lua.create_async_function(|_, _: mlua::Variadic<Value>| async move {
                        Err::<String, _>(mlua::Error::external(LuaPluginError::HttpDisabled))
                    })?,
                )?;
                http.set(
                    "post",
                    lua.create_async_function(|_, _: mlua::Variadic<Value>| async move {
                        Err::<String, _>(mlua::Error::external(LuaPluginError::HttpDisabled))
                    })?,
                )?;
            }
            #[cfg(not(feature = "async"))]
            {
                http.set(
                    "get",
                    lua.create_function(|_, _: mlua::Variadic<Value>| -> mlua::Result<String> {
                        Err(mlua::Error::external(LuaPluginError::HttpDisabled))
                    })?,
                )?;
                http.set(
                    "post",
                    lua.create_function(|_, _: mlua::Variadic<Value>| -> mlua::Result<String> {
                        Err(mlua::Error::external(LuaPluginError::HttpDisabled))
                    })?,
                )?;
            }
        }
        HttpMode::Enabled(client) => {
            #[cfg(not(feature = "async"))]
            {
                let blocking = client.clone();
                http.set(
                    "get",
                    lua.create_function(move |_, (url, options): (String, Option<Value>)| {
                        let options = parse_http_options(options).map_err(mlua::Error::external)?;
                        blocking
                            .get_blocking(&url, &options)
                            .map_err(mlua::Error::external)
                    })?,
                )?;
                let blocking = client.clone();
                http.set(
                    "post",
                    lua.create_function(
                        move |_, (url, body, options): (String, String, Option<Value>)| {
                            let options =
                                parse_http_options(options).map_err(mlua::Error::external)?;
                            blocking
                                .post_blocking(&url, &body, &options)
                                .map_err(mlua::Error::external)
                        },
                    )?,
                )?;
            }
            #[cfg(feature = "async")]
            {
                let async_client = client.clone();
                http.set(
                    "get",
                    lua.create_async_function(
                        move |_, (url, options): (String, Option<Value>)| {
                            let async_client = async_client.clone();
                            async move {
                                let options =
                                    parse_http_options(options).map_err(mlua::Error::external)?;
                                async_client
                                    .get_async(&url, &options)
                                    .await
                                    .map_err(mlua::Error::external)
                            }
                        },
                    )?,
                )?;
                let async_client = client.clone();
                http.set(
                    "post",
                    lua.create_async_function(
                        move |_, (url, body, options): (String, String, Option<Value>)| {
                            let async_client = async_client.clone();
                            async move {
                                let options =
                                    parse_http_options(options).map_err(mlua::Error::external)?;
                                async_client
                                    .post_async(&url, &body, &options)
                                    .await
                                    .map_err(mlua::Error::external)
                            }
                        },
                    )?,
                )?;
            }
        }
    }
    Ok(http)
}

fn build_log_table(lua: &Lua) -> Result<Table, LuaPluginError> {
    let log = lua.create_table()?;
    log.set(
        "debug",
        lua.create_function(|lua, values: mlua::Variadic<Value>| {
            let message = format_variadic(values);
            let (task_id, task_type) = log_context(lua);
            tracing::debug!(
                target: "weevil.lua",
                task_id = %task_id,
                task_type = %task_type,
                "{}",
                message
            );
            Ok(())
        })?,
    )?;
    log.set(
        "info",
        lua.create_function(|lua, values: mlua::Variadic<Value>| {
            let message = format_variadic(values);
            let (task_id, task_type) = log_context(lua);
            tracing::info!(
                target: "weevil.lua",
                task_id = %task_id,
                task_type = %task_type,
                "{}",
                message
            );
            Ok(())
        })?,
    )?;
    log.set(
        "warn",
        lua.create_function(|lua, values: mlua::Variadic<Value>| {
            let message = format_variadic(values);
            let (task_id, task_type) = log_context(lua);
            tracing::warn!(
                target: "weevil.lua",
                task_id = %task_id,
                task_type = %task_type,
                "{}",
                message
            );
            Ok(())
        })?,
    )?;
    log.set(
        "error",
        lua.create_function(|lua, values: mlua::Variadic<Value>| {
            let message = format_variadic(values);
            let (task_id, task_type) = log_context(lua);
            tracing::error!(
                target: "weevil.lua",
                task_id = %task_id,
                task_type = %task_type,
                "{}",
                message
            );
            Ok(())
        })?,
    )?;
    Ok(log)
}

fn parse_http_options(options: Option<Value>) -> Result<HttpRequestOptions, LuaPluginError> {
    let Some(options) = options else {
        return Ok(HttpRequestOptions::default());
    };
    match options {
        Value::Nil => Ok(HttpRequestOptions::default()),
        Value::Table(table) => parse_http_options_table(table),
        other => Err(LuaPluginError::HttpOptionsNotTable {
            kind: value_kind(&other).to_string(),
        }),
    }
}

fn parse_http_options_table(table: Table) -> Result<HttpRequestOptions, LuaPluginError> {
    let headers_value: Value = table.get("headers")?;
    let version_value: Value = table.get("version")?;
    let has_headers = !matches!(headers_value, Value::Nil);
    let has_version = !matches!(version_value, Value::Nil);
    if has_headers || has_version {
        let mut options = HttpRequestOptions::default();
        if has_headers {
            match headers_value {
                Value::Table(headers_table) => {
                    options.headers = parse_header_table(headers_table)?;
                }
                Value::Nil => {}
                other => {
                    return Err(LuaPluginError::HttpOptionsHeadersNotTable {
                        kind: value_kind(&other).to_string(),
                    });
                }
            }
        }
        if has_version {
            match version_value {
                Value::String(value) => {
                    let value = value
                        .to_str()
                        .map_err(|_| LuaPluginError::HttpVersionNotUtf8)?;
                    options.version = Some(parse_http_version(value.as_ref())?);
                }
                Value::Nil => {}
                other => {
                    return Err(LuaPluginError::HttpVersionNotString {
                        kind: value_kind(&other).to_string(),
                    });
                }
            }
        }
        return Ok(options);
    }
    let headers = parse_header_table(table)?;
    Ok(HttpRequestOptions {
        headers,
        version: None,
    })
}

fn parse_http_version(value: &str) -> Result<Version, LuaPluginError> {
    let normalized = value.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "1" | "1.1" | "http1" | "http1.1" | "http/1.1" => Ok(Version::HTTP_11),
        "2" | "h2" | "http2" | "http/2" => Ok(Version::HTTP_2),
        _ => Err(LuaPluginError::HttpVersionUnsupported {
            value: value.to_string(),
        }),
    }
}

fn parse_header_table(table: Table) -> Result<Vec<(String, String)>, LuaPluginError> {
    let mut headers = Vec::new();
    for pair in table.pairs::<Value, Value>() {
        let (key, value) = pair?;
        let name = match key {
            Value::String(value) => value
                .to_str()
                .map_err(|_| LuaPluginError::HttpHeaderNameNotUtf8)?
                .to_string(),
            other => {
                return Err(LuaPluginError::HttpHeaderNameNotString {
                    kind: value_kind(&other).to_string(),
                });
            }
        };
        let value = match value {
            Value::String(value) => value
                .to_str()
                .map_err(|_| LuaPluginError::HttpHeaderValueNotUtf8 { name: name.clone() })?
                .to_string(),
            other => {
                return Err(LuaPluginError::HttpHeaderValueNotString {
                    name,
                    kind: value_kind(&other).to_string(),
                });
            }
        };
        headers.push((name, value));
    }
    Ok(headers)
}

fn log_context(lua: &Lua) -> (String, String) {
    if let Some(context) = lua.app_data_ref::<LogContext>() {
        (context.task_id.clone(), context.task_type.clone())
    } else {
        ("unknown".to_string(), "unknown".to_string())
    }
}

fn format_variadic(values: mlua::Variadic<Value>) -> String {
    if values.is_empty() {
        return String::new();
    }
    let mut parts = Vec::with_capacity(values.len());
    for value in values {
        parts.push(format_lua_value(value));
    }
    parts.join(" ")
}

fn format_lua_value(value: Value) -> String {
    match value {
        Value::Nil => "nil".to_string(),
        Value::Boolean(value) => value.to_string(),
        Value::Integer(value) => value.to_string(),
        Value::Number(value) => value.to_string(),
        Value::String(value) => String::from_utf8_lossy(value.as_bytes().as_ref()).into_owned(),
        other => format!("<{}>", value_kind(&other)),
    }
}

fn value_kind(value: &Value) -> &'static str {
    match value {
        Value::Nil => "nil",
        Value::Boolean(_) => "boolean",
        Value::LightUserData(_) => "lightuserdata",
        Value::Integer(_) => "integer",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Table(_) => "table",
        Value::Function(_) => "function",
        Value::Thread(_) => "thread",
        Value::UserData(_) => "userdata",
        Value::Error(_) => "error",
        Value::Other(_) => "other",
    }
}
