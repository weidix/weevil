use weevil_lua::LuaPlugin;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let script = r#"
return {
  trusted_urls = { "https://example.com/" },
  run = function()
    local html = weevil.http.get("https://example.com/")
    local tree = weevil.html.parse(html)
    local node = weevil.selector.parse("h1"):select_one(tree)
    if not node then return nil end
    return tree:text(node)
  end
}
"#;
    let plugin = LuaPlugin::from_str(script)?;
    let result = plugin.call(())?;
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
