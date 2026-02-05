use std::path::PathBuf;

use super::*;

#[test]
fn render_template_replaces_fields() {
    let movie = Movie {
        title: Some("Spirited Away".to_string()),
        year: Some(2001),
        ..Movie::default()
    };
    let rendered = render_template("{title} ({year})", &movie).expect("template");
    assert_eq!(rendered, "Spirited Away (2001)");
}

#[test]
fn render_template_unknown_field_is_error() {
    let movie = Movie::default();
    let error = render_template("{missing}", &movie).expect_err("error");
    assert!(matches!(error, AppError::TemplateUnknownField { .. }));
}

#[test]
fn format_output_paths_supports_multiple_segments() {
    let movie = Movie {
        title: Some("Example".to_string()),
        genre: vec!["Action".to_string()],
        ..Movie::default()
    };
    let paths = format_output_paths("{genre}/{title}/{title}", &movie, "fallback").expect("paths");
    assert_eq!(
        paths,
        vec![PathBuf::from("Action").join("Example").join("Example")]
    );
}

#[test]
fn format_output_paths_expands_actor_fields() {
    let movie = Movie {
        title: Some("Example".to_string()),
        actor: vec![
            Actor {
                name: Some("Alice".to_string()),
                ..Actor::default()
            },
            Actor {
                name: Some("Bob".to_string()),
                ..Actor::default()
            },
        ],
        ..Movie::default()
    };
    let paths = format_output_paths("{actor}/{title}", &movie, "fallback").expect("paths");
    assert_eq!(
        paths,
        vec![
            PathBuf::from("Alice").join("Example"),
            PathBuf::from("Bob").join("Example")
        ]
    );
}

#[test]
fn format_output_paths_expands_genre_list() {
    let movie = Movie {
        title: Some("Example".to_string()),
        genre: vec!["Action".to_string(), "Drama".to_string()],
        ..Movie::default()
    };
    let paths = format_output_paths("{genre}/{title}", &movie, "fallback").expect("paths");
    assert_eq!(
        paths,
        vec![
            PathBuf::from("Action").join("Example"),
            PathBuf::from("Drama").join("Example")
        ]
    );
}

#[test]
fn format_output_paths_falls_back_when_actor_filters_match_none() {
    let movie = Movie {
        actor: vec![Actor {
            name: Some("Alice".to_string()),
            gender: Some("female".to_string()),
            ..Actor::default()
        }],
        ..Movie::default()
    };
    let paths = format_output_paths("{actor[gender=male]}", &movie, "Fallback").expect("paths");
    assert_eq!(paths, vec![PathBuf::from("Fallback")]);
}

#[test]
fn format_output_paths_dedupes_when_template_has_no_actor_fields() {
    let movie = Movie {
        title: Some("Example".to_string()),
        actor: vec![
            Actor {
                name: Some("Alice".to_string()),
                ..Actor::default()
            },
            Actor {
                name: Some("Bob".to_string()),
                ..Actor::default()
            },
        ],
        ..Movie::default()
    };
    let paths = format_output_paths("{title}", &movie, "fallback").expect("paths");
    assert_eq!(paths, vec![PathBuf::from("Example")]);
}

#[test]
fn format_output_paths_uses_template_without_actors() {
    let movie = Movie {
        title: Some("Example".to_string()),
        ..Movie::default()
    };
    let paths = format_output_paths("{title}", &movie, "fallback").expect("paths");
    assert_eq!(paths, vec![PathBuf::from("Example")]);
}

#[test]
fn sanitize_component_replaces_illegal_chars() {
    let sanitized = sanitize_component("A/B:C*");
    assert_eq!(sanitized, "A_B_C_");
}

#[test]
fn render_template_actor_gender() {
    let movie = Movie {
        actor: vec![
            Actor {
                name: Some("Alice".to_string()),
                gender: Some("female".to_string()),
                ..Actor::default()
            },
            Actor {
                name: Some("Bob".to_string()),
                gender: Some("male".to_string()),
                ..Actor::default()
            },
        ],
        ..Movie::default()
    };
    let rendered = render_template("{actor.gender}", &movie).expect("template");
    assert_eq!(rendered, "female, male");
}

#[test]
fn render_template_actor_filters_by_gender() {
    let movie = Movie {
        actor: vec![
            Actor {
                name: Some("Alice".to_string()),
                gender: Some("female".to_string()),
                ..Actor::default()
            },
            Actor {
                name: Some("Bob".to_string()),
                gender: Some("male".to_string()),
                ..Actor::default()
            },
        ],
        ..Movie::default()
    };
    let rendered = render_template("{actor[gender=female]}", &movie).expect("template");
    assert_eq!(rendered, "Alice");
}

#[test]
fn render_template_actor_role_and_order() {
    let movie = Movie {
        actor: vec![
            Actor {
                name: Some("Alice".to_string()),
                role: Some("Lead".to_string()),
                order: Some(1),
                ..Actor::default()
            },
            Actor {
                name: Some("Bob".to_string()),
                role: Some("Support".to_string()),
                order: Some(2),
                ..Actor::default()
            },
        ],
        ..Movie::default()
    };
    let roles = render_template("{actor.role}", &movie).expect("template");
    assert_eq!(roles, "Lead, Support");
    let orders = render_template("{actor.order}", &movie).expect("template");
    assert_eq!(orders, "1, 2");
    let filtered = render_template("{actor[order=2]}", &movie).expect("template");
    assert_eq!(filtered, "Bob");
}

#[test]
fn format_input_name_removes_tokens_and_collapses_whitespace() {
    let formatted = format_input_name(
        "Movie 1080p   WEB-DL",
        &vec!["1080p".to_string(), "WEB-DL".to_string()],
    )
    .expect("formatted");
    assert_eq!(formatted, "Movie");
}

#[test]
fn format_input_name_empty_is_error() {
    let error = format_input_name("1080p", &vec!["1080p".to_string()]).expect_err("expected error");
    assert!(matches!(error, AppError::InputNameFormatEmpty { .. }));
}
