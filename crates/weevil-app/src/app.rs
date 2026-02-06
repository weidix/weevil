use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use clap::Parser;
use mlua::{LuaSerdeExt, Value};
use serde::Serialize;
use weevil_lua::LuaPlugin;

use crate::cli::{Cli, Command, FolderMultiStrategy};
use crate::dir_mode;
use crate::errors::AppError;
use crate::file_mode;
use crate::mode_params::{FileModeParams, MultiFolderStrategy};
use crate::nfo;
use crate::watch_mode;

pub(crate) fn run() -> Result<(), AppError> {
    let cli = Cli::try_parse().map_err(AppError::Cli)?;
    match cli.command {
        Command::Name {
            name,
            script,
            output,
        } => run_lua_nfo(&name, &script, &output),
        Command::File {
            input,
            script,
            output,
            input_name_rules,
            folder_multi,
        } => {
            let params = FileModeParams::new(
                script,
                output,
                input_name_rules,
                map_folder_multi(folder_multi),
            );
            file_mode::run_file_mode(&input, &params)
        }
        Command::Dir {
            input,
            script,
            output,
            input_name_rules,
            folder_multi,
            max_depth,
        } => {
            let params = FileModeParams::new(
                script,
                output,
                input_name_rules,
                map_folder_multi(folder_multi),
            );
            dir_mode::run_dir_mode(&input, &params, max_depth)
        }
        Command::Watch {
            input,
            script,
            output,
            input_name_rules,
            folder_multi,
            max_depth,
        } => {
            let params = FileModeParams::new(
                script,
                output,
                input_name_rules,
                map_folder_multi(folder_multi),
            );
            watch_mode::run_watch_mode(&input, &params, max_depth)
        }
    }
}

fn map_folder_multi(strategy: FolderMultiStrategy) -> MultiFolderStrategy {
    match strategy {
        FolderMultiStrategy::HardLink => MultiFolderStrategy::HardLink,
        FolderMultiStrategy::SoftLink => MultiFolderStrategy::SoftLink,
        FolderMultiStrategy::First => MultiFolderStrategy::First,
    }
}

fn run_lua_nfo(name: &str, script: &Path, output: &Path) -> Result<(), AppError> {
    let task = TaskContext::new("name");
    let plugin = LuaPlugin::from_file(script).map_err(AppError::LuaPlugin)?;
    plugin.set_log_context(task.id.clone(), task.kind);
    let value = plugin.call((name,)).map_err(AppError::LuaPlugin)?;
    let xml = render_nfo_output(value, plugin.lua())?;
    std::fs::write(output, xml).map_err(|err| AppError::OutputWrite {
        path: output.to_path_buf(),
        source: err,
    })?;
    Ok(())
}

pub(crate) struct TaskContext {
    pub(crate) id: String,
    pub(crate) kind: &'static str,
}

impl TaskContext {
    pub(crate) fn new(kind: &'static str) -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default();
        let id = format!("{kind}-{}-{}", now.as_millis(), std::process::id());
        Self { id, kind }
    }
}

pub(crate) fn render_nfo_output(value: Option<Value>, lua: &mlua::Lua) -> Result<String, AppError> {
    let value = value.ok_or(AppError::ScriptReturnedNil)?;
    match value {
        Value::String(text) => {
            let output = text
                .to_str()
                .map_err(|_| AppError::ScriptOutputInvalidUtf8)?;
            Ok(output.to_string())
        }
        Value::Table(_) => {
            let movie: nfo::Movie = lua.from_value(value).map_err(AppError::LuaValue)?;
            let mut buffer = String::new();
            let mut serializer = quick_xml::se::Serializer::new(&mut buffer);
            serializer.indent(' ', 2);
            movie
                .serialize(serializer)
                .map_err(AppError::SerializeNfo)?;
            Ok(buffer)
        }
        other => Err(AppError::ScriptReturnedUnexpected {
            kind: value_kind(&other).to_string(),
        }),
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

#[cfg(test)]
#[path = "tests/app.rs"]
mod tests;
