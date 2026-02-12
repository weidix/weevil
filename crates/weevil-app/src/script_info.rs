use std::collections::HashSet;
use std::fmt::Write;
use std::path::PathBuf;

use tracing::warn;

use crate::config::AppConfig;
use crate::errors::AppError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ScriptInfo {
    pub(crate) path: PathBuf,
    pub(crate) alias: String,
    pub(crate) trusted_urls: Vec<String>,
    pub(crate) has_run: bool,
    pub(crate) duplicate_alias_ignored: bool,
}

pub(crate) async fn list_script_infos(
    config: &AppConfig,
    cli_scripts: Vec<PathBuf>,
) -> Result<Vec<ScriptInfo>, AppError> {
    let script_paths = if cli_scripts.is_empty() {
        gather_scripts_from_config(config).await?
    } else {
        cli_scripts
    };

    let mut seen_aliases = HashSet::new();
    let mut infos = Vec::with_capacity(script_paths.len());

    for path in script_paths {
        let script =
            tokio::fs::read_to_string(&path)
                .await
                .map_err(|err| AppError::FetchRuntime {
                    reason: format!("failed to read script {path:?}: {err}"),
                })?;
        let spec = weevil_lua::LuaPlugin::check(&script).map_err(AppError::LuaPlugin)?;
        let duplicate_alias_ignored = !seen_aliases.insert(spec.alias().to_string());

        infos.push(ScriptInfo {
            path,
            alias: spec.alias().to_string(),
            trusted_urls: spec
                .trusted_urls()
                .iter()
                .map(|url| url.original().to_string())
                .collect(),
            has_run: spec.has_run(),
            duplicate_alias_ignored,
        });
    }

    Ok(infos)
}

async fn gather_scripts_from_config(config: &AppConfig) -> Result<Vec<PathBuf>, AppError> {
    let deduped = config.list_script_paths_for_info().await;

    if deduped.is_empty() {
        return Err(AppError::ScriptInfoNoScripts);
    }

    Ok(deduped)
}

pub(crate) fn print_script_infos(infos: &[ScriptInfo]) {
    for info in infos {
        if info.duplicate_alias_ignored {
            warn!(
                "duplicate script alias detected: {}; keeping earliest script and ignoring later one",
                info.alias
            );
        }
    }

    print!("{}", format_script_infos(infos));
}

fn format_script_infos(infos: &[ScriptInfo]) -> String {
    let total = infos.len();
    let ignored = infos
        .iter()
        .filter(|info| info.duplicate_alias_ignored)
        .count();
    let active = total.saturating_sub(ignored);

    let mut output = String::new();
    let _ = writeln!(output, "scripts_total: {total}");
    let _ = writeln!(output, "active: {active}");
    let _ = writeln!(output, "ignored_duplicate_alias: {ignored}");

    for (index, info) in infos.iter().enumerate() {
        let status = script_status(info);
        let script = info.path.display();
        let _ = writeln!(output);
        let _ = writeln!(output, "[{}/{}] {status}", index + 1, total);
        let _ = writeln!(output, "  script: {script}");
        let _ = writeln!(output, "  alias: {}", info.alias);
        if info.trusted_urls.is_empty() {
            let _ = writeln!(output, "  trusted_urls: []");
        } else {
            let _ = writeln!(output, "  trusted_urls:");
            for url in &info.trusted_urls {
                let _ = writeln!(output, "    - {url}");
            }
        }
        let _ = writeln!(output, "  has_run: {}", info.has_run);
    }

    output
}

fn script_status(info: &ScriptInfo) -> &'static str {
    if info.duplicate_alias_ignored {
        "ignored-duplicate-alias"
    } else {
        "active"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_uses_trusted_urls_list_key() {
        let infos = vec![ScriptInfo {
            path: PathBuf::from("demo.lua"),
            alias: "demo.alias".to_string(),
            trusted_urls: vec![
                "https://example.com/".to_string(),
                "https://example.org/path".to_string(),
            ],
            has_run: true,
            duplicate_alias_ignored: false,
        }];

        let output = format_script_infos(&infos);
        assert!(output.contains("trusted_urls:"));
        assert!(output.contains("- https://example.com/"));
        assert!(output.contains("- https://example.org/path"));
        assert!(output.contains("scripts_total: 1"));
        assert!(output.contains("[1/1] active"));
    }
}
