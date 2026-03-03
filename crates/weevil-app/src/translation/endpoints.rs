use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::errors::AppError;

use super::config::TranslationEndpointConfig;

#[derive(Clone)]
pub(super) enum TranslationEndpoint {
    OpenAI(OpenAIEndpoint),
    GoogleFree(GoogleFreeEndpoint),
    Google(GoogleEndpoint),
    DeepL(DeepLEndpoint),
    #[cfg(test)]
    Stub(StubEndpoint),
}

impl TranslationEndpoint {
    pub(super) async fn translate(
        &self,
        client: &Client,
        target_lang: &str,
        texts: &[String],
    ) -> Result<Vec<String>, String> {
        match self {
            TranslationEndpoint::OpenAI(endpoint) => {
                endpoint.translate(client, target_lang, texts).await
            }
            TranslationEndpoint::GoogleFree(endpoint) => {
                endpoint.translate(client, target_lang, texts).await
            }
            TranslationEndpoint::Google(endpoint) => {
                endpoint.translate(client, target_lang, texts).await
            }
            TranslationEndpoint::DeepL(endpoint) => {
                endpoint.translate(client, target_lang, texts).await
            }
            #[cfg(test)]
            TranslationEndpoint::Stub(endpoint) => endpoint.translate(texts),
        }
    }
}

pub(super) fn build_endpoint(
    config: &TranslationEndpointConfig,
) -> Result<TranslationEndpoint, AppError> {
    let endpoint = match config {
        TranslationEndpointConfig::OpenAI {
            url,
            api_key,
            model,
        } => TranslationEndpoint::OpenAI(OpenAIEndpoint {
            url: url.clone(),
            api_key: api_key.clone(),
            model: model.clone(),
        }),
        TranslationEndpointConfig::GoogleFree => {
            TranslationEndpoint::GoogleFree(GoogleFreeEndpoint)
        }
        TranslationEndpointConfig::Google { api_key, url } => {
            TranslationEndpoint::Google(GoogleEndpoint {
                api_key: api_key.clone(),
                url: url.clone().unwrap_or_else(|| {
                    "https://translation.googleapis.com/language/translate/v2".to_string()
                }),
            })
        }
        TranslationEndpointConfig::DeepL { auth_key, url } => {
            TranslationEndpoint::DeepL(DeepLEndpoint {
                auth_key: auth_key.clone(),
                url: url
                    .clone()
                    .unwrap_or_else(|| "https://api-free.deepl.com/v2/translate".to_string()),
            })
        }
    };
    Ok(endpoint)
}

#[derive(Clone)]
pub(super) struct GoogleFreeEndpoint;

impl GoogleFreeEndpoint {
    async fn translate(
        &self,
        client: &Client,
        target_lang: &str,
        texts: &[String],
    ) -> Result<Vec<String>, String> {
        let mut translated = Vec::with_capacity(texts.len());
        for text in texts {
            translated.push(translate_one_google_free(client, target_lang, text).await?);
        }
        Ok(translated)
    }
}

async fn translate_one_google_free(
    client: &Client,
    target_lang: &str,
    text: &str,
) -> Result<String, String> {
    let response = client
        .get("https://translate.googleapis.com/translate_a/single")
        .query(&[
            ("client", "gtx"),
            ("sl", "auto"),
            ("tl", target_lang),
            ("dt", "t"),
            ("q", text),
        ])
        .send()
        .await
        .map_err(|err| format!("google-free translation request failed: {err}"))?;

    let status = response.status();
    if !status.is_success() {
        return Err(format!("google-free translation returned HTTP {status}"));
    }

    let payload: Value = response
        .json()
        .await
        .map_err(|err| format!("google-free translation response parse failed: {err}"))?;
    parse_google_free_translation(payload)
}

fn parse_google_free_translation(payload: Value) -> Result<String, String> {
    let top = payload
        .as_array()
        .ok_or_else(|| "google-free translation response is not an array".to_string())?;
    let segments = top
        .first()
        .and_then(Value::as_array)
        .ok_or_else(|| "google-free translation response missing text segments".to_string())?;

    let mut output = String::new();
    for segment in segments {
        let Some(segment_fields) = segment.as_array() else {
            continue;
        };
        let Some(translated_piece) = segment_fields.first().and_then(Value::as_str) else {
            continue;
        };
        output.push_str(translated_piece);
    }

    if output.is_empty() {
        return Err("google-free translation response contains empty translated text".to_string());
    }
    Ok(output)
}

#[derive(Clone)]
pub(super) struct OpenAIEndpoint {
    url: String,
    api_key: String,
    model: String,
}

#[derive(Serialize)]
struct OpenAIRequest<'a> {
    model: &'a str,
    messages: Vec<OpenAIMessage>,
    temperature: f32,
}

#[derive(Serialize)]
struct OpenAIMessage {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct OpenAIResponse {
    choices: Vec<OpenAIChoice>,
}

#[derive(Deserialize)]
struct OpenAIChoice {
    message: OpenAIMessageResponse,
}

#[derive(Deserialize)]
struct OpenAIMessageResponse {
    content: String,
}

impl OpenAIEndpoint {
    async fn translate(
        &self,
        client: &Client,
        target_lang: &str,
        texts: &[String],
    ) -> Result<Vec<String>, String> {
        let payload = OpenAIRequest {
            model: &self.model,
            messages: build_openai_messages(target_lang, texts)?,
            temperature: 0.0,
        };

        let response = client
            .post(&self.url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&payload)
            .send()
            .await
            .map_err(|err| format!("openai translation request failed: {err}"))?;

        let status = response.status();
        if !status.is_success() {
            return Err(format!("openai translation returned HTTP {status}"));
        }

        let payload: OpenAIResponse = response
            .json()
            .await
            .map_err(|err| format!("openai translation response parse failed: {err}"))?;
        let first = payload
            .choices
            .into_iter()
            .next()
            .ok_or_else(|| "openai translation response missing choices".to_string())?;
        parse_openai_translations(first.message.content.as_str())
    }
}

fn build_openai_messages(
    target_lang: &str,
    texts: &[String],
) -> Result<Vec<OpenAIMessage>, String> {
    let input = serde_json::to_string(texts)
        .map_err(|err| format!("failed to encode translation input: {err}"))?;
    let system = format!(
        "You are a translation engine. Translate each item in the JSON array to {target_lang}. Return only a JSON array of strings in the same order, with no extra text."
    );
    Ok(vec![
        OpenAIMessage {
            role: "system".to_string(),
            content: system,
        },
        OpenAIMessage {
            role: "user".to_string(),
            content: input,
        },
    ])
}

fn parse_openai_translations(content: &str) -> Result<Vec<String>, String> {
    let cleaned = strip_code_fence(content);
    let value: serde_json::Value = serde_json::from_str(cleaned.as_str())
        .map_err(|err| format!("openai translation returned invalid JSON: {err}"))?;
    let array = value
        .as_array()
        .ok_or_else(|| "openai translation response is not a JSON array".to_string())?;
    let mut translated = Vec::with_capacity(array.len());
    for item in array {
        let value = item
            .as_str()
            .ok_or_else(|| "openai translation response contains non-string values".to_string())?;
        translated.push(value.to_string());
    }
    Ok(translated)
}

fn strip_code_fence(text: &str) -> String {
    let trimmed = text.trim();
    let Some(stripped) = trimmed.strip_prefix("```") else {
        return trimmed.to_string();
    };
    let mut inner = stripped.trim_start();
    if let Some(rest) = inner.strip_prefix("json") {
        inner = rest.trim_start();
    }
    if let Some(end) = inner.rfind("```") {
        return inner[..end].trim().to_string();
    }
    trimmed.to_string()
}

#[derive(Clone)]
pub(super) struct GoogleEndpoint {
    api_key: String,
    url: String,
}

#[derive(Serialize)]
struct GoogleRequest<'a> {
    q: &'a [String],
    target: &'a str,
}

#[derive(Deserialize)]
struct GoogleResponse {
    data: GoogleData,
}

#[derive(Deserialize)]
struct GoogleData {
    translations: Vec<GoogleTranslation>,
}

#[derive(Deserialize)]
struct GoogleTranslation {
    #[serde(rename = "translatedText")]
    translated_text: String,
}

impl GoogleEndpoint {
    async fn translate(
        &self,
        client: &Client,
        target_lang: &str,
        texts: &[String],
    ) -> Result<Vec<String>, String> {
        let response = client
            .post(&self.url)
            .header("X-Goog-Api-Key", &self.api_key)
            .json(&GoogleRequest {
                q: texts,
                target: target_lang,
            })
            .send()
            .await
            .map_err(|err| format!("google translation request failed: {err}"))?;

        let status = response.status();
        if !status.is_success() {
            return Err(format!("google translation returned HTTP {status}"));
        }

        let payload: GoogleResponse = response
            .json()
            .await
            .map_err(|err| format!("google translation response parse failed: {err}"))?;
        Ok(payload
            .data
            .translations
            .into_iter()
            .map(|item| item.translated_text)
            .collect())
    }
}

#[derive(Clone)]
pub(super) struct DeepLEndpoint {
    auth_key: String,
    url: String,
}

#[derive(Deserialize)]
struct DeepLResponse {
    translations: Vec<DeepLTranslation>,
}

#[derive(Deserialize)]
struct DeepLTranslation {
    text: String,
}

impl DeepLEndpoint {
    async fn translate(
        &self,
        client: &Client,
        target_lang: &str,
        texts: &[String],
    ) -> Result<Vec<String>, String> {
        let mut params = Vec::with_capacity(texts.len() + 2);
        params.push(("auth_key".to_string(), self.auth_key.clone()));
        params.push(("target_lang".to_string(), target_lang.to_string()));
        for text in texts {
            params.push(("text".to_string(), text.clone()));
        }

        let response = client
            .post(&self.url)
            .form(&params)
            .send()
            .await
            .map_err(|err| format!("deepl translation request failed: {err}"))?;

        let status = response.status();
        if !status.is_success() {
            return Err(format!("deepl translation returned HTTP {status}"));
        }

        let payload: DeepLResponse = response
            .json()
            .await
            .map_err(|err| format!("deepl translation response parse failed: {err}"))?;
        Ok(payload
            .translations
            .into_iter()
            .map(|item| item.text)
            .collect())
    }
}

#[cfg(test)]
#[derive(Clone)]
pub(super) struct StubEndpoint {
    calls: std::sync::Arc<std::sync::Mutex<Vec<Vec<String>>>>,
    prefix: String,
}

#[cfg(test)]
impl StubEndpoint {
    pub(super) fn with_prefix(
        prefix: &str,
    ) -> (
        TranslationEndpoint,
        std::sync::Arc<std::sync::Mutex<Vec<Vec<String>>>>,
    ) {
        let calls = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        (
            TranslationEndpoint::Stub(Self {
                calls: std::sync::Arc::clone(&calls),
                prefix: prefix.to_string(),
            }),
            calls,
        )
    }

    fn translate(&self, texts: &[String]) -> Result<Vec<String>, String> {
        let mut guard = self
            .calls
            .lock()
            .map_err(|_| "translation stub lock poisoned".to_string())?;
        guard.push(texts.to_vec());
        Ok(texts
            .iter()
            .map(|value| format!("{}{}", self.prefix, value))
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::parse_google_free_translation;

    #[test]
    fn parse_google_free_translation_joins_segments() {
        let payload = serde_json::json!([
            [
                ["hello ", "hola", null, null, 1],
                ["world", "mundo", null, null, 1]
            ],
            null,
            "es"
        ]);
        let translated = parse_google_free_translation(payload).expect("translated");
        assert_eq!(translated, "hello world");
    }
}
