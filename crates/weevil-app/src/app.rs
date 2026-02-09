use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use clap::Parser;

use crate::cli::{Cli, Command, FolderMultiStrategy};
use crate::config::{
    AppConfig, DirCliOverrides, ModeCliOverrides, NameCliOverrides, ResolvedModeConfig,
};
use crate::dir_mode;
use crate::errors::AppError;
use crate::file_mode;
use crate::image_store::localize_movie_images;
use crate::mode_params::{FetchModeParams, FileModeParams, MultiFolderStrategy};
use crate::source_runner;
use crate::watch_mode;

pub(crate) fn run() -> Result<(), AppError> {
    let cli = Cli::try_parse().map_err(AppError::Cli)?;
    let config = AppConfig::load(cli.config.as_deref())?;
    match cli.command {
        Command::Name {
            name,
            scripts,
            output,
            multi_source,
            save_images,
            multi_source_max_sources,
        } => {
            let resolved = config.resolve_name_with(&NameCliOverrides {
                scripts,
                output,
                multi_source: flag_override(multi_source),
                save_images: flag_override(save_images),
                multi_source_max_sources,
            })?;
            run_lua_nfo(
                &name,
                &resolved.scripts,
                resolved.multi_source,
                resolved.save_images,
                resolved.multi_source_max_sources,
                &resolved.output,
            )
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
            let resolved = config.resolve_file_mode_with(&ModeCliOverrides {
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
            })?;
            let params = file_mode_params_from_config(resolved);
            file_mode::run_file_mode(&input, &params)
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
            let resolved = config.resolve_dir_mode_with(&DirCliOverrides {
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
                },
                max_depth,
            })?;
            let mode = resolved.mode;
            let params = file_mode_params_from_config(mode.clone());
            let fetch = fetch_mode_params_from_config(mode);
            dir_mode::run_dir_mode(&resolved.input, &params, &fetch, resolved.max_depth)
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
            let resolved = config.resolve_watch_mode_with(&DirCliOverrides {
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
                },
                max_depth,
            })?;
            let mode = resolved.mode;
            let params = file_mode_params_from_config(mode.clone());
            let fetch = fetch_mode_params_from_config(mode);
            watch_mode::run_watch_mode(&resolved.input, &params, &fetch, resolved.max_depth)
        }
    }
}

fn file_mode_params_from_config(resolved: ResolvedModeConfig) -> FileModeParams {
    FileModeParams::new(
        resolved.scripts,
        resolved.output,
        resolved.input_name_rules,
        map_folder_multi(resolved.folder_multi),
        resolved.multi_source,
        resolved.save_images,
        resolved.multi_source_max_sources,
    )
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
        };
        let fetch = fetch_mode_params_from_config(resolved);
        assert_eq!(fetch.fetch_threads(), 0);
        assert!(fetch.throttle_same_script());
        assert_eq!(fetch.script_throttle_base_ms(), 1400);
        assert!(fetch.multithread_enabled());
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

fn run_lua_nfo(
    name: &str,
    scripts: &[std::path::PathBuf],
    multi_source: bool,
    save_images: bool,
    multi_source_max_sources: u32,
    output: &Path,
) -> Result<(), AppError> {
    let task = TaskContext::new("name");
    let xml = if save_images {
        let mut source_output = source_runner::run_name_scripts_output(
            &task.id,
            task.kind,
            scripts,
            multi_source,
            multi_source_max_sources,
            name,
        )?;
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
        )?;
        source_runner::serialize_movie(&source_output.movie)?
    } else {
        source_runner::run_name_scripts(
            &task.id,
            task.kind,
            scripts,
            multi_source,
            multi_source_max_sources,
            name,
        )?
    };

    std::fs::write(output, xml).map_err(|err| AppError::OutputWrite {
        path: output.to_path_buf(),
        source: err,
    })?;
    Ok(())
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
