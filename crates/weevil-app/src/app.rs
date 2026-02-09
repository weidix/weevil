use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use clap::Parser;
use mlua::{LuaSerdeExt, Value};
use serde::Serialize;
use weevil_lua::LuaPlugin;

use crate::cli::{Cli, Command, FolderMultiStrategy};
use crate::config::{
    AppConfig, DirCliOverrides, ModeCliOverrides, NameCliOverrides, ResolvedModeConfig,
};
use crate::dir_mode;
use crate::errors::AppError;
use crate::file_mode;
use crate::mode_params::{FetchModeParams, FileModeParams, MultiFolderStrategy};
use crate::nfo;
use crate::watch_mode;

pub(crate) fn run() -> Result<(), AppError> {
    let cli = Cli::try_parse().map_err(AppError::Cli)?;
    let config = AppConfig::load(cli.config.as_deref())?;
    match cli.command {
        Command::Name {
            name,
            script,
            output,
        } => {
            let resolved = config.resolve_name_with(&NameCliOverrides { script, output })?;
            run_lua_nfo(&name, &resolved.script, &resolved.output)
        }
        Command::File {
            input,
            script,
            output,
            input_name_rules,
            folder_multi,
        } => {
            let resolved = config.resolve_file_mode_with(&ModeCliOverrides {
                script,
                output,
                input_name_rules,
                folder_multi,
                fetch_threads: None,
                throttle_same_script: None,
                script_throttle_base_ms: None,
            })?;
            let params = file_mode_params_from_config(resolved);
            file_mode::run_file_mode(&input, &params)
        }
        Command::Dir {
            input,
            script,
            output,
            input_name_rules,
            folder_multi,
            max_depth,
            fetch_threads,
            throttle_same_script,
            script_throttle_base_ms,
        } => {
            let resolved = config.resolve_dir_mode_with(&DirCliOverrides {
                input,
                mode: ModeCliOverrides {
                    script,
                    output,
                    input_name_rules,
                    folder_multi,
                    fetch_threads,
                    throttle_same_script,
                    script_throttle_base_ms,
                },
                max_depth,
            })?;
            let mode = resolved.mode;
            let params = file_mode_params_from_config(mode.clone());
            let fetch = fetch_mode_params_from_config(mode);
            dir_mode::run_dir_mode(&resolved.input, &params, &fetch, resolved.max_depth)
        }
        Command::Watch {
            input,
            script,
            output,
            input_name_rules,
            folder_multi,
            max_depth,
            fetch_threads,
            throttle_same_script,
            script_throttle_base_ms,
        } => {
            let resolved = config.resolve_watch_mode_with(&DirCliOverrides {
                input,
                mode: ModeCliOverrides {
                    script,
                    output,
                    input_name_rules,
                    folder_multi,
                    fetch_threads,
                    throttle_same_script,
                    script_throttle_base_ms,
                },
                max_depth,
            })?;
            let mode = resolved.mode;
            let params = file_mode_params_from_config(mode.clone());
            let fetch = fetch_mode_params_from_config(mode);
            watch_mode::run_watch_mode(&resolved.input, &params, &fetch, resolved.max_depth)
        }
    }
}

fn file_mode_params_from_config(resolved: ResolvedModeConfig) -> FileModeParams {
    FileModeParams::new(
        resolved.script,
        resolved.output,
        resolved.input_name_rules,
        map_folder_multi(resolved.folder_multi),
    )
}

fn fetch_mode_params_from_config(resolved: ResolvedModeConfig) -> FetchModeParams {
    FetchModeParams::new(
        resolved.fetch_threads,
        resolved.throttle_same_script,
        resolved.script_throttle_base_ms,
    )
}

#[cfg(test)]
mod config_mapping_tests {
    use super::*;

    #[test]
    fn fetch_mode_mapping_from_resolved_config() {
        let resolved = ResolvedModeConfig {
            script: "demo.lua".into(),
            output: "out/{title}".to_string(),
            input_name_rules: vec![],
            folder_multi: FolderMultiStrategy::First,
            fetch_threads: 0,
            throttle_same_script: true,
            script_throttle_base_ms: 1400,
        };
        let fetch = fetch_mode_params_from_config(resolved);
        assert_eq!(fetch.fetch_threads(), 0);
        assert!(fetch.throttle_same_script());
        assert_eq!(fetch.script_throttle_base_ms(), 1400);
        assert!(fetch.multithread_enabled());
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
