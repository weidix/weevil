use std::fmt;
use std::path::PathBuf;

#[derive(Debug)]
pub(crate) enum AppError {
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
    NfoParse(quick_xml::DeError),
    NotImplemented {
        mode: &'static str,
    },
    InputMetadata {
        path: PathBuf,
        source: std::io::Error,
    },
    InputNotFile {
        path: PathBuf,
    },
    PathNotUtf8 {
        path: PathBuf,
    },
    PathStemNotUtf8 {
        path: PathBuf,
    },
    OutputDirCreate {
        path: PathBuf,
        source: std::io::Error,
    },
    OutputPathExists {
        path: PathBuf,
    },
    SubtitleScan {
        path: PathBuf,
        source: std::io::Error,
    },
    FileLock {
        path: PathBuf,
        source: std::io::Error,
    },
    FileMove {
        from: PathBuf,
        to: PathBuf,
        source: std::io::Error,
    },
    FileCopy {
        from: PathBuf,
        to: PathBuf,
        source: std::io::Error,
    },
    FileRemove {
        path: PathBuf,
        source: std::io::Error,
    },
    TemplateInvalid {
        template: String,
        reason: String,
    },
    TemplateUnknownField {
        template: String,
        field: String,
    },
    TemplateEmpty {
        template: String,
    },
    TemplateEmptySegment {
        template: String,
    },
    TemplateAbsolutePath {
        template: String,
    },
    InputNameFormatEmpty {
        input: String,
        rules: Vec<String>,
    },
}

impl AppError {
    pub(crate) fn exit_code(&self) -> i32 {
        match self {
            AppError::Cli(error) => error.exit_code(),
            _ => 1,
        }
    }

    pub(crate) fn report(&self) {
        match self {
            AppError::Cli(error) => {
                if let Err(io_error) = error.print() {
                    eprintln!("failed to print CLI error: {io_error}");
                }
            }
            AppError::LuaPlugin(error) => {
                if let Some(message) = format_lua_plugin_error(error) {
                    eprintln!("{message}");
                } else {
                    eprintln!("{error}");
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
            AppError::NfoParse(error) => write!(f, "failed to parse NFO XML: {error}"),
            AppError::NotImplemented { mode } => {
                write!(f, "mode {mode} is reserved but not implemented yet")
            }
            AppError::InputMetadata { path, source } => {
                write!(f, "failed to read input metadata for {path:?}: {source}")
            }
            AppError::InputNotFile { path } => {
                write!(f, "input path {path:?} is not a file")
            }
            AppError::PathNotUtf8 { path } => {
                write!(f, "path {path:?} is not valid UTF-8")
            }
            AppError::PathStemNotUtf8 { path } => {
                write!(f, "file stem {path:?} is not valid UTF-8")
            }
            AppError::OutputDirCreate { path, source } => {
                write!(f, "failed to create output directory {path:?}: {source}")
            }
            AppError::OutputPathExists { path } => {
                write!(f, "output path already exists: {path:?}")
            }
            AppError::SubtitleScan { path, source } => {
                write!(f, "failed to scan subtitles in {path:?}: {source}")
            }
            AppError::FileLock { path, source } => {
                write!(f, "failed to lock file {path:?}: {source}")
            }
            AppError::FileMove { from, to, source } => {
                write!(f, "failed to move file from {from:?} to {to:?}: {source}")
            }
            AppError::FileCopy { from, to, source } => {
                write!(f, "failed to copy file from {from:?} to {to:?}: {source}")
            }
            AppError::FileRemove { path, source } => {
                write!(f, "failed to remove file {path:?}: {source}")
            }
            AppError::TemplateInvalid { template, reason } => {
                write!(f, "invalid template {template:?}: {reason}")
            }
            AppError::TemplateUnknownField { template, field } => {
                write!(f, "unknown template field {field:?} in {template:?}")
            }
            AppError::TemplateEmpty { template } => {
                write!(f, "template {template:?} resolved to empty output")
            }
            AppError::TemplateEmptySegment { template } => {
                write!(f, "template {template:?} resolved to an empty path segment")
            }
            AppError::TemplateAbsolutePath { template } => {
                write!(f, "template {template:?} resolved to an absolute path")
            }
            AppError::InputNameFormatEmpty { input, rules } => {
                write!(
                    f,
                    "input filename {input:?} resolved to empty after applying remove rules {rules:?}"
                )
            }
        }
    }
}

fn format_lua_plugin_error(error: &weevil_lua::LuaPluginError) -> Option<String> {
    match error {
        weevil_lua::LuaPluginError::Lua(lua_error) => Some(format_lua_error(lua_error)),
        weevil_lua::LuaPluginError::HttpStatus { url, status } => {
            Some(format_http_status_hint(*status, url))
        }
        _ => None,
    }
}

fn format_lua_error(error: &mlua::Error) -> String {
    match error {
        mlua::Error::CallbackError { cause, traceback } => {
            let mut message = format_lua_error(cause.as_ref());
            if let Some(frame) = extract_lua_frame(traceback) {
                if message.is_empty() {
                    message = format!("Lua error at {frame}");
                } else {
                    message = format!("{message}\nLua traceback (first frame): {frame}");
                }
            }
            message
        }
        mlua::Error::WithContext { context, cause } => {
            let inner = format_lua_error(cause.as_ref());
            if inner.is_empty() {
                context.to_string()
            } else {
                format!("{context}: {inner}")
            }
        }
        _ => {
            if let Some(plugin_error) = error.downcast_ref::<weevil_lua::LuaPluginError>() {
                if let Some(message) = format_lua_plugin_error(plugin_error) {
                    return message;
                }
                return plugin_error.to_string();
            }
            error.to_string()
        }
    }
}

fn extract_lua_frame(traceback: &str) -> Option<String> {
    for line in traceback.lines() {
        let trimmed = line.trim();
        if trimmed.contains(".lua:") {
            let end = trimmed.find(" in ").unwrap_or(trimmed.len());
            return Some(trimmed[..end].to_string());
        }
        if trimmed.starts_with("[string ") {
            let end = trimmed.find(" in ").unwrap_or(trimmed.len());
            return Some(trimmed[..end].to_string());
        }
    }
    None
}

fn format_http_status_hint(status: u16, url: &str) -> String {
    let base = format!("HTTP request returned status {status} for {url}.");
    match status {
        401 | 403 => format!(
            "{base} Hint: the site may require authentication or block non-browser clients. \
Try a mirror domain, a proxy, or confirm the URL is reachable in a browser."
        ),
        429 => format!("{base} Hint: rate limit detected. Slow down requests or retry later."),
        500..=599 => format!("{base} Hint: the server is unavailable or unstable. Retry later."),
        _ => base,
    }
}

#[cfg(test)]
#[path = "tests/errors.rs"]
mod tests;
