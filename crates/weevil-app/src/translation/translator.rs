use std::fmt;

use reqwest::Client;

use crate::errors::AppError;
use crate::nfo::{Actor, Movie};

use super::config::{ResolvedTranslationConfig, TranslationKey};
use super::detector::LanguageDetector;
use super::endpoints::{TranslationEndpoint, build_endpoint};

#[derive(Clone)]
pub(crate) struct MovieTranslator {
    enabled: bool,
    target_lang: String,
    keys: Vec<TranslationKey>,
    endpoint: Option<TranslationEndpoint>,
    client: Client,
    detector: LanguageDetector,
}

impl fmt::Debug for MovieTranslator {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MovieTranslator")
            .field("enabled", &self.enabled)
            .field("target_lang", &self.target_lang)
            .field("keys", &self.keys)
            .field("has_endpoint", &self.endpoint.is_some())
            .finish()
    }
}

impl MovieTranslator {
    pub(crate) fn new(config: &ResolvedTranslationConfig) -> Result<Self, AppError> {
        if !config.is_enabled() {
            return Ok(Self::disabled());
        }

        let target_lang = config.target_lang().ok_or_else(|| AppError::FetchRuntime {
            reason: "translation target-lang is required when translation is enabled".to_string(),
        })?;
        let detector = LanguageDetector::new(target_lang)?;
        let endpoint_config = config.endpoint().ok_or_else(|| AppError::FetchRuntime {
            reason: "translation endpoints are required when translation is enabled".to_string(),
        })?;
        let endpoint = build_endpoint(endpoint_config)?;

        Ok(Self {
            enabled: true,
            target_lang: target_lang.to_string(),
            keys: config.keys().to_vec(),
            endpoint: Some(endpoint),
            client: Client::new(),
            detector,
        })
    }

    pub(crate) fn disabled() -> Self {
        Self {
            enabled: false,
            target_lang: String::new(),
            keys: Vec::new(),
            endpoint: None,
            client: Client::new(),
            detector: LanguageDetector::unknown(),
        }
    }

    pub(crate) async fn translate_movie(&self, movie: &mut Movie) -> Result<bool, AppError> {
        if !self.enabled {
            return Ok(false);
        }

        let mut changed = false;
        for key in &self.keys {
            match key {
                TranslationKey::Title => {
                    changed |= self.translate_option_value(&mut movie.title).await?;
                }
                TranslationKey::OriginalTitle => {
                    changed |= self
                        .translate_option_value(&mut movie.originaltitle)
                        .await?;
                }
                TranslationKey::SortTitle => {
                    changed |= self.translate_option_value(&mut movie.sorttitle).await?;
                }
                TranslationKey::Plot => {
                    changed |= self.translate_option_value(&mut movie.plot).await?;
                }
                TranslationKey::Outline => {
                    changed |= self.translate_option_value(&mut movie.outline).await?;
                }
                TranslationKey::Tagline => {
                    changed |= self.translate_option_value(&mut movie.tagline).await?;
                }
                TranslationKey::Studio => {
                    changed |= self.translate_option_value(&mut movie.studio).await?;
                }
                TranslationKey::Director => {
                    changed |= self.translate_option_value(&mut movie.director).await?;
                }
                TranslationKey::Credits => {
                    changed |= self.translate_list_values(&mut movie.credits).await?;
                }
                TranslationKey::Genre => {
                    changed |= self.translate_list_values(&mut movie.genre).await?;
                }
                TranslationKey::Tag => {
                    changed |= self.translate_list_values(&mut movie.tag).await?;
                }
                TranslationKey::Country => {
                    changed |= self.translate_list_values(&mut movie.country).await?;
                }
                TranslationKey::SetName => {
                    if let Some(set_info) = movie.set_info.as_mut() {
                        changed |= self.translate_option_value(&mut set_info.name).await?;
                    }
                }
                TranslationKey::SetOverview => {
                    if let Some(set_info) = movie.set_info.as_mut() {
                        changed |= self.translate_option_value(&mut set_info.overview).await?;
                    }
                }
                TranslationKey::ActorName => {
                    changed |= self
                        .translate_actor_field(&mut movie.actor, ActorField::Name)
                        .await?;
                }
                TranslationKey::ActorRole => {
                    changed |= self
                        .translate_actor_field(&mut movie.actor, ActorField::Role)
                        .await?;
                }
                TranslationKey::ActorGender => {
                    changed |= self
                        .translate_actor_field(&mut movie.actor, ActorField::Gender)
                        .await?;
                }
                TranslationKey::Trailer => {
                    changed |= self.translate_option_value(&mut movie.trailer).await?;
                }
                TranslationKey::FileInfo => {
                    changed |= self.translate_option_value(&mut movie.fileinfo).await?;
                }
                TranslationKey::DateAdded => {
                    changed |= self.translate_option_value(&mut movie.dateadded).await?;
                }
            }
        }

        Ok(changed)
    }

    async fn translate_option_value(&self, value: &mut Option<String>) -> Result<bool, AppError> {
        let Some(current) = value.as_ref() else {
            return Ok(false);
        };
        let trimmed = current.trim();
        if trimmed.is_empty() || !self.should_translate(trimmed) {
            return Ok(false);
        }

        let translated = self.translate_texts(&[trimmed.to_string()]).await?;
        let Some(result) = translated.into_iter().next() else {
            return Err(AppError::FetchRuntime {
                reason: "translation returned no results for single value".to_string(),
            });
        };

        if result.is_empty() || result == *current {
            return Ok(false);
        }

        *value = Some(result);
        Ok(true)
    }

    async fn translate_list_values(&self, values: &mut Vec<String>) -> Result<bool, AppError> {
        if values.is_empty() {
            return Ok(false);
        }

        let mut pending = Vec::new();
        let mut positions = Vec::new();
        for (index, value) in values.iter().enumerate() {
            let trimmed = value.trim();
            if trimmed.is_empty() || !self.should_translate(trimmed) {
                continue;
            }
            pending.push(trimmed.to_string());
            positions.push(index);
        }

        if pending.is_empty() {
            return Ok(false);
        }

        let translated = self.translate_texts(&pending).await?;
        if translated.len() != pending.len() {
            return Err(AppError::FetchRuntime {
                reason: format!(
                    "translation returned {translated_len} results for {pending_len} values",
                    translated_len = translated.len(),
                    pending_len = pending.len()
                ),
            });
        }

        let mut changed = false;
        for (index, translated_value) in translated.into_iter().enumerate() {
            if translated_value.is_empty() {
                continue;
            }
            let target_index = positions[index];
            if values[target_index] != translated_value {
                values[target_index] = translated_value;
                changed = true;
            }
        }

        Ok(changed)
    }

    async fn translate_actor_field(
        &self,
        actors: &mut [Actor],
        field: ActorField,
    ) -> Result<bool, AppError> {
        if actors.is_empty() {
            return Ok(false);
        }

        let mut pending = Vec::new();
        let mut positions = Vec::new();
        for (index, actor) in actors.iter().enumerate() {
            let value = match field {
                ActorField::Name => actor.name.as_deref(),
                ActorField::Role => actor.role.as_deref(),
                ActorField::Gender => actor.gender.as_deref(),
            };
            let Some(value) = value else {
                continue;
            };
            let trimmed = value.trim();
            if trimmed.is_empty() || !self.should_translate(trimmed) {
                continue;
            }
            pending.push(trimmed.to_string());
            positions.push(index);
        }

        if pending.is_empty() {
            return Ok(false);
        }

        let translated = self.translate_texts(&pending).await?;
        if translated.len() != pending.len() {
            return Err(AppError::FetchRuntime {
                reason: format!(
                    "translation returned {translated_len} results for {pending_len} actor values",
                    translated_len = translated.len(),
                    pending_len = pending.len()
                ),
            });
        }

        let mut changed = false;
        for (index, translated_value) in translated.into_iter().enumerate() {
            if translated_value.is_empty() {
                continue;
            }
            let actor_index = positions[index];
            let actor = &mut actors[actor_index];
            let target = match field {
                ActorField::Name => &mut actor.name,
                ActorField::Role => &mut actor.role,
                ActorField::Gender => &mut actor.gender,
            };
            if target.as_deref() != Some(translated_value.as_str()) {
                *target = Some(translated_value);
                changed = true;
            }
        }

        Ok(changed)
    }

    fn should_translate(&self, text: &str) -> bool {
        !self.detector.is_target_lang(text)
    }

    async fn translate_texts(&self, texts: &[String]) -> Result<Vec<String>, AppError> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }
        let Some(endpoint) = self.endpoint.as_ref() else {
            return Err(AppError::FetchRuntime {
                reason: "translation endpoints are not configured".to_string(),
            });
        };

        endpoint
            .translate(&self.client, &self.target_lang, texts)
            .await
            .map_err(|reason| AppError::FetchRuntime {
                reason: format!("translation failed: {reason}"),
            })
    }
}

#[derive(Clone, Copy)]
enum ActorField {
    Name,
    Role,
    Gender,
}

#[cfg(test)]
impl MovieTranslator {
    pub(super) fn new_for_tests(
        target_lang: &str,
        keys: Vec<TranslationKey>,
        endpoint: TranslationEndpoint,
    ) -> Self {
        let detector =
            LanguageDetector::new(target_lang).unwrap_or_else(|_| LanguageDetector::unknown());
        Self {
            enabled: true,
            target_lang: target_lang.to_string(),
            keys,
            endpoint: Some(endpoint),
            client: Client::new(),
            detector,
        }
    }
}
