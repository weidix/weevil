// Usage:
//   cargo run -p weevil-lua --example run_script -- path/to/script.lua
//
// The Lua script must return a table with alias, trusted_urls, and run.
use mlua::Value;
use tracing_subscriber::fmt;
use weevil_lua::LuaPlugin;

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

    let plugin = LuaPlugin::from_file(&script_path)?;
    plugin.set_log_context("run_script", "lua");
    let result = plugin.call(())?;

    match result {
        None => {
            println!("no result");
        }
        Some(Value::String(text)) => {
            println!("{}", text.to_str()?);
        }
        Some(value) => {
            println!("{value:?}");
        }
    }

    Ok(())
}
