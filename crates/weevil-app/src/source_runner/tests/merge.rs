use super::*;

#[test]
fn merge_movie_fills_empty_and_merges_collections() {
    let mut target = Movie {
        title: Some("".to_string()),
        genre: vec!["Drama".to_string()],
        tag: vec!["TagA".to_string()],
        actor: vec![Actor {
            name: Some("Alice".to_string()),
            role: None,
            gender: None,
            order: Some(1),
        }],
        fanart: Some(Fanart { thumb: vec![] }),
        ratings: Some(Ratings {
            rating: vec![Rating {
                name: Some("tmdb".to_string()),
                value: Some(7.5),
                votes: None,
                ..Rating::default()
            }],
        }),
        ..Movie::default()
    };

    let incoming = Movie {
        title: Some("Movie Title".to_string()),
        genre: vec!["Action".to_string(), "Drama".to_string()],
        tag: vec!["TagB".to_string()],
        actor: vec![Actor {
            name: Some("Alice".to_string()),
            role: Some("Lead".to_string()),
            gender: Some("female".to_string()),
            order: Some(1),
        }],
        fanart: Some(Fanart {
            thumb: vec![Thumb {
                value: Some("fanart-a.jpg".to_string()),
                ..Thumb::default()
            }],
        }),
        ratings: Some(Ratings {
            rating: vec![
                Rating {
                    name: Some("tmdb".to_string()),
                    votes: Some(100),
                    ..Rating::default()
                },
                Rating {
                    name: Some("imdb".to_string()),
                    value: Some(8.2),
                    votes: Some(200),
                    ..Rating::default()
                },
            ],
        }),
        ..Movie::default()
    };

    merge_movie(&mut target, incoming);

    assert_eq!(target.title, Some("Movie Title".to_string()));
    assert_eq!(
        target.genre,
        vec!["Drama".to_string(), "Action".to_string()]
    );
    assert_eq!(target.tag, vec!["TagA".to_string(), "TagB".to_string()]);
    assert_eq!(target.actor.len(), 1);
    assert_eq!(target.actor[0].role, Some("Lead".to_string()));
    assert_eq!(target.actor[0].gender, Some("female".to_string()));
    assert_eq!(
        target.fanart.as_ref().map(|fanart| fanart.thumb.len()),
        Some(1)
    );
    assert_eq!(
        target.ratings.as_ref().map(|ratings| ratings.rating.len()),
        Some(2)
    );
    let tmdb = target
        .ratings
        .as_ref()
        .expect("ratings")
        .rating
        .iter()
        .find(|rating| rating.name.as_deref() == Some("tmdb"))
        .expect("tmdb");
    assert_eq!(tmdb.votes, Some(100));
}

#[test]
fn merge_movie_appends_unique_fanart_thumbs() {
    let mut target = Movie {
        fanart: Some(Fanart {
            thumb: vec![Thumb {
                value: Some("fanart-a.jpg".to_string()),
                preview: Some("fanart-a-preview.jpg".to_string()),
                ..Thumb::default()
            }],
        }),
        ..Movie::default()
    };

    let incoming = Movie {
        fanart: Some(Fanart {
            thumb: vec![
                Thumb {
                    value: Some("fanart-a.jpg".to_string()),
                    preview: Some("fanart-a-new-preview.jpg".to_string()),
                    ..Thumb::default()
                },
                Thumb {
                    value: Some("fanart-b.jpg".to_string()),
                    ..Thumb::default()
                },
            ],
        }),
        ..Movie::default()
    };

    merge_movie(&mut target, incoming);

    let fanart = target.fanart.expect("fanart");
    assert_eq!(fanart.thumb.len(), 2);
    assert_eq!(fanart.thumb[0].value, Some("fanart-a.jpg".to_string()));
    assert_eq!(
        fanart.thumb[0].preview,
        Some("fanart-a-preview.jpg".to_string())
    );
    assert_eq!(fanart.thumb[1].value, Some("fanart-b.jpg".to_string()));
}

#[test]
fn merge_movie_fills_empty_primary_fanart_thumb() {
    let mut target = Movie {
        fanart: Some(Fanart {
            thumb: vec![Thumb {
                value: None,
                preview: None,
                aspect: Some("fanart".to_string()),
            }],
        }),
        ..Movie::default()
    };

    let incoming = Movie {
        fanart: Some(Fanart {
            thumb: vec![Thumb {
                value: Some("fanart-b.jpg".to_string()),
                preview: Some("fanart-b-preview.jpg".to_string()),
                ..Thumb::default()
            }],
        }),
        ..Movie::default()
    };

    merge_movie(&mut target, incoming);

    let fanart = target.fanart.expect("fanart");
    assert_eq!(fanart.thumb.len(), 1);
    assert_eq!(fanart.thumb[0].value, Some("fanart-b.jpg".to_string()));
    assert_eq!(
        fanart.thumb[0].preview,
        Some("fanart-b-preview.jpg".to_string())
    );
}

#[test]
fn merge_movie_dedupes_actor_when_order_differs() {
    let mut target = Movie {
        actor: vec![
            Actor {
                name: Some("Alice".to_string()),
                role: None,
                gender: None,
                order: Some(1),
            },
            Actor {
                name: Some("Bob".to_string()),
                role: Some("Lead".to_string()),
                gender: Some("female".to_string()),
                order: Some(2),
            },
            Actor {
                name: Some("Carol".to_string()),
                role: None,
                gender: None,
                order: Some(3),
            },
        ],
        ..Movie::default()
    };

    let incoming = Movie {
        actor: vec![
            Actor {
                name: Some("Alice".to_string()),
                role: Some("Guest".to_string()),
                gender: None,
                order: Some(1),
            },
            Actor {
                name: Some("Dave".to_string()),
                role: None,
                gender: None,
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

    merge_movie(&mut target, incoming);

    assert_eq!(target.actor.len(), 4);
    assert_eq!(target.actor[0].name.as_deref(), Some("Alice"));
    assert_eq!(target.actor[0].role.as_deref(), Some("Guest"));
    assert_eq!(target.actor[1].name.as_deref(), Some("Bob"));
    assert_eq!(target.actor[1].role.as_deref(), Some("Lead"));
    assert_eq!(target.actor[1].order, Some(2));
    assert_eq!(target.actor[2].name.as_deref(), Some("Carol"));
    assert_eq!(target.actor[2].order, Some(3));
    assert_eq!(target.actor[3].name.as_deref(), Some("Dave"));
    assert_eq!(target.actor[3].order, Some(4));
    assert_eq!(
        target
            .actor
            .iter()
            .filter(|actor| actor.name.as_deref() == Some("Bob"))
            .count(),
        1
    );
}
