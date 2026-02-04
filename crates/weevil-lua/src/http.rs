use std::sync::Arc;

use reqwest::Client as AsyncClient;
use reqwest::blocking::Client as BlockingClient;
use url::Url;

use crate::error::LuaPluginError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrustedUrl {
    original: String,
    scheme: String,
    host: String,
    port: Option<u16>,
    path_prefix: String,
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
        let path_prefix = if url.path().is_empty() {
            "/".to_string()
        } else {
            url.path().to_string()
        };
        Ok(Self {
            original: input.to_string(),
            scheme: url.scheme().to_string(),
            host,
            port: url.port(),
            path_prefix,
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
        url.path().starts_with(&self.path_prefix)
    }
}

#[derive(Debug, Clone)]
pub struct HttpClient {
    allowlist: Arc<Vec<TrustedUrl>>,
    blocking: BlockingClient,
    #[cfg(feature = "async")]
    async_client: AsyncClient,
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

    pub fn get_blocking(&self, url: &str) -> Result<String, LuaPluginError> {
        let parsed = self.ensure_trusted(url)?;
        let response = self.blocking.get(parsed.as_str()).send().map_err(|err| {
            LuaPluginError::HttpRequest {
                url: parsed.to_string(),
                source: err,
            }
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
    pub async fn get_async(&self, url: &str) -> Result<String, LuaPluginError> {
        let parsed = self.ensure_trusted(url)?;
        let response = self
            .async_client
            .get(parsed.as_str())
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
