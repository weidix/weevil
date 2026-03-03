use std::collections::HashSet;

use serde::Deserialize;

use crate::errors::AppError;

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "kebab-case", deny_unknown_fields)]
pub(crate) struct TranslationConfig {
    pub(crate) target_lang: Option<String>,
    pub(crate) keys: Option<TranslationKeyList>,
    pub(crate) endpoints: Option<TranslationEndpointConfig>,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct ResolvedTranslationConfig {
    target_lang: Option<String>,
    keys: Vec<TranslationKey>,
    endpoint: Option<TranslationEndpointConfig>,
}

impl ResolvedTranslationConfig {
    pub(crate) fn is_enabled(&self) -> bool {
        !self.keys.is_empty()
    }

    pub(crate) fn target_lang(&self) -> Option<&str> {
        self.target_lang.as_deref()
    }

    pub(crate) fn keys(&self) -> &[TranslationKey] {
        &self.keys
    }

    pub(crate) fn endpoint(&self) -> Option<&TranslationEndpointConfig> {
        self.endpoint.as_ref()
    }
}

pub(crate) fn resolve_translation_config(
    mode: Option<&TranslationConfig>,
    shared: Option<&TranslationConfig>,
) -> Result<ResolvedTranslationConfig, AppError> {
    let target_lang = mode
        .and_then(|config| config.target_lang.clone())
        .or_else(|| shared.and_then(|config| config.target_lang.clone()))
        .and_then(|value| {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        });

    let keys = if let Some(list) = mode.and_then(|config| config.keys.as_ref()) {
        list.to_vec()
    } else if let Some(list) = shared.and_then(|config| config.keys.as_ref()) {
        list.to_vec()
    } else {
        Vec::new()
    };

    let endpoint = if let Some(value) = mode.and_then(|config| config.endpoints.as_ref()) {
        Some(value.clone())
    } else {
        shared.and_then(|config| config.endpoints.as_ref()).cloned()
    };

    let (keys, unknown_keys) = parse_translation_keys(keys);
    if !unknown_keys.is_empty() {
        return Err(AppError::FetchRuntime {
            reason: format!("translation keys contain unsupported values: {unknown_keys:?}"),
        });
    }

    if keys.is_empty() {
        return Ok(ResolvedTranslationConfig::default());
    }

    if target_lang.is_none() {
        return Err(AppError::FetchRuntime {
            reason: "translation target-lang is required when translation keys are configured"
                .to_string(),
        });
    }

    if endpoint.is_none() {
        return Err(AppError::FetchRuntime {
            reason: "translation endpoints are required when translation keys are configured"
                .to_string(),
        });
    }

    Ok(ResolvedTranslationConfig {
        target_lang,
        keys,
        endpoint,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum TranslationKey {
    Title,
    OriginalTitle,
    SortTitle,
    Plot,
    Outline,
    Tagline,
    Studio,
    Director,
    Credits,
    Genre,
    Tag,
    Country,
    SetName,
    SetOverview,
    ActorName,
    ActorRole,
    ActorGender,
    Trailer,
    FileInfo,
    DateAdded,
}

impl TranslationKey {
    fn parse(raw: &str) -> Option<Self> {
        let normalized = normalize_key(raw);
        match normalized.as_str() {
            "title" => Some(Self::Title),
            "originaltitle" => Some(Self::OriginalTitle),
            "sorttitle" => Some(Self::SortTitle),
            "plot" => Some(Self::Plot),
            "outline" => Some(Self::Outline),
            "tagline" => Some(Self::Tagline),
            "studio" => Some(Self::Studio),
            "director" => Some(Self::Director),
            "credits" => Some(Self::Credits),
            "genre" => Some(Self::Genre),
            "tag" => Some(Self::Tag),
            "country" => Some(Self::Country),
            "setname" | "set" => Some(Self::SetName),
            "setoverview" => Some(Self::SetOverview),
            "actorname" | "actor" => Some(Self::ActorName),
            "actorrole" => Some(Self::ActorRole),
            "actorgender" => Some(Self::ActorGender),
            "trailer" => Some(Self::Trailer),
            "fileinfo" => Some(Self::FileInfo),
            "dateadded" => Some(Self::DateAdded),
            _ => None,
        }
    }
}

fn normalize_key(raw: &str) -> String {
    raw.chars()
        .filter(|value| !value.is_whitespace() && *value != '-' && *value != '_' && *value != '.')
        .flat_map(|value| value.to_lowercase())
        .collect()
}

fn parse_translation_keys(keys: Vec<String>) -> (Vec<TranslationKey>, Vec<String>) {
    let mut resolved = Vec::new();
    let mut unknown = Vec::new();
    let mut seen = HashSet::new();

    for key in keys {
        let trimmed = key.trim();
        if trimmed.is_empty() {
            continue;
        }
        match TranslationKey::parse(trimmed) {
            Some(resolved_key) => {
                if seen.insert(resolved_key) {
                    resolved.push(resolved_key);
                }
            }
            None => unknown.push(trimmed.to_string()),
        }
    }

    (resolved, unknown)
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub(crate) enum TranslationKeyList {
    One(String),
    Many(Vec<String>),
}

impl TranslationKeyList {
    pub(crate) fn to_vec(&self) -> Vec<String> {
        match self {
            TranslationKeyList::One(value) => vec![value.clone()],
            TranslationKeyList::Many(values) => values.clone(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub(crate) enum TranslationEndpointConfig {
    #[serde(rename = "openai")]
    OpenAI {
        url: String,
        #[serde(rename = "api-key")]
        api_key: String,
        model: String,
    },
    #[serde(rename = "google-free")]
    GoogleFree,
    Google {
        api_key: String,
        url: Option<String>,
    },
    DeepL {
        auth_key: String,
        url: Option<String>,
    },
}
