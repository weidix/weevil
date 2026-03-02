use std::path::PathBuf;

use crate::source_priority::SourcePriority;
use crate::source_runner::NodeValueMapper;
use crate::translation::MovieTranslator;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MultiFolderStrategy {
    HardLink,
    SoftLink,
    First,
}

#[derive(Debug, Clone)]
pub(crate) struct FileModeParams {
    scripts: Vec<PathBuf>,
    output_template: String,
    input_name_rules: Vec<String>,
    folder_multi: MultiFolderStrategy,
    multi_source: bool,
    save_images: bool,
    multi_source_max_sources: u32,
    source_priority: SourcePriority,
    node_value_mapper: NodeValueMapper,
    movie_translator: MovieTranslator,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FetchModeParams {
    fetch_threads: u32,
    throttle_same_script: bool,
    script_throttle_base_ms: u64,
}

impl FetchModeParams {
    pub(crate) fn new(
        fetch_threads: u32,
        throttle_same_script: bool,
        script_throttle_base_ms: u64,
    ) -> Self {
        Self {
            fetch_threads,
            throttle_same_script,
            script_throttle_base_ms,
        }
    }

    pub(crate) fn fetch_threads(&self) -> u32 {
        self.fetch_threads
    }

    pub(crate) fn throttle_same_script(&self) -> bool {
        self.throttle_same_script
    }

    pub(crate) fn script_throttle_base_ms(&self) -> u64 {
        self.script_throttle_base_ms
    }

    pub(crate) fn multithread_enabled(&self) -> bool {
        self.fetch_threads == 0 || self.fetch_threads > 1
    }
}

impl FileModeParams {
    pub(crate) fn new(
        scripts: Vec<PathBuf>,
        output_template: String,
        input_name_rules: Vec<String>,
        folder_multi: MultiFolderStrategy,
        multi_source: bool,
        save_images: bool,
        multi_source_max_sources: u32,
        source_priority: SourcePriority,
        node_value_mapper: NodeValueMapper,
        movie_translator: MovieTranslator,
    ) -> Self {
        Self {
            scripts,
            output_template,
            input_name_rules,
            folder_multi,
            multi_source,
            save_images,
            multi_source_max_sources,
            source_priority,
            node_value_mapper,
            movie_translator,
        }
    }

    pub(crate) fn scripts(&self) -> &[PathBuf] {
        &self.scripts
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

    pub(crate) fn multi_source(&self) -> bool {
        self.multi_source
    }

    pub(crate) fn multi_source_max_sources(&self) -> u32 {
        self.multi_source_max_sources
    }

    pub(crate) fn save_images(&self) -> bool {
        self.save_images
    }

    pub(crate) fn source_priority(&self) -> &SourcePriority {
        &self.source_priority
    }

    pub(crate) fn node_value_mapper(&self) -> &NodeValueMapper {
        &self.node_value_mapper
    }

    pub(crate) fn movie_translator(&self) -> &MovieTranslator {
        &self.movie_translator
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_mode_params_exposes_node_value_mapper() {
        let params = FileModeParams::new(
            vec![],
            "out/{title}".to_string(),
            vec![],
            MultiFolderStrategy::First,
            false,
            false,
            2,
            SourcePriority::default(),
            NodeValueMapper::from_csv("genre,剧情,Drama\n").expect("mapper"),
            MovieTranslator::disabled(),
        );

        assert!(params.node_value_mapper().has_rules());
    }
}
