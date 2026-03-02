use whatlang::Lang;

use crate::errors::AppError;

const MIN_DETECT_CONFIDENCE: f64 = 0.6;

#[derive(Clone)]
pub(super) struct LanguageDetector {
    target: Option<Lang>,
}

impl LanguageDetector {
    pub(super) fn new(target_lang: &str) -> Result<Self, AppError> {
        let target = parse_lang(target_lang).ok_or_else(|| AppError::FetchRuntime {
            reason: format!(
                "translation target-lang {target_lang:?} is not supported for detection"
            ),
        })?;
        Ok(Self {
            target: Some(target),
        })
    }

    pub(super) fn unknown() -> Self {
        Self { target: None }
    }

    pub(super) fn is_target_lang(&self, text: &str) -> bool {
        let Some(target) = self.target else {
            return false;
        };
        let Some(info) = whatlang::detect(text) else {
            return false;
        };
        if info.confidence() < MIN_DETECT_CONFIDENCE {
            return false;
        }
        info.lang() == target
    }
}

fn parse_lang(raw: &str) -> Option<Lang> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    let normalized = trimmed.to_ascii_lowercase();
    if let Some(lang) = Lang::from_code(normalized.as_str()) {
        return Some(lang);
    }

    let base = normalized
        .split(|value| value == '-' || value == '_')
        .next()
        .unwrap_or_default();
    if let Some(lang) = Lang::from_code(base) {
        return Some(lang);
    }

    let mapped = map_iso639_1(base)?;
    Lang::from_code(mapped)
}

fn map_iso639_1(code: &str) -> Option<&'static str> {
    match code {
        "en" => Some("eng"),
        "es" => Some("spa"),
        "fr" => Some("fra"),
        "de" => Some("deu"),
        "ru" => Some("rus"),
        "ja" => Some("jpn"),
        "ko" => Some("kor"),
        "zh" => Some("cmn"),
        _ => None,
    }
}
