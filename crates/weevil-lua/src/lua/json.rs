use mlua::{AnyUserData, Lua, MetaMethod, Table, UserData, UserDataMethods, Value};
use serde_json::{Map, Number, Value as JsonValue};
use thiserror::Error;

use crate::error::LuaPluginError;

#[derive(Clone, Copy, Debug)]
struct JsonNull;

impl UserData for JsonNull {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_meta_method(MetaMethod::ToString, |_, _, ()| Ok("json.null"));
        methods.add_meta_method(MetaMethod::Eq, |_, _, other: AnyUserData| {
            Ok(other.is::<JsonNull>())
        });
    }
}

#[derive(Debug, Error)]
enum JsonEncodeError {
    #[error("unsupported Lua value {kind} at {path}")]
    UnsupportedValue { kind: String, path: String },
    #[error("unsupported table key {kind} at {path}")]
    UnsupportedKey { kind: String, path: String },
    #[error("string is not valid UTF-8 at {path}")]
    InvalidUtf8 { path: String },
    #[error("number {value} is not finite at {path}")]
    NonFiniteNumber { value: f64, path: String },
    #[error("duplicate object key {key} at {path}")]
    DuplicateKey { key: String, path: String },
    #[error("missing array entry {index} at {path}")]
    MissingArrayEntry { index: usize, path: String },
    #[error("failed to serialize JSON: {source}")]
    Serialize {
        #[source]
        source: serde_json::Error,
    },
}

#[derive(Debug, Error)]
enum JsonDecodeError {
    #[error("json decode expects UTF-8 string")]
    InvalidUtf8,
    #[error("invalid JSON: {source}")]
    InvalidJson {
        #[source]
        source: serde_json::Error,
    },
    #[error("json number {value} is not finite")]
    InvalidNumber { value: String },
}

#[derive(Clone, Debug)]
struct JsonPath {
    value: String,
}

impl JsonPath {
    fn root() -> Self {
        Self {
            value: "$".to_string(),
        }
    }

    fn push_index(&self, index: usize) -> Self {
        let mut value = self.value.clone();
        value.push('[');
        value.push_str(&index.to_string());
        value.push(']');
        Self { value }
    }

    fn push_key(&self, key: &str) -> Self {
        let mut value = self.value.clone();
        value.push_str("[\"");
        value.push_str(&escape_key(key));
        value.push_str("\"]");
        Self { value }
    }
}

pub(crate) fn build_json_table(lua: &Lua) -> Result<Table, LuaPluginError> {
    let json = lua.create_table()?;
    let null = lua.create_userdata(JsonNull)?;
    json.set("null", null)?;
    json.set(
        "encode",
        lua.create_function(|_, value: Value| {
            let path = JsonPath::root();
            let json_value = lua_value_to_json(value, &path)?;
            serde_json::to_string(&json_value)
                .map_err(|err| mlua::Error::external(JsonEncodeError::Serialize { source: err }))
        })?,
    )?;
    json.set(
        "decode",
        lua.create_function(|lua, input: mlua::String| {
            let text = input
                .to_str()
                .map_err(|_| mlua::Error::external(JsonDecodeError::InvalidUtf8))?;
            let json_value: JsonValue = serde_json::from_str(text.as_ref()).map_err(|err| {
                mlua::Error::external(JsonDecodeError::InvalidJson { source: err })
            })?;
            json_to_lua(lua, json_value)
        })?,
    )?;
    Ok(json)
}

fn lua_value_to_json(value: Value, path: &JsonPath) -> mlua::Result<JsonValue> {
    match value {
        Value::Nil => Ok(JsonValue::Null),
        Value::Boolean(value) => Ok(JsonValue::Bool(value)),
        Value::Integer(value) => Ok(JsonValue::Number(Number::from(value))),
        Value::Number(value) => {
            if !value.is_finite() {
                return Err(mlua::Error::external(JsonEncodeError::NonFiniteNumber {
                    value,
                    path: path.value.clone(),
                }));
            }
            let number = Number::from_f64(value).ok_or_else(|| {
                mlua::Error::external(JsonEncodeError::NonFiniteNumber {
                    value,
                    path: path.value.clone(),
                })
            })?;
            Ok(JsonValue::Number(number))
        }
        Value::String(value) => {
            let text = value.to_str().map_err(|_| {
                mlua::Error::external(JsonEncodeError::InvalidUtf8 {
                    path: path.value.clone(),
                })
            })?;
            Ok(JsonValue::String(text.to_string()))
        }
        Value::Table(table) => table_to_json(table, path),
        Value::UserData(userdata) => {
            if userdata.is::<JsonNull>() {
                Ok(JsonValue::Null)
            } else {
                Err(mlua::Error::external(JsonEncodeError::UnsupportedValue {
                    kind: "userdata".to_string(),
                    path: path.value.clone(),
                }))
            }
        }
        other => Err(mlua::Error::external(JsonEncodeError::UnsupportedValue {
            kind: lua_value_kind(&other).to_string(),
            path: path.value.clone(),
        })),
    }
}

fn table_to_json(table: Table, path: &JsonPath) -> mlua::Result<JsonValue> {
    let mut array_entries: Vec<(usize, Value)> = Vec::new();
    let mut object_entries: Vec<(String, Value)> = Vec::new();
    let mut has_string_keys = false;

    for pair in table.pairs::<Value, Value>() {
        let (key, value) = pair?;
        match key {
            Value::String(key) => {
                let text = key.to_str().map_err(|_| {
                    mlua::Error::external(JsonEncodeError::InvalidUtf8 {
                        path: path.value.clone(),
                    })
                })?;
                has_string_keys = true;
                object_entries.push((text.to_string(), value));
            }
            Value::Integer(key) => match usize::try_from(key) {
                Ok(index) if key >= 1 => array_entries.push((index, value)),
                _ => {
                    has_string_keys = true;
                    object_entries.push((key.to_string(), value));
                }
            },
            Value::Number(key) => match number_key_to_index(key, path)? {
                Some(index) => array_entries.push((index, value)),
                None => {
                    has_string_keys = true;
                    object_entries.push((key.to_string(), value));
                }
            },
            other => {
                return Err(mlua::Error::external(JsonEncodeError::UnsupportedKey {
                    kind: lua_value_kind(&other).to_string(),
                    path: path.value.clone(),
                }));
            }
        }
    }

    if !has_string_keys {
        if array_entries.is_empty() {
            return Ok(JsonValue::Array(Vec::new()));
        }
        let max_index = array_entries
            .iter()
            .map(|(index, _)| *index)
            .max()
            .unwrap_or(0);
        let mut seen = vec![false; max_index];
        for (index, _) in &array_entries {
            seen[index - 1] = true;
        }
        if seen.iter().all(|value| *value) {
            let mut values = vec![None; max_index];
            for (index, value) in array_entries {
                values[index - 1] = Some(value);
            }
            let mut output = Vec::with_capacity(max_index);
            for (offset, value) in values.into_iter().enumerate() {
                let index = offset + 1;
                let value = value.ok_or_else(|| {
                    mlua::Error::external(JsonEncodeError::MissingArrayEntry {
                        index,
                        path: path.value.clone(),
                    })
                })?;
                let next_path = path.push_index(index);
                output.push(lua_value_to_json(value, &next_path)?);
            }
            return Ok(JsonValue::Array(output));
        }
    }

    let mut object = Map::new();
    for (index, value) in array_entries {
        let key = index.to_string();
        insert_object_value(&mut object, key, value, path)?;
    }
    for (key, value) in object_entries {
        insert_object_value(&mut object, key, value, path)?;
    }
    Ok(JsonValue::Object(object))
}

fn insert_object_value(
    object: &mut Map<String, JsonValue>,
    key: String,
    value: Value,
    path: &JsonPath,
) -> mlua::Result<()> {
    if object.contains_key(&key) {
        return Err(mlua::Error::external(JsonEncodeError::DuplicateKey {
            key,
            path: path.value.clone(),
        }));
    }
    let next_path = path.push_key(&key);
    let json_value = lua_value_to_json(value, &next_path)?;
    object.insert(key, json_value);
    Ok(())
}

fn number_key_to_index(value: f64, path: &JsonPath) -> Result<Option<usize>, mlua::Error> {
    if !value.is_finite() {
        return Err(mlua::Error::external(JsonEncodeError::NonFiniteNumber {
            value,
            path: path.value.clone(),
        }));
    }
    if value.fract() != 0.0 {
        return Ok(None);
    }
    let text = value.to_string();
    let integer = match text.parse::<i64>() {
        Ok(value) => value,
        Err(_) => return Ok(None),
    };
    if integer < 1 {
        return Ok(None);
    }
    let index = match usize::try_from(integer) {
        Ok(value) => value,
        Err(_) => return Ok(None),
    };
    Ok(Some(index))
}

fn json_to_lua(lua: &Lua, value: JsonValue) -> mlua::Result<Value> {
    match value {
        JsonValue::Null => Ok(Value::UserData(lua.create_userdata(JsonNull)?)),
        JsonValue::Bool(value) => Ok(Value::Boolean(value)),
        JsonValue::Number(value) => json_number_to_lua(value),
        JsonValue::String(value) => Ok(Value::String(lua.create_string(&value)?)),
        JsonValue::Array(values) => {
            let table = lua.create_table_with_capacity(values.len(), 0)?;
            for (index, value) in values.into_iter().enumerate() {
                let index = index + 1;
                let entry = json_to_lua(lua, value)?;
                table.set(index, entry)?;
            }
            Ok(Value::Table(table))
        }
        JsonValue::Object(values) => {
            let table = lua.create_table_with_capacity(0, values.len())?;
            for (key, value) in values {
                let entry = json_to_lua(lua, value)?;
                table.set(key, entry)?;
            }
            Ok(Value::Table(table))
        }
    }
}

fn json_number_to_lua(value: Number) -> mlua::Result<Value> {
    if let Some(value) = value.as_i64() {
        return Ok(Value::Integer(value));
    }
    if let Some(value) = value.as_u64() {
        match i64::try_from(value) {
            Ok(value) => return Ok(Value::Integer(value)),
            Err(_) => {
                let text = value.to_string();
                let float = text.parse::<f64>().map_err(|_| {
                    mlua::Error::external(JsonDecodeError::InvalidNumber {
                        value: text.clone(),
                    })
                })?;
                if !float.is_finite() {
                    return Err(mlua::Error::external(JsonDecodeError::InvalidNumber {
                        value: text,
                    }));
                }
                return Ok(Value::Number(float));
            }
        }
    }
    if let Some(value) = value.as_f64() {
        if !value.is_finite() {
            return Err(mlua::Error::external(JsonDecodeError::InvalidNumber {
                value: value.to_string(),
            }));
        }
        return Ok(Value::Number(value));
    }
    Err(mlua::Error::external(JsonDecodeError::InvalidNumber {
        value: value.to_string(),
    }))
}

fn escape_key(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '\\' => output.push_str("\\\\"),
            '"' => output.push_str("\\\""),
            _ => output.push(ch),
        }
    }
    output
}

fn lua_value_kind(value: &Value) -> &'static str {
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
