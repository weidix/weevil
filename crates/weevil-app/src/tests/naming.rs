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
fn format_file_base_falls_back_to_input() {
    let movie = Movie::default();
    let base = format_file_base("{title}", &movie, "INPUT").expect("base");
    assert_eq!(base, "INPUT");
}

#[test]
fn format_folder_path_supports_multiple_segments() {
    let movie = Movie {
        title: Some("Example".to_string()),
        genre: vec!["Action".to_string()],
        ..Movie::default()
    };
    let path = format_folder_path("{genre}/{title}", &movie, "fallback").expect("path");
    let rendered = path.to_string_lossy();
    assert!(rendered.contains("Action"));
    assert!(rendered.contains("Example"));
}

#[test]
fn sanitize_component_replaces_illegal_chars() {
    let sanitized = sanitize_component("A/B:C*");
    assert_eq!(sanitized, "A_B_C_");
}
