use super::*;

#[test]
fn parse_csv_accepts_header_and_comments() {
    let mapper = NodeValueMapper::from_csv(
        r#"
# comment line
node,to,from1,from2
genre,GenreA,from_a,from_b
tag,TagA,tag_a,tag_b
actor,ActorA,actor_a,actor_b
"#,
    )
    .expect("mapper");

    assert!(mapper.has_rules());
    assert_eq!(mapper.map_value("genre", "from_a"), "GenreA");
    assert_eq!(mapper.map_value("genre", "from_b"), "GenreA");
    assert_eq!(mapper.map_value("tag", "tag_a"), "TagA");
    assert_eq!(mapper.map_value("tag", "tag_b"), "TagA");
    assert_eq!(mapper.map_value("actor", "actor_a"), "ActorA");
    assert_eq!(mapper.map_value("actor", "actor_b"), "ActorA");
}

#[test]
fn parse_csv_rejects_invalid_column_count() {
    let error =
        NodeValueMapper::from_csv("genre,action").expect_err("should reject invalid column count");
    assert!(error.contains("expected at least 3 columns"));
}

#[test]
fn apply_movie_maps_and_dedupes_genre_and_tag() {
    let mapper = NodeValueMapper::from_csv(
        r#"
genre,GenreA,from_a,from_b
tag,TagA,tag_a,tag_b
"#,
    )
    .expect("mapper");

    let mut movie = Movie {
        genre: vec![
            "from_a".to_string(),
            "from_b".to_string(),
            "from_c".to_string(),
        ],
        tag: vec![
            "tag_a".to_string(),
            "tag_b".to_string(),
            "tag_c".to_string(),
        ],
        ..Movie::default()
    };

    mapper.apply_movie(&mut movie);

    assert_eq!(
        movie.genre,
        vec!["GenreA".to_string(), "from_c".to_string()]
    );
    assert_eq!(movie.tag, vec!["TagA".to_string(), "tag_c".to_string()]);
}

#[test]
fn apply_movie_dedupes_actor_by_mapped_name() {
    let mapper = NodeValueMapper::from_csv(
        r#"
actor,ActorA,actor_a,actor_b
actor.role,RoleA,role_a
"#,
    )
    .expect("mapper");

    let mut movie = Movie {
        actor: vec![
            Actor {
                name: Some("actor_a".to_string()),
                role: Some("role_a".to_string()),
                gender: None,
                order: Some(1),
            },
            Actor {
                name: Some("actor_b".to_string()),
                role: None,
                gender: Some("gender_a".to_string()),
                order: Some(2),
            },
            Actor {
                name: Some("actor_c".to_string()),
                role: None,
                gender: None,
                order: Some(3),
            },
        ],
        ..Movie::default()
    };

    mapper.apply_movie(&mut movie);

    assert_eq!(movie.actor.len(), 2);
    assert_eq!(movie.actor[0].name.as_deref(), Some("ActorA"));
    assert_eq!(movie.actor[0].role.as_deref(), Some("RoleA"));
    assert_eq!(movie.actor[0].gender.as_deref(), Some("gender_a"));
    assert_eq!(movie.actor[0].order, Some(1));
    assert_eq!(movie.actor[1].name.as_deref(), Some("actor_c"));
    assert_eq!(movie.actor[1].order, Some(2));
}

#[test]
fn apply_movie_maps_set_fields() {
    let mapper = NodeValueMapper::from_csv(
        r#"
set.name,SetA,set_a
set.overview,SetOverviewA,set_overview_a
"#,
    )
    .expect("mapper");

    let mut movie = Movie {
        set_info: Some(crate::nfo::SetInfo {
            name: Some("set_a".to_string()),
            overview: Some("set_overview_a".to_string()),
        }),
        ..Movie::default()
    };

    mapper.apply_movie(&mut movie);

    let set = movie.set_info.expect("set");
    assert_eq!(set.name.as_deref(), Some("SetA"));
    assert_eq!(set.overview.as_deref(), Some("SetOverviewA"));
}

#[test]
fn parse_csv_accepts_legacy_header_order() {
    let mapper = NodeValueMapper::from_csv(
        r#"
node,from,to
genre,from_a,GenreA
"#,
    )
    .expect("mapper");

    assert_eq!(mapper.map_value("genre", "from_a"), "GenreA");
}

#[test]
fn parse_csv_treats_pipe_as_literal() {
    let mapper = NodeValueMapper::from_csv(
        r#"
node,to,from1
genre,GenrePipe,from_a|from_b
"#,
    )
    .expect("mapper");

    assert_eq!(mapper.map_value("genre", "from_a|from_b"), "GenrePipe");
}
