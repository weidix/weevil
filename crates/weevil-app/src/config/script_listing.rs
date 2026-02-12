use std::path::PathBuf;

use super::{AppConfig, StringPathList, dedupe_paths, expand_script_patterns};

impl AppConfig {
    pub(crate) async fn list_script_paths_for_info(&self) -> Vec<PathBuf> {
        let mut scripts = Vec::new();

        append_script_sources(&mut scripts, &self.shared.script, &self.shared.scripts);
        append_script_sources(&mut scripts, &self.name.script, &self.name.scripts);
        append_script_sources(&mut scripts, &self.file.script, &self.file.scripts);
        append_script_sources(&mut scripts, &self.dir.mode.script, &self.dir.mode.scripts);
        append_script_sources(
            &mut scripts,
            &self.watch.mode.script,
            &self.watch.mode.scripts,
        );

        expand_script_patterns(dedupe_paths(scripts)).await
    }
}

fn append_script_sources(
    target: &mut Vec<PathBuf>,
    single: &Option<PathBuf>,
    many: &Option<StringPathList>,
) {
    if let Some(path) = single {
        target.push(path.clone());
    }

    if let Some(paths) = many {
        target.extend(paths.to_vec());
    }
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::*;

    #[tokio::test]
    async fn list_script_paths_for_info_collects_all_sections() {
        let config: AppConfig = toml::from_str(
            r#"
[shared]
script = "scripts/shared-one.lua"
scripts = ["scripts/shared-two.lua", "scripts/dup.lua"]

[name]
script = "scripts/name.lua"

[file]
scripts = ["scripts/file.lua", "scripts/dup.lua"]

[dir]
input = "videos"
script = "scripts/dir.lua"

[watch]
input = "incoming"
scripts = ["scripts/watch.lua"]
"#,
        )
        .expect("config");

        let scripts = config.list_script_paths_for_info().await;
        assert_eq!(
            scripts,
            vec![
                PathBuf::from("scripts/shared-one.lua"),
                PathBuf::from("scripts/shared-two.lua"),
                PathBuf::from("scripts/dup.lua"),
                PathBuf::from("scripts/name.lua"),
                PathBuf::from("scripts/file.lua"),
                PathBuf::from("scripts/dir.lua"),
                PathBuf::from("scripts/watch.lua"),
            ]
        );
    }

    #[tokio::test]
    async fn list_script_paths_for_info_expands_glob_patterns() {
        let dir = tempdir().expect("temp dir");
        let scripts_dir = dir.path().join("scripts");
        tokio::fs::create_dir_all(scripts_dir.join("nested"))
            .await
            .expect("create scripts");

        let alpha = scripts_dir.join("alpha.lua");
        let beta = scripts_dir.join("nested").join("beta.lua");
        let other = scripts_dir.join("nested").join("ignore.txt");

        tokio::fs::write(&alpha, "return {} ")
            .await
            .expect("write alpha");
        tokio::fs::write(&beta, "return {} ")
            .await
            .expect("write beta");
        tokio::fs::write(&other, "ignore").await.expect("write txt");

        let config: AppConfig = toml::from_str(&format!(
            r#"
[shared]
scripts = ["{}/*.lua", "{}/**/*.lua"]
"#,
            scripts_dir.display(),
            scripts_dir.display()
        ))
        .expect("config");

        let scripts = config.list_script_paths_for_info().await;
        assert_eq!(scripts, vec![alpha, beta]);
    }
}
