use std::fs;
use std::path::{Path, PathBuf};

use mlua::{LuaSerdeExt, Value};
use quick_xml::de::from_str;
use serde::Serialize;
use weevil_lua::{LuaPlugin, TrustedUrl};

use crate::errors::AppError;
use crate::nfo::Movie;
use crate::source_priority::SourcePriority;

mod merge;
mod node_mapping;
mod priority_merge;

pub(crate) use merge::{merge_movie, merge_movie_details, merge_movie_images};
pub(crate) use node_mapping::NodeValueMapper;
use priority_merge::{MergeSource, merge_sources_movie};

pub(crate) struct FileScriptOutput {
    pub(crate) movie: Movie,
    pub(crate) xml: String,
    pub(crate) trusted_urls: Vec<TrustedUrl>,
    pub(crate) merged_sources: bool,
}

pub(crate) fn run_name_scripts(
    task_id: &str,
    task_kind: &'static str,
    scripts: &[PathBuf],
    multi_source: bool,
    multi_source_max_sources: u32,
    source_priority: &SourcePriority,
    mapper: &NodeValueMapper,
    name: &str,
) -> Result<String, AppError> {
    let output = run_name_scripts_output(
        task_id,
        task_kind,
        scripts,
        multi_source,
        multi_source_max_sources,
        source_priority,
        mapper,
        name,
    )?;
    Ok(output.xml)
}

pub(crate) fn run_name_scripts_output(
    task_id: &str,
    task_kind: &'static str,
    scripts: &[PathBuf],
    multi_source: bool,
    multi_source_max_sources: u32,
    source_priority: &SourcePriority,
    mapper: &NodeValueMapper,
    name: &str,
) -> Result<FileScriptOutput, AppError> {
    let mut sources = Vec::new();
    let mut last_error = None;
    let success_limit = success_limit(
        multi_source,
        multi_source_max_sources,
        source_priority.is_configured(),
    );

    for script in scripts {
        match run_script(task_id, task_kind, script, ScriptCallArgs::Name { name }) {
            Ok(source) => {
                sources.push(source);
                if sources.len() >= success_limit {
                    break;
                }
            }
            Err(error) => {
                last_error = Some(error);
            }
        }
    }

    if sources.is_empty() {
        return Err(last_error.unwrap_or_else(no_scripts_configured_error));
    }

    let merged_sources =
        should_merge_sources(multi_source, source_priority.is_configured(), sources.len());
    let mut first = sources.remove(0);
    if merged_sources {
        let mut merge_sources = Vec::with_capacity(sources.len() + 1);
        merge_sources.push(MergeSource {
            alias: first.alias.clone(),
            movie: first.movie.clone(),
        });
        for source in &sources {
            merge_sources.push(MergeSource {
                alias: source.alias.clone(),
                movie: source.movie.clone(),
            });
        }
        first.movie = merge_sources_movie(&merge_sources, source_priority, multi_source);
    }
    for source in sources {
        merge_trusted_urls(&mut first.trusted_urls, source.trusted_urls);
    }

    mapper.apply_movie(&mut first.movie);

    let xml = if merged_sources || mapper.has_rules() {
        serialize_movie(&first.movie)?
    } else {
        first.xml
    };

    Ok(FileScriptOutput {
        movie: first.movie,
        xml,
        trusted_urls: first.trusted_urls,
        merged_sources,
    })
}

pub(crate) fn run_file_scripts(
    task_id: &str,
    task_kind: &'static str,
    scripts: &[PathBuf],
    multi_source: bool,
    multi_source_max_sources: u32,
    source_priority: &SourcePriority,
    mapper: &NodeValueMapper,
    input_name: &str,
    input_path: &str,
) -> Result<FileScriptOutput, AppError> {
    let mut sources = Vec::new();
    let mut last_error = None;
    let success_limit = success_limit(
        multi_source,
        multi_source_max_sources,
        source_priority.is_configured(),
    );

    for script in scripts {
        match run_script(
            task_id,
            task_kind,
            script,
            ScriptCallArgs::File {
                input_name,
                input_path,
            },
        ) {
            Ok(source) => {
                sources.push(source);
                if sources.len() >= success_limit {
                    break;
                }
            }
            Err(error) => {
                last_error = Some(error);
            }
        }
    }

    if sources.is_empty() {
        return Err(last_error.unwrap_or_else(no_scripts_configured_error));
    }

    let merged_sources =
        should_merge_sources(multi_source, source_priority.is_configured(), sources.len());
    let mut first = sources.remove(0);
    if merged_sources {
        let mut merge_sources = Vec::with_capacity(sources.len() + 1);
        merge_sources.push(MergeSource {
            alias: first.alias.clone(),
            movie: first.movie.clone(),
        });
        for source in &sources {
            merge_sources.push(MergeSource {
                alias: source.alias.clone(),
                movie: source.movie.clone(),
            });
        }
        first.movie = merge_sources_movie(&merge_sources, source_priority, multi_source);
    }
    for source in sources {
        merge_trusted_urls(&mut first.trusted_urls, source.trusted_urls);
    }

    mapper.apply_movie(&mut first.movie);

    let xml = if merged_sources || mapper.has_rules() {
        serialize_movie(&first.movie)?
    } else {
        first.xml
    };

    Ok(FileScriptOutput {
        movie: first.movie,
        xml,
        trusted_urls: first.trusted_urls,
        merged_sources,
    })
}

#[cfg(test)]
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
            let movie: Movie = lua.from_value(value).map_err(AppError::LuaValue)?;
            serialize_movie(&movie)
        }
        other => Err(AppError::ScriptReturnedUnexpected {
            kind: value_kind(&other).to_string(),
        }),
    }
}

pub(crate) fn success_limit(
    multi_source: bool,
    multi_source_max_sources: u32,
    source_priority_configured: bool,
) -> usize {
    if source_priority_configured {
        return usize::MAX;
    }
    if !multi_source {
        return 1;
    }
    if multi_source_max_sources == 0 {
        return usize::MAX;
    }
    usize::try_from(multi_source_max_sources).unwrap_or(usize::MAX)
}

fn should_merge_sources(
    multi_source: bool,
    source_priority_configured: bool,
    source_count: usize,
) -> bool {
    source_count > 1 && (multi_source || source_priority_configured)
}

pub(crate) fn serialize_movie(movie: &Movie) -> Result<String, AppError> {
    let mut buffer = String::new();
    let mut serializer = quick_xml::se::Serializer::new(&mut buffer);
    serializer.indent(' ', 2);
    movie
        .serialize(serializer)
        .map_err(AppError::SerializeNfo)?;
    Ok(buffer)
}

fn run_script(
    task_id: &str,
    task_kind: &'static str,
    script: &Path,
    args: ScriptCallArgs<'_>,
) -> Result<ScriptSource, AppError> {
    let plugin = LuaPlugin::from_file(script).map_err(AppError::LuaPlugin)?;
    let alias = plugin.alias().to_string();
    plugin.set_log_context(task_id.to_string(), task_kind);

    let value = match args {
        ScriptCallArgs::Name { name } => plugin.call((name,)).map_err(AppError::LuaPlugin)?,
        ScriptCallArgs::File {
            input_name,
            input_path,
        } => plugin
            .call((input_name, input_path))
            .map_err(AppError::LuaPlugin)?,
    };

    let (movie, xml) = decode_script_output(value, plugin.lua())?;

    Ok(ScriptSource {
        alias,
        movie,
        xml,
        trusted_urls: plugin.trusted_urls().to_vec(),
    })
}

fn decode_script_output(
    value: Option<Value>,
    lua: &mlua::Lua,
) -> Result<(Movie, String), AppError> {
    let value = value.ok_or(AppError::ScriptReturnedNil)?;
    match value {
        Value::Table(_) => {
            let movie: Movie = lua.from_value(value).map_err(AppError::LuaValue)?;
            let xml = serialize_movie(&movie)?;
            Ok((movie, xml))
        }
        Value::String(text) => {
            let xml = text
                .to_str()
                .map_err(|_| AppError::ScriptOutputInvalidUtf8)?
                .to_string();
            let movie: Movie = from_str(&xml).map_err(AppError::NfoParse)?;
            Ok((movie, xml))
        }
        other => Err(AppError::ScriptReturnedUnexpected {
            kind: value_kind(&other).to_string(),
        }),
    }
}

fn merge_trusted_urls(target: &mut Vec<TrustedUrl>, incoming: Vec<TrustedUrl>) {
    for url in incoming {
        if !target.contains(&url) {
            target.push(url);
        }
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

fn no_scripts_configured_error() -> AppError {
    AppError::FetchRuntime {
        reason: "no scripts configured".to_string(),
    }
}

pub(crate) fn load_node_value_mapper(path: Option<&Path>) -> Result<NodeValueMapper, AppError> {
    let Some(path) = path else {
        return Ok(NodeValueMapper::default());
    };

    let file = fs::File::open(path).map_err(|source| AppError::FetchRuntime {
        reason: format!("failed to read node mapping CSV {path:?}: {source}"),
    })?;

    NodeValueMapper::from_csv_file(file).map_err(|reason| AppError::FetchRuntime {
        reason: format!("failed to parse node mapping CSV {path:?}: {reason}"),
    })
}

#[derive(Clone, Copy)]
enum ScriptCallArgs<'a> {
    Name {
        name: &'a str,
    },
    File {
        input_name: &'a str,
        input_path: &'a str,
    },
}

struct ScriptSource {
    alias: String,
    movie: Movie,
    xml: String,
    trusted_urls: Vec<TrustedUrl>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn success_limit_defaults_to_one_without_multi_source() {
        assert_eq!(success_limit(false, 5, false), 1);
    }

    #[test]
    fn success_limit_zero_means_unlimited() {
        assert_eq!(success_limit(true, 0, false), usize::MAX);
    }

    #[test]
    fn success_limit_priority_configured_means_unlimited() {
        assert_eq!(success_limit(false, 1, true), usize::MAX);
    }

    #[test]
    fn should_merge_sources_when_priority_configured() {
        assert!(should_merge_sources(false, true, 2));
        assert!(!should_merge_sources(false, true, 1));
    }

    #[test]
    fn load_node_value_mapper_loads_csv_file() {
        let dir = tempdir().expect("temp dir");
        let csv_path = dir.path().join("node-map.csv");
        std::fs::write(
            &csv_path,
            "node,from,to\ngenre,剧情,Drama\ngenre,剧情,Drama2\n",
        )
        .expect("write csv");

        let mapper = load_node_value_mapper(Some(csv_path.as_path())).expect("mapper");
        assert!(mapper.has_rules());
        let mut movie = Movie {
            genre: vec!["剧情".to_string()],
            ..Movie::default()
        };
        mapper.apply_movie(&mut movie);
        assert_eq!(movie.genre, vec!["Drama2".to_string()]);
    }

    #[test]
    fn load_node_value_mapper_returns_empty_when_missing_path() {
        let mapper = load_node_value_mapper(None).expect("mapper");
        assert!(!mapper.has_rules());
    }

    #[test]
    fn load_node_value_mapper_rejects_invalid_csv() {
        let dir = tempdir().expect("temp dir");
        let csv_path = dir.path().join("node-map.csv");
        std::fs::write(&csv_path, "genre,only-two\n").expect("write csv");

        let error = load_node_value_mapper(Some(csv_path.as_path())).expect_err("invalid");
        assert!(matches!(error, AppError::FetchRuntime { .. }));
    }
}
