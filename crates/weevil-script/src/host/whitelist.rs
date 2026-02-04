use globset::{Glob, GlobSet, GlobSetBuilder};

use super::HostError;

#[derive(Debug, Clone)]
pub(crate) struct UrlWhitelist {
    matcher: GlobSet,
}

impl UrlWhitelist {
    pub(crate) fn new(patterns: Vec<String>) -> Result<Self, HostError> {
        let mut builder = GlobSetBuilder::new();
        for pattern in &patterns {
            let glob = Glob::new(pattern).map_err(|err| {
                HostError::Policy(format!("invalid whitelist pattern {pattern}: {err}"))
            })?;
            builder.add(glob);
        }
        let matcher = builder
            .build()
            .map_err(|err| HostError::Policy(format!("build whitelist failed: {err}")))?;
        Ok(Self { matcher })
    }

    pub(crate) fn allows(&self, url: &str) -> bool {
        self.matcher.is_match(url)
    }
}

#[cfg(test)]
mod tests {
    use super::UrlWhitelist;
    use crate::host::HostError;

    #[test]
    fn whitelist_matches_globs() {
        let whitelist = UrlWhitelist::new(vec![
            "https://example.com/*".to_string(),
            "https://static.example.com/favicon.ico".to_string(),
        ])
        .expect("build whitelist");
        assert!(whitelist.allows("https://example.com/index.html"));
        assert!(whitelist.allows("https://example.com/path/inner"));
        assert!(whitelist.allows("https://static.example.com/favicon.ico"));
        assert!(!whitelist.allows("https://evil.example.com/index.html"));
    }

    #[test]
    fn whitelist_empty_denies_all() {
        let whitelist = UrlWhitelist::new(Vec::new()).expect("build empty whitelist");
        assert!(!whitelist.allows("https://example.com/index.html"));
    }

    #[test]
    fn whitelist_rejects_invalid_patterns() {
        let err = UrlWhitelist::new(vec!["https://example.com/[".to_string()])
            .expect_err("invalid pattern should fail");
        assert!(matches!(err, HostError::Policy(_)));
    }
}
