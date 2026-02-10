use serde::Deserialize;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct SourcePriority {
    images: Vec<String>,
    details: Vec<String>,
}

impl SourcePriority {
    pub(crate) fn from_mode_and_shared(
        mode: Option<&SourcePriorityConfig>,
        shared: Option<&SourcePriorityConfig>,
    ) -> Self {
        let images = resolve_aliases(
            mode.and_then(SourcePriorityConfig::images),
            shared.and_then(SourcePriorityConfig::images),
        );
        let details = resolve_aliases(
            mode.and_then(SourcePriorityConfig::details),
            shared.and_then(SourcePriorityConfig::details),
        );
        Self { images, details }
    }

    pub(crate) fn images(&self) -> &[String] {
        &self.images
    }

    pub(crate) fn details(&self) -> &[String] {
        &self.details
    }

    pub(crate) fn is_configured(&self) -> bool {
        !self.images.is_empty() || !self.details.is_empty()
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "kebab-case", deny_unknown_fields)]
pub(crate) struct SourcePriorityConfig {
    images: Option<AliasList>,
    details: Option<AliasList>,
}

impl SourcePriorityConfig {
    fn images(&self) -> Option<&AliasList> {
        self.images.as_ref()
    }

    fn details(&self) -> Option<&AliasList> {
        self.details.as_ref()
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
enum AliasList {
    One(String),
    Many(Vec<String>),
}

impl AliasList {
    fn to_vec(&self) -> Vec<String> {
        match self {
            AliasList::One(value) => vec![value.clone()],
            AliasList::Many(values) => values.clone(),
        }
    }
}

fn resolve_aliases(primary: Option<&AliasList>, fallback: Option<&AliasList>) -> Vec<String> {
    let Some(raw_aliases) = primary.or(fallback).map(AliasList::to_vec) else {
        return Vec::new();
    };

    let mut resolved = Vec::new();
    for alias in raw_aliases {
        let normalized = alias.trim();
        if normalized.is_empty() {
            continue;
        }
        if !resolved.iter().any(|existing| existing == normalized) {
            resolved.push(normalized.to_string());
        }
    }
    resolved
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mode_priority_overrides_shared_per_group() {
        let shared: SourcePriorityConfig = toml::from_str(
            r#"
images = ["source.shared.a", "source.shared.b"]
details = ["source.shared.a"]
"#,
        )
        .expect("shared config");
        let mode: SourcePriorityConfig = toml::from_str(
            r#"
details = ["source.mode.x", "source.mode.y"]
"#,
        )
        .expect("mode config");

        let resolved = SourcePriority::from_mode_and_shared(Some(&mode), Some(&shared));
        assert_eq!(resolved.images(), ["source.shared.a", "source.shared.b"]);
        assert_eq!(resolved.details(), ["source.mode.x", "source.mode.y"]);
    }

    #[test]
    fn source_priority_trims_and_dedupes_aliases() {
        let mode: SourcePriorityConfig = toml::from_str(
            r#"
images = [" source.a ", "source.a", "", "source.b"]
"#,
        )
        .expect("mode config");

        let resolved = SourcePriority::from_mode_and_shared(Some(&mode), None);
        assert_eq!(resolved.images(), ["source.a", "source.b"]);
        assert!(resolved.details().is_empty());
    }
}
