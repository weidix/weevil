use std::collections::HashSet;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use clap::Parser;
use tracing::warn;

use crate::cli::{Cli, Command, FolderMultiStrategy};
use crate::config::{
    AppConfig, DirCliOverrides, ModeCliOverrides, NameCliOverrides, ResolvedModeConfig,
    ResolvedNameConfig,
};
use crate::dir_mode;
use crate::errors::AppError;
use crate::file_mode;
use crate::image_store::localize_movie_images;
use crate::mode_params::{FetchModeParams, FileModeParams, MultiFolderStrategy};
use crate::script_info;
use crate::script_throttle::ScriptThrottleConfig;
use crate::source_priority::SourcePriority;
use crate::source_runner;
use crate::translation::MovieTranslator;
use crate::watch_mode;

pub(crate) async fn run() -> Result<(), AppError> {
    let cli = Cli::try_parse().map_err(AppError::Cli)?;
    let config = AppConfig::load(cli.config.as_deref()).await?;
    let cli_node_mapping_csv = cli.node_mapping_csv.clone();
    match cli.command {
        Command::Name {
            name,
            scripts,
            output,
            multi_source,
            save_images,
            multi_source_max_sources,
        } => {
            let resolved = config
                .resolve_name_with(&NameCliOverrides {
                    scripts,
                    output,
                    multi_source: flag_override(multi_source),
                    save_images: flag_override(save_images),
                    multi_source_max_sources,
                    node_mapping_csv: cli_node_mapping_csv.clone(),
                })
                .await?;
            let resolved = dedupe_resolved_name_script_aliases(resolved).await?;
            run_lua_nfo(
                &name,
                &resolved.scripts,
                resolved.multi_source,
                resolved.save_images,
                resolved.multi_source_max_sources,
                &resolved.source_priority,
                &resolved.node_mapping_csv,
                &resolved.translation,
                &resolved.output,
            )
            .await
        }
        Command::File {
            input,
            scripts,
            output,
            input_name_rules,
            folder_multi,
            multi_source,
            save_images,
            multi_source_max_sources,
        } => {
            if input.is_empty() {
                return Err(AppError::FetchRuntime {
                    reason: "file mode requires at least one --input".to_string(),
                });
            }
            let resolved = config
                .resolve_file_mode_with(&ModeCliOverrides {
                    scripts,
                    output,
                    input_name_rules,
                    folder_multi,
                    fetch_threads: None,
                    throttle_same_script: None,
                    script_throttle_base_ms: None,
                    multi_source: flag_override(multi_source),
                    save_images: flag_override(save_images),
                    multi_source_max_sources,
                    node_mapping_csv: cli_node_mapping_csv.clone(),
                })
                .await?;
            let resolved = dedupe_resolved_script_aliases(resolved).await?;
            let params = file_mode_params_from_config(resolved).await?;
            file_mode::run_file_mode_inputs(&input, &params, ScriptThrottleConfig::disabled()).await
        }
        Command::Dir {
            input,
            scripts,
            output,
            input_name_rules,
            folder_multi,
            max_depth,
            fetch_threads,
            throttle_same_script,
            script_throttle_base_ms,
            multi_source,
            save_images,
            multi_source_max_sources,
        } => {
            let resolved = config
                .resolve_dir_mode_with(&DirCliOverrides {
                    input,
                    mode: ModeCliOverrides {
                        scripts,
                        output,
                        input_name_rules,
                        folder_multi,
                        fetch_threads,
                        throttle_same_script: flag_override(throttle_same_script),
                        script_throttle_base_ms,
                        multi_source: flag_override(multi_source),
                        save_images: flag_override(save_images),
                        multi_source_max_sources,
                        node_mapping_csv: cli_node_mapping_csv.clone(),
                    },
                    max_depth,
                })
                .await?;
            let input = resolved.input;
            let max_depth = resolved.max_depth;
            let mode = dedupe_resolved_script_aliases(resolved.mode).await?;
            let params = file_mode_params_from_config(mode.clone()).await?;
            let fetch = fetch_mode_params_from_config(mode);
            dir_mode::run_dir_mode(&input, &params, &fetch, max_depth).await
        }
        Command::Watch {
            input,
            scripts,
            output,
            input_name_rules,
            folder_multi,
            max_depth,
            fetch_threads,
            throttle_same_script,
            script_throttle_base_ms,
            multi_source,
            save_images,
            multi_source_max_sources,
        } => {
            let resolved = config
                .resolve_watch_mode_with(&DirCliOverrides {
                    input,
                    mode: ModeCliOverrides {
                        scripts,
                        output,
                        input_name_rules,
                        folder_multi,
                        fetch_threads,
                        throttle_same_script: flag_override(throttle_same_script),
                        script_throttle_base_ms,
                        multi_source: flag_override(multi_source),
                        save_images: flag_override(save_images),
                        multi_source_max_sources,
                        node_mapping_csv: cli_node_mapping_csv.clone(),
                    },
                    max_depth,
                })
                .await?;
            let input = resolved.input;
            let max_depth = resolved.max_depth;
            let mode = dedupe_resolved_script_aliases(resolved.mode).await?;
            let params = file_mode_params_from_config(mode.clone()).await?;
            let fetch = fetch_mode_params_from_config(mode);
            watch_mode::run_watch_mode(&input, &params, &fetch, max_depth).await
        }
        Command::Scripts { scripts } => {
            let infos = script_info::list_script_infos(&config, scripts).await?;
            script_info::print_script_infos(&infos);
            Ok(())
        }
    }
}

async fn dedupe_resolved_script_aliases(
    mut resolved: ResolvedModeConfig,
) -> Result<ResolvedModeConfig, AppError> {
    resolved.scripts = dedupe_script_aliases_with_warning(resolved.scripts).await?;
    Ok(resolved)
}

async fn dedupe_resolved_name_script_aliases(
    mut resolved: ResolvedNameConfig,
) -> Result<ResolvedNameConfig, AppError> {
    resolved.scripts = dedupe_script_aliases_with_warning(resolved.scripts).await?;
    Ok(resolved)
}

async fn dedupe_script_aliases_with_warning(
    scripts: Vec<std::path::PathBuf>,
) -> Result<Vec<std::path::PathBuf>, AppError> {
    let mut deduped = Vec::with_capacity(scripts.len());
    let mut seen_aliases = HashSet::new();

    for script in scripts {
        let alias = script_alias_from_path(&script).await?;
        if seen_aliases.insert(alias.clone()) {
            deduped.push(script);
            continue;
        }
        warn!(
            "duplicate script alias detected: {alias}; keeping earliest script and ignoring later one"
        );
    }

    Ok(deduped)
}

async fn file_mode_params_from_config(
    resolved: ResolvedModeConfig,
) -> Result<FileModeParams, AppError> {
    let mapper = source_runner::load_node_value_mapper(&resolved.node_mapping_csv).await?;
    let translator = MovieTranslator::new(&resolved.translation)?;

    Ok(FileModeParams::new(
        resolved.scripts,
        resolved.output,
        resolved.input_name_rules,
        map_folder_multi(resolved.folder_multi),
        resolved.multi_source,
        resolved.save_images,
        resolved.multi_source_max_sources,
        resolved.source_priority,
        mapper,
        translator,
    ))
}

fn fetch_mode_params_from_config(resolved: ResolvedModeConfig) -> FetchModeParams {
    FetchModeParams::new(
        resolved.fetch_threads,
        resolved.throttle_same_script,
        resolved.script_throttle_base_ms,
    )
}

#[cfg(test)]
mod config_mapping_tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn fetch_mode_mapping_from_resolved_config() {
        let resolved = ResolvedModeConfig {
            scripts: vec!["demo.lua".into()],
            output: "out/{title}".to_string(),
            input_name_rules: vec![],
            folder_multi: FolderMultiStrategy::First,
            fetch_threads: 0,
            throttle_same_script: true,
            script_throttle_base_ms: 1400,
            multi_source: true,
            save_images: true,
            multi_source_max_sources: 3,
            source_priority: crate::source_priority::SourcePriority::default(),
            node_mapping_csv: Vec::new(),
            translation: crate::translation::ResolvedTranslationConfig::default(),
        };
        let fetch = fetch_mode_params_from_config(resolved);
        assert_eq!(fetch.fetch_threads(), 0);
        assert!(fetch.throttle_same_script());
        assert_eq!(fetch.script_throttle_base_ms(), 1400);
        assert!(fetch.multithread_enabled());
    }

    #[tokio::test]
    async fn dedupe_resolved_script_aliases_keeps_earliest_alias() {
        let dir = tempdir().expect("temp dir");
        let first = dir.path().join("a.lua");
        let second = dir.path().join("b.lua");
        let third = dir.path().join("c.lua");

        tokio::fs::write(
            &first,
            r#"return { alias = "source.a", trusted_urls = {}, run = function() return nil end }"#,
        )
        .await
        .expect("write first");
        tokio::fs::write(
            &second,
            r#"return { alias = "source.b", trusted_urls = {}, run = function() return nil end }"#,
        )
        .await
        .expect("write second");
        tokio::fs::write(
            &third,
            r#"return { alias = "source.a", trusted_urls = {}, run = function() return nil end }"#,
        )
        .await
        .expect("write third");

        let resolved = ResolvedModeConfig {
            scripts: vec![first.clone(), second.clone(), third],
            output: "out/{title}".to_string(),
            input_name_rules: vec![],
            folder_multi: FolderMultiStrategy::First,
            fetch_threads: 1,
            throttle_same_script: false,
            script_throttle_base_ms: 1000,
            multi_source: false,
            save_images: false,
            multi_source_max_sources: 2,
            source_priority: crate::source_priority::SourcePriority::default(),
            node_mapping_csv: Vec::new(),
            translation: crate::translation::ResolvedTranslationConfig::default(),
        };

        let deduped = dedupe_resolved_script_aliases(resolved)
            .await
            .expect("dedupe scripts");
        assert_eq!(deduped.scripts, vec![first, second]);
    }

    #[tokio::test]
    async fn dedupe_resolved_name_script_aliases_keeps_earliest_alias() {
        let dir = tempdir().expect("temp dir");
        let first = dir.path().join("name-a.lua");
        let second = dir.path().join("name-b.lua");
        let third = dir.path().join("name-c.lua");

        tokio::fs::write(
            &first,
            r#"return { alias = "name.a", trusted_urls = {}, run = function() return nil end }"#,
        )
        .await
        .expect("write first");
        tokio::fs::write(
            &second,
            r#"return { alias = "name.b", trusted_urls = {}, run = function() return nil end }"#,
        )
        .await
        .expect("write second");
        tokio::fs::write(
            &third,
            r#"return { alias = "name.a", trusted_urls = {}, run = function() return nil end }"#,
        )
        .await
        .expect("write third");

        let resolved = ResolvedNameConfig {
            scripts: vec![first.clone(), second.clone(), third],
            output: dir.path().join("out.nfo"),
            multi_source: false,
            save_images: false,
            multi_source_max_sources: 2,
            source_priority: crate::source_priority::SourcePriority::default(),
            node_mapping_csv: Vec::new(),
            translation: crate::translation::ResolvedTranslationConfig::default(),
        };

        let deduped = dedupe_resolved_name_script_aliases(resolved)
            .await
            .expect("dedupe scripts");
        assert_eq!(deduped.scripts, vec![first, second]);
    }
}

fn flag_override(enabled: bool) -> Option<bool> {
    if enabled { Some(true) } else { None }
}

fn map_folder_multi(strategy: FolderMultiStrategy) -> MultiFolderStrategy {
    match strategy {
        FolderMultiStrategy::HardLink => MultiFolderStrategy::HardLink,
        FolderMultiStrategy::SoftLink => MultiFolderStrategy::SoftLink,
        FolderMultiStrategy::First => MultiFolderStrategy::First,
    }
}

async fn run_lua_nfo(
    name: &str,
    scripts: &[std::path::PathBuf],
    multi_source: bool,
    save_images: bool,
    multi_source_max_sources: u32,
    source_priority: &SourcePriority,
    node_mapping_csv: &[std::path::PathBuf],
    translation: &crate::translation::ResolvedTranslationConfig,
    output: &Path,
) -> Result<(), AppError> {
    let task = TaskContext::new("name");
    let mapper = source_runner::load_node_value_mapper(node_mapping_csv).await?;
    let translator = MovieTranslator::new(translation)?;
    let xml = if save_images {
        let mut source_output = source_runner::run_name_scripts_output(
            &task.id,
            task.kind,
            scripts,
            multi_source,
            multi_source_max_sources,
            source_priority,
            &mapper,
            &translator,
            name,
            ScriptThrottleConfig::disabled(),
        )
        .await?;
        let output_dir = output.parent().unwrap_or_else(|| Path::new("."));
        let file_base = output
            .file_stem()
            .and_then(|value| value.to_str())
            .ok_or_else(|| AppError::PathStemNotUtf8 {
                path: output.to_path_buf(),
            })?;
        localize_movie_images(
            &mut source_output.movie,
            output_dir,
            file_base,
            &source_output.trusted_urls,
        )
        .await?;
        source_runner::serialize_movie(&source_output.movie)?
    } else {
        source_runner::run_name_scripts(
            &task.id,
            task.kind,
            scripts,
            multi_source,
            multi_source_max_sources,
            source_priority,
            &mapper,
            &translator,
            name,
            ScriptThrottleConfig::disabled(),
        )
        .await?
    };

    tokio::fs::write(output, xml)
        .await
        .map_err(|err| AppError::OutputWrite {
            path: output.to_path_buf(),
            source: err,
        })?;
    Ok(())
}

async fn script_alias_from_path(path: &Path) -> Result<String, AppError> {
    let script = tokio::fs::read_to_string(path)
        .await
        .map_err(|err| AppError::FetchRuntime {
            reason: format!("failed to read script {path:?}: {err}"),
        })?;
    weevil_lua::script_alias(&script).map_err(AppError::LuaPlugin)
}

pub(crate) struct TaskContext {
    pub(crate) id: String,
    pub(crate) kind: &'static str,
}

impl TaskContext {
    pub(crate) fn new(kind: &'static str) -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default();
        let id = format!("{kind}-{}-{}", now.as_millis(), std::process::id());
        Self { id, kind }
    }
}

#[cfg(test)]
#[path = "tests/app.rs"]
mod tests;
