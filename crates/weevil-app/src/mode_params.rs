use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MultiFolderStrategy {
    HardLink,
    SoftLink,
    First,
}

#[derive(Debug, Clone)]
pub(crate) struct FileModeParams {
    script: PathBuf,
    output_template: String,
    input_name_rules: Vec<String>,
    folder_multi: MultiFolderStrategy,
}

impl FileModeParams {
    pub(crate) fn new(
        script: PathBuf,
        output_template: String,
        input_name_rules: Vec<String>,
        folder_multi: MultiFolderStrategy,
    ) -> Self {
        Self {
            script,
            output_template,
            input_name_rules,
            folder_multi,
        }
    }

    pub(crate) fn script(&self) -> &Path {
        &self.script
    }

    pub(crate) fn output_template(&self) -> &str {
        &self.output_template
    }

    pub(crate) fn input_name_rules(&self) -> &[String] {
        &self.input_name_rules
    }

    pub(crate) fn folder_multi(&self) -> MultiFolderStrategy {
        self.folder_multi
    }
}
