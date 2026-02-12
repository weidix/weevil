use std::path::{Path, PathBuf};

use serde::Deserialize;
use tokio::fs;

use crate::cli::FolderMultiStrategy;
use crate::errors::AppError;
use crate::source_priority::{SourcePriority, SourcePriorityConfig};

use self::script_paths::{dedupe_paths, expand_script_patterns};
use self::value_types::{StringList, StringPathList};

const DEFAULT_CONFIG_PATH: &str = "weevil.toml";

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "kebab-case", deny_unknown_fields)]
pub(crate) struct AppConfig {
    shared: SharedConfig,
    name: NameConfig,
    file: ModeConfig,
    dir: DirConfig,
    watch: DirConfig,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct NameCliOverrides {
    pub(crate) scripts: Vec<PathBuf>,
    pub(crate) output: Option<PathBuf>,
    pub(crate) multi_source: Option<bool>,
    pub(crate) save_images: Option<bool>,
    pub(crate) multi_source_max_sources: Option<u32>,
    pub(crate) node_mapping_csv: Vec<PathBuf>,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct ModeCliOverrides {
    pub(crate) scripts: Vec<PathBuf>,
    pub(crate) output: Option<String>,
    pub(crate) input_name_rules: Vec<String>,
    pub(crate) folder_multi: Option<FolderMultiStrategy>,
    pub(crate) fetch_threads: Option<u32>,
    pub(crate) throttle_same_script: Option<bool>,
    pub(crate) script_throttle_base_ms: Option<u64>,
    pub(crate) multi_source: Option<bool>,
    pub(crate) save_images: Option<bool>,
    pub(crate) multi_source_max_sources: Option<u32>,
    pub(crate) node_mapping_csv: Vec<PathBuf>,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct DirCliOverrides {
    pub(crate) input: Option<PathBuf>,
    pub(crate) mode: ModeCliOverrides,
    pub(crate) max_depth: Option<i32>,
}

#[derive(Debug, Clone)]
pub(crate) struct ResolvedNameConfig {
    pub(crate) scripts: Vec<PathBuf>,
    pub(crate) output: PathBuf,
    pub(crate) multi_source: bool,
    pub(crate) save_images: bool,
    pub(crate) multi_source_max_sources: u32,
    pub(crate) source_priority: SourcePriority,
    pub(crate) node_mapping_csv: Vec<PathBuf>,
}

#[derive(Debug, Clone)]
pub(crate) struct ResolvedModeConfig {
    pub(crate) scripts: Vec<PathBuf>,
    pub(crate) output: String,
    pub(crate) input_name_rules: Vec<String>,
    pub(crate) folder_multi: FolderMultiStrategy,
    pub(crate) fetch_threads: u32,
    pub(crate) throttle_same_script: bool,
    pub(crate) script_throttle_base_ms: u64,
    pub(crate) multi_source: bool,
    pub(crate) save_images: bool,
    pub(crate) multi_source_max_sources: u32,
    pub(crate) source_priority: SourcePriority,
    pub(crate) node_mapping_csv: Vec<PathBuf>,
}

#[derive(Debug, Clone)]
pub(crate) struct ResolvedDirConfig {
    pub(crate) input: PathBuf,
    pub(crate) mode: ResolvedModeConfig,
    pub(crate) max_depth: i32,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "kebab-case", deny_unknown_fields)]
struct SharedConfig {
    script: Option<PathBuf>,
    scripts: Option<StringPathList>,
    output: Option<String>,
    input_name_rule: Option<StringList>,
    folder_multi: Option<FolderMultiStrategy>,
    max_depth: Option<i32>,
    fetch_threads: Option<u32>,
    throttle_same_script: Option<bool>,
    script_throttle_base_ms: Option<u64>,
    multi_source: Option<bool>,
    save_images: Option<bool>,
    multi_source_max_sources: Option<u32>,
    source_priority: Option<SourcePriorityConfig>,
    node_mapping_csv: Option<StringPathList>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "kebab-case", deny_unknown_fields)]
struct NameConfig {
    script: Option<PathBuf>,
    scripts: Option<StringPathList>,
    output: Option<PathBuf>,
    multi_source: Option<bool>,
    save_images: Option<bool>,
    multi_source_max_sources: Option<u32>,
    source_priority: Option<SourcePriorityConfig>,
    node_mapping_csv: Option<StringPathList>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "kebab-case", deny_unknown_fields)]
struct ModeConfig {
    script: Option<PathBuf>,
    scripts: Option<StringPathList>,
    output: Option<String>,
    input_name_rule: Option<StringList>,
    folder_multi: Option<FolderMultiStrategy>,
    max_depth: Option<i32>,
    fetch_threads: Option<u32>,
    throttle_same_script: Option<bool>,
    script_throttle_base_ms: Option<u64>,
    multi_source: Option<bool>,
    save_images: Option<bool>,
    multi_source_max_sources: Option<u32>,
    source_priority: Option<SourcePriorityConfig>,
    node_mapping_csv: Option<StringPathList>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "kebab-case", deny_unknown_fields)]
struct DirConfig {
    input: Option<PathBuf>,
    #[serde(flatten)]
    mode: ModeConfig,
}

impl AppConfig {
    pub(crate) async fn load(path: Option<&Path>) -> Result<Self, AppError> {
        let Some(path) = resolve_path(path).await else {
            return Ok(Self::default());
        };

        let content = fs::read_to_string(&path)
            .await
            .map_err(|source| AppError::ConfigRead {
                path: path.clone(),
                source,
            })?;

        toml::from_str(&content).map_err(|source| AppError::ConfigParse { path, source })
    }

    pub(crate) async fn resolve_name_with(
        &self,
        cli: &NameCliOverrides,
    ) -> Result<ResolvedNameConfig, AppError> {
        let scripts = self.resolve_name_scripts(cli).await;
        if scripts.is_empty() {
            return Err(AppError::ConfigMissingField {
                mode: "name",
                field: "script",
            });
        }

        let output = self
            .resolve_name_output(cli)
            .ok_or(AppError::ConfigMissingField {
                mode: "name",
                field: "output",
            })?;

        let multi_source = cli
            .multi_source
            .or(self.name.multi_source)
            .or(self.shared.multi_source)
            .unwrap_or(false);

        let save_images = cli
            .save_images
            .or(self.name.save_images)
            .or(self.shared.save_images)
            .unwrap_or(false);

        let multi_source_max_sources = cli
            .multi_source_max_sources
            .or(self.name.multi_source_max_sources)
            .or(self.shared.multi_source_max_sources)
            .unwrap_or(2);

        let source_priority = SourcePriority::from_mode_and_shared(
            self.name.source_priority.as_ref(),
            self.shared.source_priority.as_ref(),
        );

        let node_mapping_csv = resolve_node_mapping_csv(
            &cli.node_mapping_csv,
            self.name.node_mapping_csv.as_ref(),
            self.shared.node_mapping_csv.as_ref(),
        );

        Ok(ResolvedNameConfig {
            scripts,
            output,
            multi_source,
            save_images,
            multi_source_max_sources,
            source_priority,
            node_mapping_csv,
        })
    }

    pub(crate) async fn resolve_file_mode_with(
        &self,
        cli: &ModeCliOverrides,
    ) -> Result<ResolvedModeConfig, AppError> {
        resolve_mode_config("file", &self.file, &self.shared, cli).await
    }

    pub(crate) async fn resolve_dir_mode_with(
        &self,
        cli: &DirCliOverrides,
    ) -> Result<ResolvedDirConfig, AppError> {
        let mode = resolve_mode_config("dir", &self.dir.mode, &self.shared, &cli.mode).await?;
        let input = cli.input.clone().or_else(|| self.dir.input.clone()).ok_or(
            AppError::ConfigMissingField {
                mode: "dir",
                field: "input",
            },
        )?;
        Ok(ResolvedDirConfig {
            input,
            mode,
            max_depth: resolve_max_depth(&self.dir.mode, &self.shared, cli.max_depth),
        })
    }

    pub(crate) async fn resolve_watch_mode_with(
        &self,
        cli: &DirCliOverrides,
    ) -> Result<ResolvedDirConfig, AppError> {
        let mode = resolve_mode_config("watch", &self.watch.mode, &self.shared, &cli.mode).await?;
        let input = cli
            .input
            .clone()
            .or_else(|| self.watch.input.clone())
            .ok_or(AppError::ConfigMissingField {
                mode: "watch",
                field: "input",
            })?;
        Ok(ResolvedDirConfig {
            input,
            mode,
            max_depth: resolve_max_depth(&self.watch.mode, &self.shared, cli.max_depth),
        })
    }

    fn resolve_name_output(&self, cli: &NameCliOverrides) -> Option<PathBuf> {
        cli.output
            .clone()
            .or_else(|| self.name.output.clone())
            .or_else(|| self.shared.output.clone().map(PathBuf::from))
    }

    async fn resolve_name_scripts(&self, cli: &NameCliOverrides) -> Vec<PathBuf> {
        let cli_scripts = dedupe_paths(cli.scripts.clone());
        if !cli_scripts.is_empty() {
            return cli_scripts;
        }

        if let Some(list) = self.name.scripts.as_ref().map(StringPathList::to_vec) {
            let scripts = expand_script_patterns(list).await;
            if !scripts.is_empty() {
                return scripts;
            }
        }

        if let Some(script) = self.name.script.clone() {
            return expand_script_patterns(vec![script]).await;
        }

        if let Some(list) = self.shared.scripts.as_ref().map(StringPathList::to_vec) {
            let scripts = expand_script_patterns(list).await;
            if !scripts.is_empty() {
                return scripts;
            }
        }

        expand_script_patterns(self.shared.script.clone().into_iter().collect()).await
    }
}

async fn resolve_path(path: Option<&Path>) -> Option<PathBuf> {
    if let Some(path) = path {
        return Some(path.to_path_buf());
    }

    let default_path = PathBuf::from(DEFAULT_CONFIG_PATH);
    if fs::try_exists(&default_path).await.ok()? {
        return Some(default_path);
    }

    None
}

async fn resolve_mode_config(
    mode: &'static str,
    mode_config: &ModeConfig,
    shared: &SharedConfig,
    cli: &ModeCliOverrides,
) -> Result<ResolvedModeConfig, AppError> {
    let scripts = resolve_mode_scripts(mode_config, shared, cli).await.ok_or(
        AppError::ConfigMissingField {
            mode,
            field: "script",
        },
    )?;

    let output = cli
        .output
        .clone()
        .or_else(|| mode_config.output.clone())
        .or_else(|| shared.output.clone())
        .ok_or(AppError::ConfigMissingField {
            mode,
            field: "output",
        })?;

    let input_name_rules = if cli.input_name_rules.is_empty() {
        mode_config
            .input_name_rule
            .as_ref()
            .map(StringList::to_vec)
            .or_else(|| shared.input_name_rule.as_ref().map(StringList::to_vec))
            .unwrap_or_default()
    } else {
        cli.input_name_rules.clone()
    };

    let folder_multi = cli
        .folder_multi
        .or(mode_config.folder_multi)
        .or(shared.folder_multi)
        .unwrap_or(FolderMultiStrategy::First);

    let fetch_threads = cli
        .fetch_threads
        .or(mode_config.fetch_threads)
        .or(shared.fetch_threads)
        .unwrap_or(1);

    let throttle_same_script = cli
        .throttle_same_script
        .or(mode_config.throttle_same_script)
        .or(shared.throttle_same_script)
        .unwrap_or(false);

    let script_throttle_base_ms = cli
        .script_throttle_base_ms
        .or(mode_config.script_throttle_base_ms)
        .or(shared.script_throttle_base_ms)
        .unwrap_or(1000);

    let multi_source = cli
        .multi_source
        .or(mode_config.multi_source)
        .or(shared.multi_source)
        .unwrap_or(false);

    let save_images = cli
        .save_images
        .or(mode_config.save_images)
        .or(shared.save_images)
        .unwrap_or(false);

    let multi_source_max_sources = cli
        .multi_source_max_sources
        .or(mode_config.multi_source_max_sources)
        .or(shared.multi_source_max_sources)
        .unwrap_or(2);

    let source_priority = SourcePriority::from_mode_and_shared(
        mode_config.source_priority.as_ref(),
        shared.source_priority.as_ref(),
    );

    let node_mapping_csv = resolve_node_mapping_csv(
        &cli.node_mapping_csv,
        mode_config.node_mapping_csv.as_ref(),
        shared.node_mapping_csv.as_ref(),
    );

    Ok(ResolvedModeConfig {
        scripts,
        output,
        input_name_rules,
        folder_multi,
        fetch_threads,
        throttle_same_script,
        script_throttle_base_ms,
        multi_source,
        save_images,
        multi_source_max_sources,
        source_priority,
        node_mapping_csv,
    })
}

async fn resolve_mode_scripts(
    mode_config: &ModeConfig,
    shared: &SharedConfig,
    cli: &ModeCliOverrides,
) -> Option<Vec<PathBuf>> {
    let cli_scripts = dedupe_paths(cli.scripts.clone());
    if !cli_scripts.is_empty() {
        return Some(cli_scripts);
    }

    if let Some(list) = mode_config.scripts.as_ref().map(StringPathList::to_vec) {
        let scripts = expand_script_patterns(list).await;
        if !scripts.is_empty() {
            return Some(scripts);
        }
    }

    if let Some(script) = mode_config.script.clone() {
        return Some(expand_script_patterns(vec![script]).await);
    }

    if let Some(list) = shared.scripts.as_ref().map(StringPathList::to_vec) {
        let scripts = expand_script_patterns(list).await;
        if !scripts.is_empty() {
            return Some(scripts);
        }
    }

    match shared.script.clone() {
        Some(script) => Some(expand_script_patterns(vec![script]).await),
        None => None,
    }
}

fn resolve_node_mapping_csv(
    cli: &[PathBuf],
    mode: Option<&StringPathList>,
    shared: Option<&StringPathList>,
) -> Vec<PathBuf> {
    if !cli.is_empty() {
        return dedupe_paths(cli.to_vec());
    }

    if let Some(paths) = mode.and_then(non_empty_path_list) {
        return dedupe_paths(paths);
    }

    if let Some(paths) = shared.and_then(non_empty_path_list) {
        return dedupe_paths(paths);
    }

    Vec::new()
}

fn non_empty_path_list(list: &StringPathList) -> Option<Vec<PathBuf>> {
    let paths = list.to_vec();
    if paths.is_empty() { None } else { Some(paths) }
}

fn resolve_max_depth(mode: &ModeConfig, shared: &SharedConfig, cli: Option<i32>) -> i32 {
    cli.or(mode.max_depth).or(shared.max_depth).unwrap_or(-1)
}

#[cfg(test)]
#[path = "tests/config_local.rs"]
mod local_tests;
#[cfg(test)]
#[path = "tests/config_source_priority.rs"]
mod source_priority_tests;
#[cfg(test)]
#[path = "tests/config.rs"]
mod tests;

mod script_listing;
mod script_paths;
mod value_types;
