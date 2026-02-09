use std::path::PathBuf;

use super::{AppConfig, StringPathList, dedupe_paths};

impl AppConfig {
    pub(crate) fn list_script_paths_for_info(&self) -> Vec<PathBuf> {
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

        dedupe_paths(scripts)
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
    use super::*;

    #[test]
    fn list_script_paths_for_info_collects_all_sections() {
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

        let scripts = config.list_script_paths_for_info();
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
}
