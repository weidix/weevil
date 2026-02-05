use std::sync::Arc;

use reqwest::Client as AsyncClient;
use reqwest::Version;
use reqwest::blocking::Client as BlockingClient;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use url::Url;

use crate::error::LuaPluginError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrustedUrl {
    original: String,
    scheme: String,
    host: String,
    port: Option<u16>,
    path_pattern: String,
    path_has_wildcard: bool,
}

impl TrustedUrl {
    pub fn parse(input: &str) -> Result<Self, LuaPluginError> {
        let url = Url::parse(input).map_err(|_| LuaPluginError::InvalidTrustedUrl {
            value: input.to_string(),
        })?;
        let scheme = url.scheme().to_string();
        if scheme != "http" && scheme != "https" {
            return Err(LuaPluginError::TrustedUrlUnsupportedScheme {
                scheme,
                value: input.to_string(),
            });
        }
        if !url.has_authority() {
            return Err(LuaPluginError::InvalidTrustedUrl {
                value: input.to_string(),
            });
        }
        let host = url
            .host_str()
            .ok_or_else(|| LuaPluginError::TrustedUrlMissingHost {
                value: input.to_string(),
            })?
            .to_string();
        let path_pattern = if url.path().is_empty() {
            "/".to_string()
        } else {
            url.path().to_string()
        };
        let path_has_wildcard = path_pattern.as_bytes().iter().any(|ch| *ch == b'*');
        Ok(Self {
            original: input.to_string(),
            scheme: url.scheme().to_string(),
            host,
            port: url.port(),
            path_pattern,
            path_has_wildcard,
        })
    }

    pub fn original(&self) -> &str {
        &self.original
    }

    pub fn matches(&self, url: &Url) -> bool {
        if url.scheme() != self.scheme {
            return false;
        }
        let Some(host) = url.host_str() else {
            return false;
        };
        if !host.eq_ignore_ascii_case(&self.host) {
            return false;
        }
        if url.port() != self.port {
            return false;
        }
        if self.path_has_wildcard {
            path_glob_matches(&self.path_pattern, url.path())
        } else {
            url.path().starts_with(&self.path_pattern)
        }
    }
}

fn path_glob_matches(pattern: &str, path: &str) -> bool {
    let pattern = pattern.as_bytes();
    let path = path.as_bytes();
    let mut pattern_index = 0;
    let mut path_index = 0;
    let mut star_index = None;
    let mut star_match_index = 0;
    while path_index < path.len() {
        if pattern_index < pattern.len() && pattern[pattern_index] == b'*' {
            star_index = Some(pattern_index);
            star_match_index = path_index;
            pattern_index += 1;
        } else if pattern_index < pattern.len() && pattern[pattern_index] == path[path_index] {
            pattern_index += 1;
            path_index += 1;
        } else if let Some(star_index) = star_index {
            if star_match_index < path.len() && path[star_match_index] == b'/' {
                return false;
            }
            star_match_index += 1;
            path_index = star_match_index;
            pattern_index = star_index + 1;
        } else {
            return false;
        }
    }
    while pattern_index < pattern.len() && pattern[pattern_index] == b'*' {
        pattern_index += 1;
    }
    pattern_index == pattern.len()
}

#[cfg(test)]
mod tests {
    use super::TrustedUrl;
    use url::Url;

    fn trusted_url(input: &str) -> TrustedUrl {
        TrustedUrl::parse(input).expect("trusted url")
    }

    fn parsed_url(input: &str) -> Url {
        Url::parse(input).expect("url")
    }

    #[test]
    fn trusted_url_prefix_matches_without_wildcards() {
        let trusted = trusted_url("https://example.com/foo/");
        assert!(trusted.matches(&parsed_url("https://example.com/foo/bar")));
        assert!(trusted.matches(&parsed_url("https://example.com/foo/")));
        assert!(!trusted.matches(&parsed_url("https://example.com/foobar/")));
    }

    #[test]
    fn trusted_url_matches_single_segment_wildcard() {
        let trusted = trusted_url("https://example.com/foo/*/bar");
        assert!(trusted.matches(&parsed_url("https://example.com/foo/a/bar")));
        assert!(!trusted.matches(&parsed_url("https://example.com/foo/a/b/bar")));
    }

    #[test]
    fn trusted_url_matches_suffix_wildcard() {
        let trusted = trusted_url("https://example.com/*.nfo");
        assert!(trusted.matches(&parsed_url("https://example.com/movie.nfo")));
        assert!(!trusted.matches(&parsed_url("https://example.com/movie.txt")));
        assert!(!trusted.matches(&parsed_url("https://example.com/dir/movie.nfo")));
    }
}

#[derive(Debug, Clone)]
pub struct HttpClient {
    allowlist: Arc<Vec<TrustedUrl>>,
    blocking: BlockingClient,
    #[cfg(feature = "async")]
    async_client: AsyncClient,
}

#[derive(Debug, Default, Clone)]
pub struct HttpRequestOptions {
    pub(crate) headers: Vec<(String, String)>,
    pub(crate) version: Option<Version>,
}

impl HttpClient {
    pub fn new(allowlist: Vec<TrustedUrl>) -> Result<Self, LuaPluginError> {
        let blocking = BlockingClient::builder()
            .user_agent("weevil-lua/0.1")
            .build()
            .map_err(|err| LuaPluginError::HttpRequest {
                url: "client".to_string(),
                source: err,
            })?;
        #[cfg(feature = "async")]
        let async_client = AsyncClient::builder()
            .user_agent("weevil-lua/0.1")
            .build()
            .map_err(|err| LuaPluginError::HttpRequest {
                url: "client".to_string(),
                source: err,
            })?;
        Ok(Self {
            allowlist: Arc::new(allowlist),
            blocking,
            #[cfg(feature = "async")]
            async_client,
        })
    }

    pub fn allowlist(&self) -> &[TrustedUrl] {
        self.allowlist.as_ref()
    }

    pub fn get_blocking(
        &self,
        url: &str,
        options: &HttpRequestOptions,
    ) -> Result<String, LuaPluginError> {
        let parsed = self.ensure_trusted(url)?;
        let mut request = self.blocking.get(parsed.as_str());
        if let Some(version) = options.version {
            request = request.version(version);
        }
        if !options.headers.is_empty() {
            let header_map = build_headers(&options.headers)?;
            request = request.headers(header_map);
        }
        let response = request.send().map_err(|err| LuaPluginError::HttpRequest {
            url: parsed.to_string(),
            source: err,
        })?;
        let status = response.status();
        if !status.is_success() {
            return Err(LuaPluginError::HttpStatus {
                url: parsed.to_string(),
                status: status.as_u16(),
            });
        }
        response.text().map_err(|err| LuaPluginError::HttpRequest {
            url: parsed.to_string(),
            source: err,
        })
    }

    #[cfg(feature = "async")]
    pub async fn get_async(
        &self,
        url: &str,
        options: &HttpRequestOptions,
    ) -> Result<String, LuaPluginError> {
        let parsed = self.ensure_trusted(url)?;
        let mut request = self.async_client.get(parsed.as_str());
        if let Some(version) = options.version {
            request = request.version(version);
        }
        if !options.headers.is_empty() {
            let header_map = build_headers(&options.headers)?;
            request = request.headers(header_map);
        }
        let response = request
            .send()
            .await
            .map_err(|err| LuaPluginError::HttpRequest {
                url: parsed.to_string(),
                source: err,
            })?;
        let status = response.status();
        if !status.is_success() {
            return Err(LuaPluginError::HttpStatus {
                url: parsed.to_string(),
                status: status.as_u16(),
            });
        }
        response
            .text()
            .await
            .map_err(|err| LuaPluginError::HttpRequest {
                url: parsed.to_string(),
                source: err,
            })
    }

    fn ensure_trusted(&self, url: &str) -> Result<Url, LuaPluginError> {
        let parsed = Url::parse(url).map_err(|_| LuaPluginError::InvalidHttpUrl {
            value: url.to_string(),
        })?;
        let scheme = parsed.scheme().to_string();
        if scheme != "http" && scheme != "https" {
            return Err(LuaPluginError::HttpUrlUnsupportedScheme {
                scheme,
                value: url.to_string(),
            });
        }
        if !parsed.has_authority() {
            return Err(LuaPluginError::InvalidHttpUrl {
                value: url.to_string(),
            });
        }
        if parsed.host_str().is_none() {
            return Err(LuaPluginError::HttpUrlMissingHost {
                value: url.to_string(),
            });
        }
        if !self.allowlist.iter().any(|entry| entry.matches(&parsed)) {
            return Err(LuaPluginError::UntrustedUrl {
                url: url.to_string(),
            });
        }
        Ok(parsed)
    }
}

fn build_headers(headers: &[(String, String)]) -> Result<HeaderMap, LuaPluginError> {
    let mut map = HeaderMap::new();
    for (name, value) in headers {
        let header_name = HeaderName::from_bytes(name.as_bytes())
            .map_err(|_| LuaPluginError::HttpHeaderInvalidName { name: name.clone() })?;
        let header_value =
            HeaderValue::from_str(value).map_err(|_| LuaPluginError::HttpHeaderInvalidValue {
                name: name.clone(),
                value: value.clone(),
            })?;
        map.append(header_name, header_value);
    }
    Ok(map)
}
