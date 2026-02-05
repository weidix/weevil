mod nfo;

use std::fmt;
use std::path::{Path, PathBuf};

use clap::{Parser, Subcommand};
use mlua::{LuaSerdeExt, Value};
use weevil_lua::LuaPlugin;

fn main() {
    if let Err(error) = run() {
        error.report();
        std::process::exit(error.exit_code());
    }
}

fn run() -> Result<(), AppError> {
    let cli = Cli::try_parse().map_err(AppError::Cli)?;
    match cli.command {
        Command::Name {
            name,
            script,
            output,
        } => run_lua_nfo(&name, &script, &output),
        Command::File => Err(AppError::NotImplemented { mode: "file" }),
        Command::Dir => Err(AppError::NotImplemented { mode: "dir" }),
        Command::Watch => Err(AppError::NotImplemented { mode: "watch" }),
    }
}

fn run_lua_nfo(name: &str, script: &Path, output: &Path) -> Result<(), AppError> {
    let plugin = LuaPlugin::from_file(script).map_err(AppError::LuaPlugin)?;
    let value = plugin.call((name,)).map_err(AppError::LuaPlugin)?;
    let xml = render_nfo_output(value, plugin.lua())?;
    std::fs::write(output, xml).map_err(|err| AppError::OutputWrite {
        path: output.to_path_buf(),
        source: err,
    })?;
    Ok(())
}

fn render_nfo_output(value: Option<Value>, lua: &mlua::Lua) -> Result<String, AppError> {
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
            let xml = quick_xml::se::to_string(&movie).map_err(AppError::SerializeNfo)?;
            Ok(xml)
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

#[derive(Debug, Parser)]
#[command(
    name = "weevil",
    about = "Command-line toolkit for scraping NFO metadata",
    arg_required_else_help = true
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    #[command(
        name = "name",
        about = "Generate NFO output by name with a Lua script.",
        after_help = "Notes:\n  The Lua run function must return either:\n    - a table matching the NFO movie schema, or\n    - a string containing raw NFO XML."
    )]
    Name {
        #[arg(long, short = 'n', value_name = "NAME")]
        name: String,
        #[arg(long, short = 's', value_name = "SCRIPT")]
        script: PathBuf,
        #[arg(long, short = 'o', value_name = "OUTPUT")]
        output: PathBuf,
    },
    #[command(name = "file", about = "Reserved: file to file mode.")]
    File,
    #[command(name = "dir", about = "Reserved: directory to directory mode.")]
    Dir,
    #[command(name = "watch", about = "Reserved: directory watch mode.")]
    Watch,
}

#[derive(Debug)]
enum AppError {
    Cli(clap::Error),
    LuaPlugin(weevil_lua::LuaPluginError),
    LuaValue(mlua::Error),
    OutputWrite {
        path: PathBuf,
        source: std::io::Error,
    },
    ScriptReturnedNil,
    ScriptReturnedUnexpected {
        kind: String,
    },
    ScriptOutputInvalidUtf8,
    SerializeNfo(quick_xml::se::SeError),
    NotImplemented {
        mode: &'static str,
    },
}

impl AppError {
    fn exit_code(&self) -> i32 {
        match self {
            AppError::Cli(error) => error.exit_code(),
            _ => 1,
        }
    }

    fn report(&self) {
        match self {
            AppError::Cli(error) => {
                if let Err(io_error) = error.print() {
                    eprintln!("failed to print CLI error: {io_error}");
                }
            }
            other => {
                eprintln!("{other}");
            }
        }
    }
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AppError::Cli(error) => write!(f, "{error}"),
            AppError::LuaPlugin(error) => write!(f, "{error}"),
            AppError::LuaValue(error) => write!(f, "failed to decode Lua value: {error}"),
            AppError::OutputWrite { path, source } => {
                write!(f, "failed to write output file {path:?}: {source}")
            }
            AppError::ScriptReturnedNil => {
                write!(f, "Lua script returned nil; expected a table or XML string")
            }
            AppError::ScriptReturnedUnexpected { kind } => write!(
                f,
                "Lua script returned {kind}; expected a table or XML string"
            ),
            AppError::ScriptOutputInvalidUtf8 => {
                write!(f, "Lua script returned a string that is not valid UTF-8")
            }
            AppError::SerializeNfo(error) => {
                write!(f, "failed to serialize NFO XML: {error}")
            }
            AppError::NotImplemented { mode } => {
                write!(f, "mode {mode} is reserved but not implemented yet")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_name_command() {
        let cli = Cli::try_parse_from([
            "weevil",
            "name",
            "--name",
            "Spirited Away",
            "--script",
            "script.lua",
            "--output",
            "movie.nfo",
        ])
        .expect("expected command");

        if let Command::Name {
            name,
            script,
            output,
        } = cli.command
        {
            assert_eq!(name, "Spirited Away");
            assert_eq!(script, PathBuf::from("script.lua"));
            assert_eq!(output, PathBuf::from("movie.nfo"));
        } else {
            panic!("expected name command");
        }
    }

    #[test]
    fn parse_missing_mode_is_help() {
        let error = Cli::try_parse_from(["weevil"]).expect_err("expected error");
        assert!(matches!(
            error.kind(),
            clap::error::ErrorKind::DisplayHelp
                | clap::error::ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand
        ));
    }

    #[test]
    fn parse_unknown_mode() {
        let error = Cli::try_parse_from(["weevil", "oops"]).expect_err("expected error");
        assert!(matches!(
            error.kind(),
            clap::error::ErrorKind::UnknownArgument | clap::error::ErrorKind::InvalidSubcommand
        ));
    }

    #[test]
    fn parse_reserved_mode() {
        let cli = Cli::try_parse_from(["weevil", "file"]).expect("expected command");
        assert!(matches!(cli.command, Command::File));
    }

    #[test]
    fn parse_extra_args() {
        let error = Cli::try_parse_from([
            "weevil",
            "name",
            "--name",
            "Name",
            "--script",
            "script.lua",
            "--output",
            "movie.nfo",
            "extra",
        ])
        .expect_err("expected error");
        assert_eq!(error.kind(), clap::error::ErrorKind::UnknownArgument);
    }

    #[test]
    fn render_nfo_from_table() {
        let lua = mlua::Lua::new();
        let value: Value = lua
            .load(r#"{ title = "Spirited Away" }"#)
            .eval()
            .expect("expected value");
        let xml = render_nfo_output(Some(value), &lua).expect("expected xml");
        assert!(xml.contains("<movie>"));
        assert!(xml.contains("<title>Spirited Away</title>"));
    }

    #[test]
    fn render_nfo_from_string() {
        let lua = mlua::Lua::new();
        let text = lua.create_string("<movie />").expect("expected lua string");
        let xml = render_nfo_output(Some(Value::String(text)), &lua).expect("expected xml");
        assert_eq!(xml, "<movie />");
    }
}
