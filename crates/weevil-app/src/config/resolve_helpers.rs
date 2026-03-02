use std::path::PathBuf;

use super::script_paths::dedupe_paths;
use super::value_types::StringPathList;
use super::{ModeConfig, SharedConfig};

pub(super) fn resolve_node_mapping_csv(
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

pub(super) fn resolve_max_depth(mode: &ModeConfig, shared: &SharedConfig, cli: Option<i32>) -> i32 {
    cli.or(mode.max_depth).or(shared.max_depth).unwrap_or(-1)
}

fn non_empty_path_list(list: &StringPathList) -> Option<Vec<PathBuf>> {
    let paths = list.to_vec();
    if paths.is_empty() { None } else { Some(paths) }
}
