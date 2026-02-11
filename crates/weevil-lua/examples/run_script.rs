// Usage:
//   cargo run -p weevil-lua --example run_script -- path/to/script.lua [input-name] [input-path]
//
// The Lua script must return a table with "alias", "trusted_urls", and "run".
use weevil_lua::LuaPlugin;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let script_path = match args.next() {
        Some(value) => value,
        None => {
            eprintln!(
                "Usage: run_script <script.lua> [input-name] [input-path]\n\
Pass the script path followed by optional input name/path."
            );
            std::process::exit(2);
        }
    };

    let input_name = args.next();
    let input_path = args.next();
    if args.next().is_some() {
        eprintln!("Too many arguments. Expected at most 3.");
        std::process::exit(2);
    }

    let plugin = LuaPlugin::from_file(&script_path)?;
    let result = match (input_name.as_deref(), input_path.as_deref()) {
        (Some(name), Some(path)) => plugin.call((name, path))?,
        (Some(name), None) => plugin.call((name,))?,
        (None, None) => plugin.call(())?,
        (None, Some(_)) => {
            eprintln!("input-path requires input-name.");
            std::process::exit(2);
        }
    };

    match result {
        Some(value) => {
            if let mlua::Value::String(text) = value {
                println!("{}", text.to_str()?);
            } else {
                println!("{value:?}");
            }
        }
        None => {
            println!("no result");
        }
    }

    Ok(())
}
