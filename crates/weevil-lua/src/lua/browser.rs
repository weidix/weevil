use mlua::{Lua, Table, Value};
#[cfg(feature = "browser")]
use mlua::{UserData, UserDataMethods};

#[cfg(feature = "browser")]
use crate::browser::{BrowserLaunchOptions, BrowserPage, BrowserSession};
use crate::error::LuaPluginError;
#[cfg(feature = "browser")]
use crate::lua::browser_codec::{cookies_to_lua, json_to_lua, parse_cookie_inputs};

#[derive(Clone, Copy)]
pub enum BrowserMode {
    Disabled,
    #[cfg(not(feature = "browser"))]
    Unsupported,
    #[cfg(feature = "browser")]
    Enabled,
}

#[cfg(feature = "browser")]
#[derive(Debug, Clone, PartialEq, Eq)]
struct LuaBrowserLaunchOptions {
    headless: bool,
    executable_path: Option<String>,
    no_sandbox: bool,
    args: Vec<String>,
}

#[cfg(feature = "browser")]
impl Default for LuaBrowserLaunchOptions {
    fn default() -> Self {
        Self {
            headless: true,
            executable_path: None,
            no_sandbox: false,
            args: Vec::new(),
        }
    }
}

#[cfg(feature = "browser")]
impl From<LuaBrowserLaunchOptions> for BrowserLaunchOptions {
    fn from(value: LuaBrowserLaunchOptions) -> Self {
        Self {
            headless: value.headless,
            executable_path: value.executable_path,
            no_sandbox: value.no_sandbox,
            args: value.args,
        }
    }
}

pub fn build_browser_table(lua: &Lua, mode: BrowserMode) -> Result<Table, LuaPluginError> {
    match mode {
        BrowserMode::Disabled => build_disabled_table(lua),
        #[cfg(not(feature = "browser"))]
        BrowserMode::Unsupported => build_unsupported_table(lua),
        #[cfg(feature = "browser")]
        BrowserMode::Enabled => build_enabled_table(lua),
    }
}

fn build_disabled_table(lua: &Lua) -> Result<Table, LuaPluginError> {
    let browser = lua.create_table()?;
    #[cfg(feature = "async")]
    {
        browser.set(
            "launch",
            lua.create_async_function(|_, _: mlua::Variadic<Value>| async move {
                Err::<Value, _>(mlua::Error::external(LuaPluginError::BrowserDisabled))
            })?,
        )?;
        browser.set(
            "connect",
            lua.create_async_function(|_, _: mlua::Variadic<Value>| async move {
                Err::<Value, _>(mlua::Error::external(LuaPluginError::BrowserDisabled))
            })?,
        )?;
    }
    #[cfg(not(feature = "async"))]
    {
        browser.set(
            "launch",
            lua.create_function(|_, _: mlua::Variadic<Value>| -> mlua::Result<Value> {
                Err(mlua::Error::external(LuaPluginError::BrowserDisabled))
            })?,
        )?;
        browser.set(
            "connect",
            lua.create_function(|_, _: mlua::Variadic<Value>| -> mlua::Result<Value> {
                Err(mlua::Error::external(LuaPluginError::BrowserDisabled))
            })?,
        )?;
    }
    Ok(browser)
}

#[cfg(not(feature = "browser"))]
fn build_unsupported_table(lua: &Lua) -> Result<Table, LuaPluginError> {
    let browser = lua.create_table()?;
    #[cfg(feature = "async")]
    {
        browser.set(
            "launch",
            lua.create_async_function(|_, _: mlua::Variadic<Value>| async move {
                Err::<Value, _>(mlua::Error::external(
                    LuaPluginError::BrowserFeatureDisabled,
                ))
            })?,
        )?;
        browser.set(
            "connect",
            lua.create_async_function(|_, _: mlua::Variadic<Value>| async move {
                Err::<Value, _>(mlua::Error::external(
                    LuaPluginError::BrowserFeatureDisabled,
                ))
            })?,
        )?;
    }
    #[cfg(not(feature = "async"))]
    {
        browser.set(
            "launch",
            lua.create_function(|_, _: mlua::Variadic<Value>| -> mlua::Result<Value> {
                Err(mlua::Error::external(
                    LuaPluginError::BrowserFeatureDisabled,
                ))
            })?,
        )?;
        browser.set(
            "connect",
            lua.create_function(|_, _: mlua::Variadic<Value>| -> mlua::Result<Value> {
                Err(mlua::Error::external(
                    LuaPluginError::BrowserFeatureDisabled,
                ))
            })?,
        )?;
    }
    Ok(browser)
}

#[cfg(feature = "browser")]
fn build_enabled_table(lua: &Lua) -> Result<Table, LuaPluginError> {
    let browser = lua.create_table()?;
    browser.set(
        "launch",
        lua.create_async_function(|_, options: Option<Value>| async move {
            let options = parse_launch_options(options).map_err(mlua::Error::external)?;
            let session = BrowserSession::launch(options.into())
                .await
                .map_err(mlua::Error::external)?;
            Ok(LuaBrowserSession(session))
        })?,
    )?;
    browser.set(
        "connect",
        lua.create_async_function(|_, endpoint: String| async move {
            let session = BrowserSession::connect(&endpoint)
                .await
                .map_err(mlua::Error::external)?;
            Ok(LuaBrowserSession(session))
        })?,
    )?;
    Ok(browser)
}

#[cfg(feature = "browser")]
#[derive(Clone)]
struct LuaBrowserSession(BrowserSession);

#[cfg(feature = "browser")]
#[derive(Clone)]
struct LuaBrowserPage(BrowserPage);

#[cfg(feature = "browser")]
impl UserData for LuaBrowserSession {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("__tostring", |_, _, ()| {
            Ok("weevil.browser.session".to_string())
        });
        methods.add_async_method("websocket_address", |_, this, ()| async move {
            Ok(this.0.websocket_address().await)
        });
        methods.add_async_method("new_page", |_, this, url: Option<String>| async move {
            let page = this
                .0
                .new_page(url.as_deref())
                .await
                .map_err(mlua::Error::external)?;
            Ok(LuaBrowserPage(page))
        });
        methods.add_async_method("get_cookies", |lua, this, ()| async move {
            let cookies = this.0.get_cookies().await.map_err(mlua::Error::external)?;
            cookies_to_lua(&lua, cookies)
        });
        methods.add_async_method("set_cookies", |_, this, value: Value| async move {
            let cookies = parse_cookie_inputs(value).map_err(mlua::Error::external)?;
            this.0
                .set_cookies(cookies)
                .await
                .map_err(mlua::Error::external)?;
            Ok(())
        });
        methods.add_async_method("clear_cookies", |_, this, ()| async move {
            this.0
                .clear_cookies()
                .await
                .map_err(mlua::Error::external)?;
            Ok(())
        });
        methods.add_async_method("close", |_, this, ()| async move {
            this.0.close().await.map_err(mlua::Error::external)?;
            Ok(())
        });
    }
}

#[cfg(feature = "browser")]
impl UserData for LuaBrowserPage {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("__tostring", |_, _, ()| {
            Ok("weevil.browser.page".to_string())
        });
        methods.add_async_method("goto", |_, this, url: String| async move {
            this.0.goto(&url).await.map_err(mlua::Error::external)?;
            Ok(())
        });
        methods.add_async_method("content", |_, this, ()| async move {
            this.0.content().await.map_err(mlua::Error::external)
        });
        methods.add_async_method("url", |_, this, ()| async move {
            this.0.url().await.map_err(mlua::Error::external)
        });
        methods.add_async_method("title", |_, this, ()| async move {
            this.0.title().await.map_err(mlua::Error::external)
        });
        methods.add_async_method("evaluate", |lua, this, expression: String| async move {
            let value = this
                .0
                .evaluate(&expression)
                .await
                .map_err(mlua::Error::external)?;
            json_to_lua(&lua, value)
        });
        methods.add_async_method("get_cookies", |lua, this, ()| async move {
            let cookies = this.0.get_cookies().await.map_err(mlua::Error::external)?;
            cookies_to_lua(&lua, cookies)
        });
        methods.add_async_method("set_cookies", |_, this, value: Value| async move {
            let cookies = parse_cookie_inputs(value).map_err(mlua::Error::external)?;
            this.0
                .set_cookies(cookies)
                .await
                .map_err(mlua::Error::external)?;
            Ok(())
        });
        methods.add_async_method("click", |_, this, selector: String| async move {
            this.0
                .click(&selector)
                .await
                .map_err(mlua::Error::external)?;
            Ok(())
        });
        methods.add_async_method(
            "type",
            |_, this, (selector, text): (String, String)| async move {
                this.0
                    .type_text(&selector, &text)
                    .await
                    .map_err(mlua::Error::external)?;
                Ok(())
            },
        );
        methods.add_async_method(
            "press",
            |_, this, (selector, key): (String, String)| async move {
                this.0
                    .press_key(&selector, &key)
                    .await
                    .map_err(mlua::Error::external)?;
                Ok(())
            },
        );
        methods.add_async_method("set_user_agent", |_, this, user_agent: String| async move {
            this.0
                .set_user_agent(&user_agent)
                .await
                .map_err(mlua::Error::external)?;
            Ok(())
        });
        methods.add_async_method("reload", |_, this, ()| async move {
            this.0.reload().await.map_err(mlua::Error::external)?;
            Ok(())
        });
        methods.add_async_method("wait_for_navigation", |_, this, ()| async move {
            this.0
                .wait_for_navigation()
                .await
                .map_err(mlua::Error::external)?;
            Ok(())
        });
        methods.add_async_method("close", |_, this, ()| async move {
            this.0.close().await.map_err(mlua::Error::external)?;
            Ok(())
        });
    }
}

#[cfg(feature = "browser")]
fn parse_launch_options(options: Option<Value>) -> Result<LuaBrowserLaunchOptions, LuaPluginError> {
    let Some(options) = options else {
        return Ok(LuaBrowserLaunchOptions::default());
    };
    match options {
        Value::Nil => Ok(LuaBrowserLaunchOptions::default()),
        Value::Table(table) => parse_launch_options_table(table),
        other => Err(LuaPluginError::BrowserOptionsNotTable {
            kind: value_kind(&other).to_string(),
        }),
    }
}

#[cfg(feature = "browser")]
fn parse_launch_options_table(table: Table) -> Result<LuaBrowserLaunchOptions, LuaPluginError> {
    let headless = parse_bool_field(&table, "headless")?.unwrap_or(true);
    let executable_path = parse_string_field(&table, "executable_path")?;
    let no_sandbox = parse_bool_field(&table, "no_sandbox")?.unwrap_or(false);
    let args = parse_args_field(&table)?;
    Ok(LuaBrowserLaunchOptions {
        headless,
        executable_path,
        no_sandbox,
        args,
    })
}

#[cfg(feature = "browser")]
fn parse_bool_field(table: &Table, field: &str) -> Result<Option<bool>, LuaPluginError> {
    let value: Value = table.get(field)?;
    match value {
        Value::Nil => Ok(None),
        Value::Boolean(flag) => Ok(Some(flag)),
        other => Err(LuaPluginError::BrowserOptionNotBoolean {
            name: field.to_string(),
            kind: value_kind(&other).to_string(),
        }),
    }
}

#[cfg(feature = "browser")]
fn parse_string_field(table: &Table, field: &str) -> Result<Option<String>, LuaPluginError> {
    let value: Value = table.get(field)?;
    match value {
        Value::Nil => Ok(None),
        Value::String(text) => Ok(Some(
            text.to_str()
                .map_err(|_| LuaPluginError::BrowserOptionNotUtf8 {
                    name: field.to_string(),
                })?
                .to_string(),
        )),
        other => Err(LuaPluginError::BrowserOptionNotString {
            name: field.to_string(),
            kind: value_kind(&other).to_string(),
        }),
    }
}

#[cfg(feature = "browser")]
fn parse_args_field(table: &Table) -> Result<Vec<String>, LuaPluginError> {
    let value: Value = table.get("args")?;
    match value {
        Value::Nil => Ok(Vec::new()),
        Value::Table(args_table) => parse_args_table(args_table),
        other => Err(LuaPluginError::BrowserOptionArgsNotArray {
            kind: value_kind(&other).to_string(),
        }),
    }
}

#[cfg(feature = "browser")]
fn parse_args_table(table: Table) -> Result<Vec<String>, LuaPluginError> {
    let mut pair_count = 0usize;
    for pair in table.pairs::<Value, Value>() {
        pair?;
        pair_count += 1;
    }
    if pair_count != table.raw_len() {
        return Err(LuaPluginError::BrowserOptionArgsNotArray {
            kind: "non-array-table".to_string(),
        });
    }

    let mut args = Vec::new();
    for (index, entry) in table.sequence_values::<Value>().enumerate() {
        let entry = entry?;
        let arg = match entry {
            Value::String(value) => value,
            other => {
                return Err(LuaPluginError::BrowserOptionArgNotString {
                    index: index + 1,
                    kind: value_kind(&other).to_string(),
                });
            }
        };
        let arg = arg
            .to_str()
            .map_err(|_| LuaPluginError::BrowserOptionArgNotUtf8 { index: index + 1 })?;
        args.push(arg.to_string());
    }
    Ok(args)
}

#[cfg(feature = "browser")]
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

#[cfg(all(test, feature = "browser"))]
#[path = "browser_tests.rs"]
mod tests;
