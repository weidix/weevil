mod config;
mod detector;
mod endpoints;
mod translator;

pub(crate) use config::{ResolvedTranslationConfig, TranslationConfig, resolve_translation_config};
pub(crate) use translator::MovieTranslator;

#[cfg(test)]
mod tests;
