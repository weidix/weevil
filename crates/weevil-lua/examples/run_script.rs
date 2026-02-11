// Usage:
//   cargo run -p weevil-lua --example run_script -- path/to/script.lua
//
// The Lua script is executed directly with the weevil module installed.
use std::fs;

use mlua::{Lua, Value};
use tracing_subscriber::fmt;
use weevil_lua::{HttpMode, install_module};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let script_path = match args.next() {
        Some(value) => value,
        None => {
            eprintln!(
                "Usage: run_script <script.lua>\n\
Pass the script path only."
            );
            std::process::exit(2);
        }
    };

    if args.next().is_some() {
        eprintln!("Too many arguments. Expected only the script path.");
        std::process::exit(2);
    }

    let _ = fmt::try_init();

    let script = fs::read_to_string(&script_path)?;
    let lua = Lua::new();
    install_module(&lua, HttpMode::Disabled)?;
    let result: Value = lua.load(&script).eval()?;

    match result {
        Value::Nil => {
            println!("no result");
        }
        Value::String(text) => {
            println!("{}", text.to_str()?);
        }
        value => {
            println!("{value:?}");
        }
    }

    Ok(())
}
