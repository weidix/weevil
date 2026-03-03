use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::Arc;

use mlua::{IntoLuaMulti, Lua, RegistryKey, Value};

use crate::error::LuaPluginError;
use crate::http::{HttpClient, TrustedUrl};
use crate::lua::{
    BrowserMode, HttpMode, LogContext, install_module, set_browser, set_http, set_log_context,
};

#[derive(Debug, Clone)]
pub struct LuaPluginSpec {
    alias: String,
    trusted_urls: Vec<TrustedUrl>,
    has_run: bool,
}

impl LuaPluginSpec {
    pub fn alias(&self) -> &str {
        &self.alias
    }

    pub fn trusted_urls(&self) -> &[TrustedUrl] {
        &self.trusted_urls
    }

    pub fn has_run(&self) -> bool {
        self.has_run
    }
}

pub struct LuaPlugin {
    alias: String,
    lua: Lua,
    run_key: RegistryKey,
    trusted_urls: Vec<TrustedUrl>,
}

impl LuaPlugin {
    pub fn from_script(script: &str) -> Result<Self, LuaPluginError> {
        let lua = Lua::new();
        install_module(&lua, HttpMode::Disabled)?;
        let table = eval_script_table(&lua, script)?;
        let alias = parse_alias(&table)?;
        let trusted_urls = parse_trusted_urls(&table)?;
        let run = parse_run(&table)?;
        let run_key = lua.create_registry_value(run)?;
        let http = Arc::new(HttpClient::new(trusted_urls.clone())?);
        set_http(&lua, HttpMode::Enabled(http))?;
        #[cfg(feature = "browser")]
        let browser_mode = BrowserMode::Enabled;
        #[cfg(not(feature = "browser"))]
        let browser_mode = BrowserMode::Unsupported;
        set_browser(&lua, browser_mode)?;
        Ok(Self {
            alias,
            lua,
            run_key,
            trusted_urls,
        })
    }

    pub fn from_file(path: impl AsRef<Path>) -> Result<Self, LuaPluginError> {
        let path = path.as_ref();
        let contents = fs::read_to_string(path).map_err(|err| LuaPluginError::ScriptIo {
            path: path.to_path_buf(),
            source: err,
        })?;
        Self::from_script(&contents)
    }

    pub fn check(script: &str) -> Result<LuaPluginSpec, LuaPluginError> {
        let lua = Lua::new();
        install_module(&lua, HttpMode::Disabled)?;
        let table = eval_script_table(&lua, script)?;
        let alias = parse_alias(&table)?;
        let trusted_urls = parse_trusted_urls(&table)?;
        let has_run = table.contains_key("run")?;
        Ok(LuaPluginSpec {
            alias,
            trusted_urls,
            has_run,
        })
    }

    pub fn check_file(path: impl AsRef<Path>) -> Result<LuaPluginSpec, LuaPluginError> {
        let path = path.as_ref();
        let contents = fs::read_to_string(path).map_err(|err| LuaPluginError::ScriptIo {
            path: path.to_path_buf(),
            source: err,
        })?;
        Self::check(&contents)
    }

    pub fn trusted_urls(&self) -> &[TrustedUrl] {
        &self.trusted_urls
    }

    pub fn alias(&self) -> &str {
        &self.alias
    }

    pub fn lua(&self) -> &Lua {
        &self.lua
    }

    pub fn set_log_context(&self, task_id: impl Into<String>, task_type: impl Into<String>) {
        set_log_context(
            &self.lua,
            LogContext {
                task_id: task_id.into(),
                task_type: task_type.into(),
            },
        );
    }

    pub fn call<A>(&self, args: A) -> Result<Option<Value>, LuaPluginError>
    where
        A: IntoLuaMulti,
    {
        let run = self.lua.registry_value::<mlua::Function>(&self.run_key)?;
        let output = run.call::<Option<Value>>(args)?;
        Ok(output)
    }

    #[cfg(feature = "async")]
    pub async fn call_async<A>(&self, args: A) -> Result<Option<Value>, LuaPluginError>
    where
        A: IntoLuaMulti,
    {
        let run = self.lua.registry_value::<mlua::Function>(&self.run_key)?;
        let output = run.call_async::<Option<Value>>(args).await?;
        Ok(output)
    }
}

impl FromStr for LuaPlugin {
    type Err = LuaPluginError;

    fn from_str(script: &str) -> Result<Self, Self::Err> {
        LuaPlugin::from_script(script)
    }
}

pub fn check_script(script: &str) -> Result<LuaPluginSpec, LuaPluginError> {
    LuaPlugin::check(script)
}

pub fn script_alias(script: &str) -> Result<String, LuaPluginError> {
    let lua = Lua::new();
    install_module(&lua, HttpMode::Disabled)?;
    let table = eval_script_table(&lua, script)?;
    parse_alias(&table)
}

pub fn script_alias_file(path: impl AsRef<Path>) -> Result<String, LuaPluginError> {
    let path = path.as_ref();
    let contents = fs::read_to_string(path).map_err(|err| LuaPluginError::ScriptIo {
        path: path.to_path_buf(),
        source: err,
    })?;
    script_alias(&contents)
}

pub fn dedupe_script_paths_by_alias(paths: &[PathBuf]) -> Result<Vec<PathBuf>, LuaPluginError> {
    let mut deduped = Vec::with_capacity(paths.len());
    let mut seen_aliases = HashSet::new();

    for path in paths {
        let alias = script_alias_file(path)?;
        if seen_aliases.insert(alias) {
            deduped.push(path.clone());
        }
    }

    Ok(deduped)
}

fn eval_script_table(lua: &Lua, script: &str) -> Result<mlua::Table, LuaPluginError> {
    let value: Value = lua.load(script).eval()?;
    match value {
        Value::Nil => Err(LuaPluginError::ScriptReturnMissing),
        Value::Table(table) => Ok(table),
        other => Err(LuaPluginError::ScriptReturnNotTable {
            kind: value_kind(&other).to_string(),
        }),
    }
}

fn parse_trusted_urls(table: &mlua::Table) -> Result<Vec<TrustedUrl>, LuaPluginError> {
    let value: Value = table
        .get("trusted_urls")
        .map_err(|_| LuaPluginError::MissingTrustedUrls)?;
    let list = match value {
        Value::Table(list) => list,
        other => {
            return Err(LuaPluginError::InvalidTrustedUrlsType {
                kind: value_kind(&other).to_string(),
            });
        }
    };
    let mut urls = Vec::new();
    for (index, entry) in list.sequence_values::<Value>().enumerate() {
        let entry = entry?;
        let raw = match entry {
            Value::String(value) => value,
            other => {
                return Err(LuaPluginError::InvalidTrustedUrlEntry {
                    index: index + 1,
                    kind: value_kind(&other).to_string(),
                });
            }
        };
        let text = raw
            .to_str()
            .map_err(|_| LuaPluginError::TrustedUrlEntryNotUtf8 { index: index + 1 })?;
        urls.push(TrustedUrl::parse(text.as_ref())?);
    }
    Ok(urls)
}

fn parse_alias(table: &mlua::Table) -> Result<String, LuaPluginError> {
    let value: Value = table
        .get("alias")
        .map_err(|_| LuaPluginError::MissingAlias)?;
    let alias = match value {
        Value::Nil => return Err(LuaPluginError::MissingAlias),
        Value::String(value) => value,
        other => {
            return Err(LuaPluginError::InvalidAliasType {
                kind: value_kind(&other).to_string(),
            });
        }
    };

    let alias = alias
        .to_str()
        .map_err(|_| LuaPluginError::AliasNotUtf8)?
        .trim()
        .to_string();

    if alias.is_empty() {
        return Err(LuaPluginError::EmptyAlias);
    }

    Ok(alias)
}

fn parse_run(table: &mlua::Table) -> Result<mlua::Function, LuaPluginError> {
    table
        .get("run")
        .map_err(|_| LuaPluginError::MissingRunFunction)
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
