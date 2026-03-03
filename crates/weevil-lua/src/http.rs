use std::sync::Arc;

use reqwest::Version;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use url::Url;

use crate::error::LuaPluginError;
#[cfg(feature = "async")]
use reqwest::Client as AsyncClient;
#[cfg(not(feature = "async"))]
use reqwest::blocking::Client as BlockingClient;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrustedUrl {
    original: String,
    scheme: String,
    host: HostPattern,
    port: Option<u16>,
    path_pattern: String,
    path_has_wildcard: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum HostPattern {
    Exact(String),
    WildcardSingleLabel { suffix: String },
}

impl TrustedUrl {
    pub fn parse(input: &str) -> Result<Self, LuaPluginError> {
        let wildcard_host = trusted_host_wildcard(input);
        let url = parse_trusted_url(input, wildcard_host)?;
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
            })?;
        let host = match wildcard_host {
            Some(_) => {
                let Some(suffix) = host.strip_prefix("wildcard.") else {
                    return Err(LuaPluginError::InvalidTrustedUrl {
                        value: input.to_string(),
                    });
                };
                if suffix.is_empty() {
                    return Err(LuaPluginError::InvalidTrustedUrl {
                        value: input.to_string(),
                    });
                }
                HostPattern::WildcardSingleLabel {
                    suffix: suffix.to_ascii_lowercase(),
                }
            }
            None => HostPattern::Exact(host.to_ascii_lowercase()),
        };
        let path_pattern = if url.path().is_empty() {
            "/".to_string()
        } else {
            url.path().to_string()
        };
        let path_has_wildcard = path_pattern.as_bytes().contains(&b'*');
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
        let host_matches = match &self.host {
            HostPattern::Exact(expected) => host.eq_ignore_ascii_case(expected),
            HostPattern::WildcardSingleLabel { suffix } => {
                let host = host.to_ascii_lowercase();
                if !host.ends_with(suffix) {
                    return false;
                }
                let prefix_len = host.len().saturating_sub(suffix.len());
                if prefix_len < 2 {
                    return false;
                }
                let prefix = &host[..prefix_len];
                if !prefix.ends_with('.') {
                    return false;
                }
                let label = &prefix[..prefix.len() - 1];
                !label.is_empty() && !label.contains('.')
            }
        };
        if !host_matches {
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

fn parse_trusted_url(input: &str, wildcard_host: Option<&str>) -> Result<Url, LuaPluginError> {
    if wildcard_host.is_none() {
        return Url::parse(input).map_err(|_| LuaPluginError::InvalidTrustedUrl {
            value: input.to_string(),
        });
    }

    let Some((scheme, rest)) = input.split_once("://") else {
        return Err(LuaPluginError::InvalidTrustedUrl {
            value: input.to_string(),
        });
    };
    let authority_end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let authority = &rest[..authority_end];
    if !authority.starts_with("*.") || authority.contains('@') {
        return Err(LuaPluginError::InvalidTrustedUrl {
            value: input.to_string(),
        });
    }
    let normalized_authority = authority.replacen("*.", "wildcard.", 1);
    let normalized = format!(
        "{scheme}://{normalized_authority}{}",
        &rest[authority_end..]
    );
    Url::parse(&normalized).map_err(|_| LuaPluginError::InvalidTrustedUrl {
        value: input.to_string(),
    })
}

fn trusted_host_wildcard(input: &str) -> Option<&str> {
    let (_, rest) = input.split_once("://")?;
    let authority_end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let authority = &rest[..authority_end];
    authority.strip_prefix("*.")
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

    #[test]
    fn trusted_url_matches_single_label_host_wildcard() {
        let trusted = trusted_url("https://*.jdbstatic.com/");
        assert!(trusted.matches(&parsed_url("https://c0.jdbstatic.com/covers/v4/V40e3.jpg")));
        assert!(!trusted.matches(&parsed_url("https://jdbstatic.com/covers/v4/V40e3.jpg")));
        assert!(!trusted.matches(&parsed_url("https://a.b.jdbstatic.com/covers/v4/V40e3.jpg")));
    }

    #[test]
    fn trusted_url_host_wildcard_respects_port() {
        let trusted = trusted_url("https://*.example.com:8443/");
        assert!(trusted.matches(&parsed_url("https://api.example.com:8443/path")));
        assert!(!trusted.matches(&parsed_url("https://api.example.com/path")));
    }
}

#[derive(Debug, Clone)]
pub struct HttpClient {
    allowlist: Arc<Vec<TrustedUrl>>,
    #[cfg(not(feature = "async"))]
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
        #[cfg(not(feature = "async"))]
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
            #[cfg(not(feature = "async"))]
            blocking,
            #[cfg(feature = "async")]
            async_client,
        })
    }

    pub fn allowlist(&self) -> &[TrustedUrl] {
        self.allowlist.as_ref()
    }

    #[cfg(not(feature = "async"))]
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

    #[cfg(not(feature = "async"))]
    pub fn get_bytes_blocking(
        &self,
        url: &str,
        options: &HttpRequestOptions,
    ) -> Result<Vec<u8>, LuaPluginError> {
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
        let bytes = response
            .bytes()
            .map_err(|err| LuaPluginError::HttpRequest {
                url: parsed.to_string(),
                source: err,
            })?;
        Ok(bytes.to_vec())
    }

    #[cfg(not(feature = "async"))]
    pub fn post_blocking(
        &self,
        url: &str,
        body: &str,
        options: &HttpRequestOptions,
    ) -> Result<String, LuaPluginError> {
        let parsed = self.ensure_trusted(url)?;
        let mut request = self.blocking.post(parsed.as_str()).body(body.to_string());
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

    #[cfg(feature = "async")]
    pub async fn get_bytes_async(
        &self,
        url: &str,
        options: &HttpRequestOptions,
    ) -> Result<Vec<u8>, LuaPluginError> {
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
        let bytes = response
            .bytes()
            .await
            .map_err(|err| LuaPluginError::HttpRequest {
                url: parsed.to_string(),
                source: err,
            })?;
        Ok(bytes.to_vec())
    }

    #[cfg(feature = "async")]
    pub async fn post_async(
        &self,
        url: &str,
        body: &str,
        options: &HttpRequestOptions,
    ) -> Result<String, LuaPluginError> {
        let parsed = self.ensure_trusted(url)?;
        let mut request = self
            .async_client
            .post(parsed.as_str())
            .body(body.to_string());
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
