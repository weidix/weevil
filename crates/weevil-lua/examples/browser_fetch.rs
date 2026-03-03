#[cfg(feature = "browser")]
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    use mlua::Value;
    use weevil_lua::LuaPlugin;

    let script = r#"
return {
  alias = "browser.demo",
  trusted_urls = {},
  run = function()
    local browser = weevil.browser.launch({ headless = true })
    local page = browser:new_page("https://example.com/")
    local title = page:title()
    browser:close()
    return title
  end
}
"#;

    let plugin = LuaPlugin::from_script(script)?;
    let output = plugin.call_async(()).await?;
    match output {
        Some(Value::String(text)) => println!("{}", text.to_str()?),
        Some(value) => println!("{value:?}"),
        None => println!("no result"),
    }
    Ok(())
}

#[cfg(not(feature = "browser"))]
fn main() {
    eprintln!("This example requires the browser feature.");
}
