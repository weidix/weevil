use mlua::{Lua, Table, Value};
use serde_json::Value as JsonValue;

use crate::browser::{BrowserCookie, BrowserCookieInput};
use crate::error::LuaPluginError;

pub(super) fn parse_cookie_inputs(value: Value) -> Result<Vec<BrowserCookieInput>, LuaPluginError> {
    let table = match value {
        Value::Table(table) => table,
        other => {
            return Err(LuaPluginError::BrowserCookiesNotArray {
                kind: value_kind(&other).to_string(),
            });
        }
    };
    parse_cookie_array(table)
}

fn parse_cookie_array(table: Table) -> Result<Vec<BrowserCookieInput>, LuaPluginError> {
    let mut pair_count = 0usize;
    for pair in table.pairs::<Value, Value>() {
        pair?;
        pair_count += 1;
    }
    if pair_count != table.raw_len() {
        return Err(LuaPluginError::BrowserCookiesNotArray {
            kind: "non-array-table".to_string(),
        });
    }

    let mut cookies = Vec::new();
    for (index, entry) in table.sequence_values::<Value>().enumerate() {
        let entry = entry?;
        let index = index + 1;
        let table = match entry {
            Value::Table(table) => table,
            other => {
                return Err(LuaPluginError::BrowserCookieNotTable {
                    index,
                    kind: value_kind(&other).to_string(),
                });
            }
        };
        cookies.push(parse_cookie_table(table, index)?);
    }
    Ok(cookies)
}

fn parse_cookie_table(table: Table, index: usize) -> Result<BrowserCookieInput, LuaPluginError> {
    Ok(BrowserCookieInput {
        name: parse_cookie_required_string(&table, index, "name")?,
        value: parse_cookie_required_string(&table, index, "value")?,
        url: parse_cookie_optional_string(&table, index, "url")?,
        domain: parse_cookie_optional_string(&table, index, "domain")?,
        path: parse_cookie_optional_string(&table, index, "path")?,
        secure: parse_cookie_optional_bool(&table, index, "secure")?,
        http_only: parse_cookie_optional_bool(&table, index, "http_only")?,
        expires: parse_cookie_optional_number(&table, index, "expires")?,
    })
}

fn parse_cookie_required_string(
    table: &Table,
    index: usize,
    field: &str,
) -> Result<String, LuaPluginError> {
    let value: Value = table.get(field)?;
    match value {
        Value::String(text) => text.to_str().map(|value| value.to_string()).map_err(|_| {
            LuaPluginError::BrowserCookieFieldNotUtf8 {
                index,
                field: field.to_string(),
            }
        }),
        Value::Nil => Err(LuaPluginError::BrowserCookieMissingField {
            index,
            field: field.to_string(),
        }),
        other => Err(LuaPluginError::BrowserCookieFieldNotString {
            index,
            field: field.to_string(),
            kind: value_kind(&other).to_string(),
        }),
    }
}

fn parse_cookie_optional_string(
    table: &Table,
    index: usize,
    field: &str,
) -> Result<Option<String>, LuaPluginError> {
    let value: Value = table.get(field)?;
    match value {
        Value::Nil => Ok(None),
        Value::String(text) => text
            .to_str()
            .map(|value| Some(value.to_string()))
            .map_err(|_| LuaPluginError::BrowserCookieFieldNotUtf8 {
                index,
                field: field.to_string(),
            }),
        other => Err(LuaPluginError::BrowserCookieFieldNotString {
            index,
            field: field.to_string(),
            kind: value_kind(&other).to_string(),
        }),
    }
}

fn parse_cookie_optional_bool(
    table: &Table,
    index: usize,
    field: &str,
) -> Result<Option<bool>, LuaPluginError> {
    let value: Value = table.get(field)?;
    match value {
        Value::Nil => Ok(None),
        Value::Boolean(flag) => Ok(Some(flag)),
        other => Err(LuaPluginError::BrowserCookieFieldNotBoolean {
            index,
            field: field.to_string(),
            kind: value_kind(&other).to_string(),
        }),
    }
}

fn parse_cookie_optional_number(
    table: &Table,
    index: usize,
    field: &str,
) -> Result<Option<f64>, LuaPluginError> {
    let value: Value = table.get(field)?;
    match value {
        Value::Nil => Ok(None),
        Value::Integer(number) => Ok(Some(number as f64)),
        Value::Number(number) => Ok(Some(number)),
        other => Err(LuaPluginError::BrowserCookieFieldNotNumber {
            index,
            field: field.to_string(),
            kind: value_kind(&other).to_string(),
        }),
    }
}

pub(super) fn cookies_to_lua(lua: &Lua, cookies: Vec<BrowserCookie>) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    for (index, cookie) in cookies.into_iter().enumerate() {
        let cookie_table = lua.create_table()?;
        cookie_table.set("name", cookie.name)?;
        cookie_table.set("value", cookie.value)?;
        cookie_table.set("domain", cookie.domain)?;
        cookie_table.set("path", cookie.path)?;
        cookie_table.set("expires", cookie.expires)?;
        cookie_table.set("secure", cookie.secure)?;
        cookie_table.set("http_only", cookie.http_only)?;
        cookie_table.set("session", cookie.session)?;
        table.raw_set(index + 1, cookie_table)?;
    }
    Ok(table)
}

pub(super) fn json_to_lua(lua: &Lua, value: JsonValue) -> mlua::Result<Value> {
    match value {
        JsonValue::Null => Ok(Value::Nil),
        JsonValue::Bool(value) => Ok(Value::Boolean(value)),
        JsonValue::Number(value) => {
            if let Some(value) = value.as_i64() {
                Ok(Value::Integer(value))
            } else if let Some(value) = value.as_u64() {
                if let Ok(value) = i64::try_from(value) {
                    Ok(Value::Integer(value))
                } else {
                    Ok(Value::Number(value as f64))
                }
            } else if let Some(value) = value.as_f64() {
                Ok(Value::Number(value))
            } else {
                Ok(Value::Nil)
            }
        }
        JsonValue::String(value) => Ok(Value::String(lua.create_string(&value)?)),
        JsonValue::Array(values) => {
            let table = lua.create_table()?;
            for (index, value) in values.into_iter().enumerate() {
                table.raw_set(index + 1, json_to_lua(lua, value)?)?;
            }
            Ok(Value::Table(table))
        }
        JsonValue::Object(values) => {
            let table = lua.create_table()?;
            for (name, value) in values {
                table.set(name, json_to_lua(lua, value)?)?;
            }
            Ok(Value::Table(table))
        }
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
