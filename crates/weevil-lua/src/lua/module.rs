use std::sync::Arc;

use mlua::{Lua, Table};
use weevil_core::{HtmlTree, Selector, XPath};

use crate::error::LuaPluginError;
use crate::http::HttpClient;
use crate::lua::types::{LuaHtmlTree, LuaSelector, LuaXPath};

#[derive(Clone)]
pub enum HttpMode {
    Disabled,
    Enabled(Arc<HttpClient>),
}

pub fn install_module(lua: &Lua, http_mode: HttpMode) -> Result<(), LuaPluginError> {
    let weevil = lua.create_table()?;
    weevil.set("html", build_html_table(lua)?)?;
    weevil.set("selector", build_selector_table(lua)?)?;
    weevil.set("xpath", build_xpath_table(lua)?)?;
    weevil.set("http", build_http_table(lua, http_mode)?)?;
    lua.globals().set("weevil", weevil)?;
    Ok(())
}

pub fn set_http(lua: &Lua, http_mode: HttpMode) -> Result<(), LuaPluginError> {
    let globals = lua.globals();
    let weevil: Table = globals.get("weevil")?;
    let http = build_http_table(lua, http_mode)?;
    weevil.set("http", http)?;
    Ok(())
}

fn build_html_table(lua: &Lua) -> Result<Table, LuaPluginError> {
    let html = lua.create_table()?;
    html.set(
        "parse",
        lua.create_function(|_, input: String| Ok(LuaHtmlTree::new(HtmlTree::parse(&input))))?,
    )?;
    html.set(
        "parse_bytes",
        lua.create_function(|_, input: mlua::String| {
            Ok(LuaHtmlTree::new(HtmlTree::parse_bytes(
                input.as_bytes().as_ref(),
            )))
        })?,
    )?;
    html.set(
        "parse_checked",
        lua.create_function(|_, input: String| {
            HtmlTree::parse_checked(&input)
                .map(LuaHtmlTree::new)
                .map_err(mlua::Error::external)
        })?,
    )?;
    html.set(
        "parse_bytes_checked",
        lua.create_function(|_, input: mlua::String| {
            HtmlTree::parse_bytes_checked(input.as_bytes().as_ref())
                .map(LuaHtmlTree::new)
                .map_err(mlua::Error::external)
        })?,
    )?;
    html.set(
        "parse_with_errors",
        lua.create_function(|lua, input: String| {
            let output = HtmlTree::parse_with_errors(&input);
            let table = lua.create_table()?;
            table.set("tree", LuaHtmlTree::new(output.tree))?;
            let errors = output
                .errors
                .into_iter()
                .map(|issue| issue.message().to_string())
                .collect::<Vec<_>>();
            table.set("errors", errors)?;
            Ok(table)
        })?,
    )?;
    html.set(
        "parse_bytes_with_errors",
        lua.create_function(|lua, input: mlua::String| {
            let output = HtmlTree::parse_bytes_with_errors(input.as_bytes().as_ref());
            let table = lua.create_table()?;
            table.set("tree", LuaHtmlTree::new(output.tree))?;
            let errors = output
                .errors
                .into_iter()
                .map(|issue| issue.message().to_string())
                .collect::<Vec<_>>();
            table.set("errors", errors)?;
            Ok(table)
        })?,
    )?;
    Ok(html)
}

fn build_selector_table(lua: &Lua) -> Result<Table, LuaPluginError> {
    let selector = lua.create_table()?;
    selector.set(
        "parse",
        lua.create_function(|_, input: String| {
            Selector::parse(&input)
                .map(LuaSelector)
                .map_err(mlua::Error::external)
        })?,
    )?;
    Ok(selector)
}

fn build_xpath_table(lua: &Lua) -> Result<Table, LuaPluginError> {
    let xpath = lua.create_table()?;
    xpath.set(
        "parse",
        lua.create_function(|_, input: String| {
            XPath::parse(&input)
                .map(LuaXPath)
                .map_err(mlua::Error::external)
        })?,
    )?;
    Ok(xpath)
}

fn build_http_table(lua: &Lua, http_mode: HttpMode) -> Result<Table, LuaPluginError> {
    let http = lua.create_table()?;
    match http_mode {
        HttpMode::Disabled => {
            http.set(
                "get",
                lua.create_function(|_, _: String| -> mlua::Result<String> {
                    Err(mlua::Error::external(LuaPluginError::HttpDisabled))
                })?,
            )?;
            #[cfg(feature = "async")]
            http.set(
                "get_async",
                lua.create_async_function(|_, _: String| async move {
                    Err::<String, _>(mlua::Error::external(LuaPluginError::HttpDisabled))
                })?,
            )?;
        }
        HttpMode::Enabled(client) => {
            let blocking = client.clone();
            http.set(
                "get",
                lua.create_function(move |_, url: String| {
                    blocking.get_blocking(&url).map_err(mlua::Error::external)
                })?,
            )?;
            #[cfg(feature = "async")]
            {
                let async_client = client.clone();
                http.set(
                    "get_async",
                    lua.create_async_function(move |_, url: String| {
                        let async_client = async_client.clone();
                        async move {
                            async_client
                                .get_async(&url)
                                .await
                                .map_err(mlua::Error::external)
                        }
                    })?,
                )?;
            }
        }
    }
    Ok(http)
}
