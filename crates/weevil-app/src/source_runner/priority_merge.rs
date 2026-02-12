use crate::nfo::Movie;
use crate::source_priority::SourcePriority;

use super::{merge_movie, merge_movie_details, merge_movie_images};

#[derive(Debug, Clone)]
pub(crate) struct MergeSource {
    pub(crate) alias: String,
    pub(crate) movie: Movie,
}

pub(crate) fn merge_sources_movie(
    sources: &[MergeSource],
    source_priority: &SourcePriority,
    merge_group_fallback: bool,
) -> Movie {
    if sources.is_empty() {
        return Movie::default();
    }

    if !source_priority.is_configured() {
        return merge_by_default_order(sources);
    }

    let detail_sources = order_sources_for_group(sources, source_priority.details());
    let image_sources = order_sources_for_group(sources, source_priority.images());

    let mut merged = Movie::default();
    if merge_group_fallback {
        for source in detail_sources {
            merge_movie_details(&mut merged, &source.movie);
        }
    } else {
        if let Some(source) = detail_sources.first() {
            merge_movie_details(&mut merged, &source.movie);
        }
    }

    if let Some(source) = select_image_source(&image_sources) {
        merge_movie_images(&mut merged, &source.movie);
    }

    merged
}

fn merge_by_default_order(sources: &[MergeSource]) -> Movie {
    if sources.len() == 1 {
        let mut merged = Movie::default();
        merge_movie(&mut merged, sources[0].movie.clone());
        return merged;
    }

    let mut merged = Movie::default();
    for source in sources {
        merge_movie_details(&mut merged, &source.movie);
    }

    let ordered_sources = sources.iter().collect::<Vec<_>>();
    if let Some(source) = select_image_source(&ordered_sources) {
        merge_movie_images(&mut merged, &source.movie);
    }

    merged
}

fn order_sources_for_group<'a>(
    sources: &'a [MergeSource],
    priorities: &[String],
) -> Vec<&'a MergeSource> {
    if priorities.is_empty() {
        return sources.iter().collect();
    }

    let mut ordered = Vec::with_capacity(priorities.len());

    for alias in priorities {
        for source in sources {
            if source.alias == *alias {
                ordered.push(source);
                break;
            }
        }
    }

    ordered
}

fn select_image_source<'a>(sources: &[&'a MergeSource]) -> Option<&'a MergeSource> {
    for source in sources {
        if movie_has_image_data(&source.movie) {
            return Some(source);
        }
    }
    sources.first().copied()
}

fn movie_has_image_data(movie: &Movie) -> bool {
    if movie.thumb.as_ref().is_some_and(thumb_has_value) {
        return true;
    }
    if let Some(fanart) = movie.fanart.as_ref() {
        return fanart.thumb.iter().any(thumb_has_value);
    }
    false
}

fn thumb_has_value(thumb: &crate::nfo::Thumb) -> bool {
    value_has_content(thumb.value.as_deref())
        || value_has_content(thumb.preview.as_deref())
        || value_has_content(thumb.aspect.as_deref())
}

fn value_has_content(value: Option<&str>) -> bool {
    value.is_some_and(|text| !text.trim().is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nfo::{Fanart, Thumb};

    #[test]
    fn merge_sources_movie_uses_priority_per_group() {
        let sources = vec![
            MergeSource {
                alias: "source.a".to_string(),
                movie: Movie {
                    title: Some("title-a".to_string()),
                    runtime: Some(110),
                    thumb: Some(Thumb {
                        value: Some("thumb-a.jpg".to_string()),
                        ..Thumb::default()
                    }),
                    ..Movie::default()
                },
            },
            MergeSource {
                alias: "source.b".to_string(),
                movie: Movie {
                    title: Some("title-b".to_string()),
                    runtime: Some(120),
                    thumb: Some(Thumb {
                        value: Some("thumb-b.jpg".to_string()),
                        ..Thumb::default()
                    }),
                    ..Movie::default()
                },
            },
            MergeSource {
                alias: "source.c".to_string(),
                movie: Movie {
                    title: Some("title-c".to_string()),
                    runtime: Some(130),
                    thumb: Some(Thumb {
                        value: Some("thumb-c.jpg".to_string()),
                        ..Thumb::default()
                    }),
                    ..Movie::default()
                },
            },
        ];

        let mode: crate::source_priority::SourcePriorityConfig = toml::from_str(
            r#"
details = ["source.b"]
images = ["source.c", "source.b"]
"#,
        )
        .expect("priority config");
        let priority = SourcePriority::from_mode_and_shared(Some(&mode), None);

        let merged = merge_sources_movie(&sources, &priority, true);
        assert_eq!(merged.title.as_deref(), Some("title-b"));
        assert_eq!(merged.runtime, Some(120));
        assert_eq!(
            merged
                .thumb
                .as_ref()
                .and_then(|thumb| thumb.value.as_deref()),
            Some("thumb-c.jpg")
        );
    }

    #[test]
    fn merge_sources_movie_keeps_default_order_without_priority() {
        let sources = vec![
            MergeSource {
                alias: "source.a".to_string(),
                movie: Movie {
                    title: Some("title-a".to_string()),
                    thumb: Some(Thumb {
                        value: Some("thumb-a.jpg".to_string()),
                        ..Thumb::default()
                    }),
                    ..Movie::default()
                },
            },
            MergeSource {
                alias: "source.b".to_string(),
                movie: Movie {
                    title: Some("title-b".to_string()),
                    thumb: Some(Thumb {
                        value: Some("thumb-b.jpg".to_string()),
                        ..Thumb::default()
                    }),
                    ..Movie::default()
                },
            },
        ];

        let merged = merge_sources_movie(&sources, &SourcePriority::default(), true);
        assert_eq!(merged.title.as_deref(), Some("title-a"));
        assert_eq!(
            merged
                .thumb
                .as_ref()
                .and_then(|thumb| thumb.value.as_deref()),
            Some("thumb-a.jpg")
        );
    }

    #[test]
    fn merge_sources_movie_without_priority_does_not_merge_images_across_sources() {
        let sources = vec![
            MergeSource {
                alias: "source.a".to_string(),
                movie: Movie {
                    fanart: Some(Fanart {
                        thumb: vec![Thumb {
                            value: Some("fanart-a-1.jpg".to_string()),
                            ..Thumb::default()
                        }],
                    }),
                    ..Movie::default()
                },
            },
            MergeSource {
                alias: "source.b".to_string(),
                movie: Movie {
                    fanart: Some(Fanart {
                        thumb: vec![Thumb {
                            value: Some("fanart-b-1.jpg".to_string()),
                            ..Thumb::default()
                        }],
                    }),
                    ..Movie::default()
                },
            },
        ];

        let merged = merge_sources_movie(&sources, &SourcePriority::default(), true);
        let fanart = merged.fanart.expect("fanart");
        assert_eq!(fanart.thumb.len(), 1);
        assert_eq!(fanart.thumb[0].value.as_deref(), Some("fanart-a-1.jpg"));
    }

    #[test]
    fn merge_sources_movie_without_priority_falls_back_to_next_image_source() {
        let sources = vec![
            MergeSource {
                alias: "source.a".to_string(),
                movie: Movie::default(),
            },
            MergeSource {
                alias: "source.b".to_string(),
                movie: Movie {
                    thumb: Some(Thumb {
                        value: Some("thumb-b.jpg".to_string()),
                        ..Thumb::default()
                    }),
                    ..Movie::default()
                },
            },
        ];

        let merged = merge_sources_movie(&sources, &SourcePriority::default(), true);
        assert_eq!(
            merged
                .thumb
                .as_ref()
                .and_then(|thumb| thumb.value.as_deref()),
            Some("thumb-b.jpg")
        );
    }

    #[test]
    fn merge_sources_movie_applies_priority_even_without_multi_source_flag() {
        let sources = vec![
            MergeSource {
                alias: "source.a".to_string(),
                movie: Movie {
                    title: Some("title-a".to_string()),
                    runtime: Some(110),
                    thumb: Some(Thumb {
                        value: Some("thumb-a.jpg".to_string()),
                        ..Thumb::default()
                    }),
                    ..Movie::default()
                },
            },
            MergeSource {
                alias: "source.b".to_string(),
                movie: Movie {
                    title: Some("title-b".to_string()),
                    runtime: Some(120),
                    thumb: Some(Thumb {
                        value: Some("thumb-b.jpg".to_string()),
                        ..Thumb::default()
                    }),
                    ..Movie::default()
                },
            },
            MergeSource {
                alias: "source.c".to_string(),
                movie: Movie {
                    title: Some("title-c".to_string()),
                    runtime: Some(130),
                    thumb: Some(Thumb {
                        value: Some("thumb-c.jpg".to_string()),
                        ..Thumb::default()
                    }),
                    ..Movie::default()
                },
            },
        ];

        let mode: crate::source_priority::SourcePriorityConfig = toml::from_str(
            r#"
details = ["source.b"]
images = ["source.c"]
"#,
        )
        .expect("priority config");
        let priority = SourcePriority::from_mode_and_shared(Some(&mode), None);

        let merged = merge_sources_movie(&sources, &priority, true);
        assert_eq!(merged.title.as_deref(), Some("title-b"));
        assert_eq!(merged.runtime, Some(120));
        assert_eq!(
            merged
                .thumb
                .as_ref()
                .and_then(|thumb| thumb.value.as_deref()),
            Some("thumb-c.jpg")
        );
    }

    #[test]
    fn merge_sources_movie_priority_without_multi_source_uses_first_per_group() {
        let sources = vec![
            MergeSource {
                alias: "source.a".to_string(),
                movie: Movie {
                    title: Some("title-a".to_string()),
                    runtime: Some(100),
                    thumb: Some(Thumb {
                        value: Some("thumb-a.jpg".to_string()),
                        ..Thumb::default()
                    }),
                    ..Movie::default()
                },
            },
            MergeSource {
                alias: "source.b".to_string(),
                movie: Movie {
                    title: Some("title-b".to_string()),
                    runtime: None,
                    thumb: Some(Thumb {
                        value: Some("thumb-b.jpg".to_string()),
                        ..Thumb::default()
                    }),
                    ..Movie::default()
                },
            },
            MergeSource {
                alias: "source.c".to_string(),
                movie: Movie {
                    title: Some("title-c".to_string()),
                    runtime: Some(130),
                    thumb: Some(Thumb {
                        value: Some("thumb-c.jpg".to_string()),
                        ..Thumb::default()
                    }),
                    ..Movie::default()
                },
            },
        ];

        let mode: crate::source_priority::SourcePriorityConfig = toml::from_str(
            r#"
details = ["source.b", "source.c"]
images = ["source.c", "source.b"]
"#,
        )
        .expect("priority config");
        let priority = SourcePriority::from_mode_and_shared(Some(&mode), None);

        let merged = merge_sources_movie(&sources, &priority, false);
        assert_eq!(merged.title.as_deref(), Some("title-b"));
        assert_eq!(merged.runtime, None);
        assert_eq!(
            merged
                .thumb
                .as_ref()
                .and_then(|thumb| thumb.value.as_deref()),
            Some("thumb-c.jpg")
        );
    }

    #[test]
    fn merge_sources_movie_configured_group_is_strict_to_listed_aliases() {
        let sources = vec![
            MergeSource {
                alias: "source.a".to_string(),
                movie: Movie {
                    runtime: Some(100),
                    ..Movie::default()
                },
            },
            MergeSource {
                alias: "source.b".to_string(),
                movie: Movie {
                    runtime: None,
                    ..Movie::default()
                },
            },
            MergeSource {
                alias: "source.c".to_string(),
                movie: Movie {
                    runtime: Some(130),
                    ..Movie::default()
                },
            },
        ];

        let mode: crate::source_priority::SourcePriorityConfig = toml::from_str(
            r#"
details = ["source.b"]
"#,
        )
        .expect("priority config");
        let priority = SourcePriority::from_mode_and_shared(Some(&mode), None);

        let merged = merge_sources_movie(&sources, &priority, true);
        assert_eq!(merged.runtime, None);
    }

    #[test]
    fn merge_sources_movie_empty_group_uses_script_order() {
        let sources = vec![
            MergeSource {
                alias: "source.a".to_string(),
                movie: Movie {
                    runtime: Some(100),
                    thumb: Some(Thumb {
                        value: Some("thumb-a.jpg".to_string()),
                        ..Thumb::default()
                    }),
                    ..Movie::default()
                },
            },
            MergeSource {
                alias: "source.b".to_string(),
                movie: Movie {
                    runtime: Some(120),
                    thumb: Some(Thumb {
                        value: Some("thumb-b.jpg".to_string()),
                        ..Thumb::default()
                    }),
                    ..Movie::default()
                },
            },
        ];

        let mode: crate::source_priority::SourcePriorityConfig = toml::from_str(
            r#"
images = ["source.b"]
"#,
        )
        .expect("priority config");
        let priority = SourcePriority::from_mode_and_shared(Some(&mode), None);

        let merged = merge_sources_movie(&sources, &priority, true);
        assert_eq!(merged.runtime, Some(100));
        assert_eq!(
            merged
                .thumb
                .as_ref()
                .and_then(|thumb| thumb.value.as_deref()),
            Some("thumb-b.jpg")
        );
    }

    #[test]
    fn merge_sources_movie_images_uses_single_source_even_with_multi_source_enabled() {
        let sources = vec![
            MergeSource {
                alias: "source.a".to_string(),
                movie: Movie {
                    fanart: Some(Fanart {
                        thumb: vec![Thumb {
                            value: Some("fanart-a-1.jpg".to_string()),
                            ..Thumb::default()
                        }],
                    }),
                    ..Movie::default()
                },
            },
            MergeSource {
                alias: "source.b".to_string(),
                movie: Movie {
                    fanart: Some(Fanart {
                        thumb: vec![Thumb {
                            value: Some("fanart-b-1.jpg".to_string()),
                            ..Thumb::default()
                        }],
                    }),
                    ..Movie::default()
                },
            },
        ];

        let mode: crate::source_priority::SourcePriorityConfig = toml::from_str(
            r#"
details = ["source.a", "source.b"]
images = ["source.a", "source.b"]
"#,
        )
        .expect("priority config");
        let priority = SourcePriority::from_mode_and_shared(Some(&mode), None);

        let merged = merge_sources_movie(&sources, &priority, true);
        let fanart = merged.fanart.expect("fanart");
        assert_eq!(fanart.thumb.len(), 1);
        assert_eq!(fanart.thumb[0].value.as_deref(), Some("fanart-a-1.jpg"));
    }

    #[test]
    fn merge_sources_movie_images_fallbacks_to_next_source_with_image_data() {
        let sources = vec![
            MergeSource {
                alias: "source.a".to_string(),
                movie: Movie::default(),
            },
            MergeSource {
                alias: "source.b".to_string(),
                movie: Movie {
                    thumb: Some(Thumb {
                        value: Some("thumb-b.jpg".to_string()),
                        ..Thumb::default()
                    }),
                    ..Movie::default()
                },
            },
        ];

        let mode: crate::source_priority::SourcePriorityConfig = toml::from_str(
            r#"
images = ["source.a", "source.b"]
"#,
        )
        .expect("priority config");
        let priority = SourcePriority::from_mode_and_shared(Some(&mode), None);

        let merged = merge_sources_movie(&sources, &priority, true);
        assert_eq!(
            merged
                .thumb
                .as_ref()
                .and_then(|thumb| thumb.value.as_deref()),
            Some("thumb-b.jpg")
        );
    }
}
