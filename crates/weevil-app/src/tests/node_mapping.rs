use super::*;

#[test]
fn parse_csv_accepts_header_and_comments() {
    let mapper = NodeValueMapper::from_csv(
        r#"
# comment line
node,from,to
genre,剧情,Drama
tag,中字,Chinese Subtitle
"actor","Alice A.",Alice
"#,
    )
    .expect("mapper");

    assert!(mapper.has_rules());
    assert_eq!(mapper.map_value("genre", "剧情"), "Drama");
    assert_eq!(mapper.map_value("tag", "中字"), "Chinese Subtitle");
    assert_eq!(mapper.map_value("actor", "Alice A."), "Alice");
}

#[test]
fn parse_csv_rejects_invalid_column_count() {
    let error =
        NodeValueMapper::from_csv("genre,action").expect_err("should reject invalid column count");
    assert!(error.contains("expected 3 columns"));
}

#[test]
fn apply_movie_maps_and_dedupes_genre_and_tag() {
    let mapper = NodeValueMapper::from_csv(
        r#"
genre,剧情,Drama
genre,drama,Drama
tag,中文字幕,Chinese Subtitle
tag,中字,Chinese Subtitle
"#,
    )
    .expect("mapper");

    let mut movie = Movie {
        genre: vec![
            "剧情".to_string(),
            "drama".to_string(),
            "Action".to_string(),
        ],
        tag: vec![
            "中文字幕".to_string(),
            "中字".to_string(),
            "Uncut".to_string(),
        ],
        ..Movie::default()
    };

    mapper.apply_movie(&mut movie);

    assert_eq!(movie.genre, vec!["Drama".to_string(), "Action".to_string()]);
    assert_eq!(
        movie.tag,
        vec!["Chinese Subtitle".to_string(), "Uncut".to_string()]
    );
}

#[test]
fn apply_movie_dedupes_actor_by_mapped_name() {
    let mapper = NodeValueMapper::from_csv(
        r#"
actor,爱丽丝,Alice
actor,艾丽丝,Alice
actor.role,主演,Lead
"#,
    )
    .expect("mapper");

    let mut movie = Movie {
        actor: vec![
            Actor {
                name: Some("爱丽丝".to_string()),
                role: Some("主演".to_string()),
                gender: None,
                order: Some(1),
            },
            Actor {
                name: Some("艾丽丝".to_string()),
                role: None,
                gender: Some("female".to_string()),
                order: Some(2),
            },
            Actor {
                name: Some("Bob".to_string()),
                role: None,
                gender: None,
                order: Some(3),
            },
        ],
        ..Movie::default()
    };

    mapper.apply_movie(&mut movie);

    assert_eq!(movie.actor.len(), 2);
    assert_eq!(movie.actor[0].name.as_deref(), Some("Alice"));
    assert_eq!(movie.actor[0].role.as_deref(), Some("Lead"));
    assert_eq!(movie.actor[0].gender.as_deref(), Some("female"));
    assert_eq!(movie.actor[0].order, Some(1));
    assert_eq!(movie.actor[1].name.as_deref(), Some("Bob"));
    assert_eq!(movie.actor[1].order, Some(2));
}

#[test]
fn apply_movie_maps_set_fields() {
    let mapper = NodeValueMapper::from_csv(
        r#"
set.name,合集A,Collection A
set.overview,说明A,Overview A
"#,
    )
    .expect("mapper");

    let mut movie = Movie {
        set_info: Some(crate::nfo::SetInfo {
            name: Some("合集A".to_string()),
            overview: Some("说明A".to_string()),
        }),
        ..Movie::default()
    };

    mapper.apply_movie(&mut movie);

    let set = movie.set_info.expect("set");
    assert_eq!(set.name.as_deref(), Some("Collection A"));
    assert_eq!(set.overview.as_deref(), Some("Overview A"));
}
