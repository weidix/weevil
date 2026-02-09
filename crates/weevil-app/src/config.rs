use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::cli::FolderMultiStrategy;
use crate::errors::AppError;

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
    pub(crate) script: Option<PathBuf>,
    pub(crate) output: Option<PathBuf>,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct ModeCliOverrides {
    pub(crate) script: Option<PathBuf>,
    pub(crate) output: Option<String>,
    pub(crate) input_name_rules: Vec<String>,
    pub(crate) folder_multi: Option<FolderMultiStrategy>,
    pub(crate) fetch_threads: Option<u32>,
    pub(crate) throttle_same_script: Option<bool>,
    pub(crate) script_throttle_base_ms: Option<u64>,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct DirCliOverrides {
    pub(crate) input: Option<PathBuf>,
    pub(crate) mode: ModeCliOverrides,
    pub(crate) max_depth: Option<i32>,
}

impl AppConfig {
    pub(crate) fn load(path: Option<&Path>) -> Result<Self, AppError> {
        let Some(path) = resolve_path(path) else {
            return Ok(Self::default());
        };

        let content = fs::read_to_string(&path).map_err(|source| AppError::ConfigRead {
            path: path.clone(),
            source,
        })?;

        toml::from_str(&content).map_err(|source| AppError::ConfigParse { path, source })
    }

    pub(crate) fn resolve_name_with(
        &self,
        cli: &NameCliOverrides,
    ) -> Result<ResolvedNameConfig, AppError> {
        let script = self
            .resolve_name_script(cli)
            .ok_or(AppError::ConfigMissingField {
                mode: "name",
                field: "script",
            })?;

        let output = self
            .resolve_name_output(cli)
            .ok_or(AppError::ConfigMissingField {
                mode: "name",
                field: "output",
            })?;

        Ok(ResolvedNameConfig { script, output })
    }

    pub(crate) fn resolve_file_mode_with(
        &self,
        cli: &ModeCliOverrides,
    ) -> Result<ResolvedModeConfig, AppError> {
        resolve_mode_config("file", &self.file, &self.shared, cli)
    }

    pub(crate) fn resolve_dir_mode_with(
        &self,
        cli: &DirCliOverrides,
    ) -> Result<ResolvedDirConfig, AppError> {
        let mode = resolve_mode_config("dir", &self.dir.mode, &self.shared, &cli.mode)?;
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

    pub(crate) fn resolve_watch_mode_with(
        &self,
        cli: &DirCliOverrides,
    ) -> Result<ResolvedDirConfig, AppError> {
        let mode = resolve_mode_config("watch", &self.watch.mode, &self.shared, &cli.mode)?;
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

    fn resolve_name_script(&self, cli: &NameCliOverrides) -> Option<PathBuf> {
        cli.script
            .clone()
            .or_else(|| self.name.script.clone())
            .or_else(|| self.shared.script.clone())
    }

    fn resolve_name_output(&self, cli: &NameCliOverrides) -> Option<PathBuf> {
        cli.output
            .clone()
            .or_else(|| self.name.output.clone())
            .or_else(|| self.shared.output.clone().map(PathBuf::from))
    }
}

#[derive(Debug, Clone)]
pub(crate) struct ResolvedNameConfig {
    pub(crate) script: PathBuf,
    pub(crate) output: PathBuf,
}

#[derive(Debug, Clone)]
pub(crate) struct ResolvedModeConfig {
    pub(crate) script: PathBuf,
    pub(crate) output: String,
    pub(crate) input_name_rules: Vec<String>,
    pub(crate) folder_multi: FolderMultiStrategy,
    pub(crate) fetch_threads: u32,
    pub(crate) throttle_same_script: bool,
    pub(crate) script_throttle_base_ms: u64,
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
    output: Option<String>,
    input_name_rule: Option<StringList>,
    folder_multi: Option<FolderMultiStrategy>,
    max_depth: Option<i32>,
    fetch_threads: Option<u32>,
    throttle_same_script: Option<bool>,
    script_throttle_base_ms: Option<u64>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "kebab-case", deny_unknown_fields)]
struct NameConfig {
    script: Option<PathBuf>,
    output: Option<PathBuf>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "kebab-case", deny_unknown_fields)]
struct ModeConfig {
    script: Option<PathBuf>,
    output: Option<String>,
    input_name_rule: Option<StringList>,
    folder_multi: Option<FolderMultiStrategy>,
    max_depth: Option<i32>,
    fetch_threads: Option<u32>,
    throttle_same_script: Option<bool>,
    script_throttle_base_ms: Option<u64>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "kebab-case", deny_unknown_fields)]
struct DirConfig {
    input: Option<PathBuf>,
    #[serde(flatten)]
    mode: ModeConfig,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
enum StringList {
    One(String),
    Many(Vec<String>),
}

impl StringList {
    fn to_vec(&self) -> Vec<String> {
        match self {
            StringList::One(value) => vec![value.clone()],
            StringList::Many(values) => values.clone(),
        }
    }
}

fn resolve_path(path: Option<&Path>) -> Option<PathBuf> {
    if let Some(path) = path {
        return Some(path.to_path_buf());
    }

    let default_path = PathBuf::from(DEFAULT_CONFIG_PATH);
    if default_path.exists() {
        return Some(default_path);
    }

    None
}

fn resolve_mode_config(
    mode: &'static str,
    mode_config: &ModeConfig,
    shared: &SharedConfig,
    cli: &ModeCliOverrides,
) -> Result<ResolvedModeConfig, AppError> {
    let script = cli
        .script
        .clone()
        .or_else(|| mode_config.script.clone())
        .or_else(|| shared.script.clone())
        .ok_or(AppError::ConfigMissingField {
            mode,
            field: "script",
        })?;

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

    Ok(ResolvedModeConfig {
        script,
        output,
        input_name_rules,
        folder_multi,
        fetch_threads,
        throttle_same_script,
        script_throttle_base_ms,
    })
}

fn resolve_max_depth(mode: &ModeConfig, shared: &SharedConfig, cli: Option<i32>) -> i32 {
    cli.or(mode.max_depth).or(shared.max_depth).unwrap_or(-1)
}

#[cfg(test)]
#[path = "tests/config.rs"]
mod tests;
