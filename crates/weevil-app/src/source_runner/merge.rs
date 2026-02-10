use crate::nfo::{Actor, Fanart, Movie, Rating, Ratings, SetInfo, Thumb, UniqueId};

pub(crate) fn merge_movie(target: &mut Movie, incoming: Movie) {
    merge_movie_details(target, &incoming);
    merge_movie_images(target, &incoming);
}

pub(crate) fn merge_movie_details(target: &mut Movie, incoming: &Movie) {
    merge_option_string(&mut target.title, incoming.title.clone());
    merge_option_string(&mut target.originaltitle, incoming.originaltitle.clone());
    merge_option_string(&mut target.sorttitle, incoming.sorttitle.clone());
    merge_option_copy(&mut target.year, incoming.year);
    merge_option_string(&mut target.premiered, incoming.premiered.clone());
    merge_option_copy(&mut target.runtime, incoming.runtime);
    merge_option_string(&mut target.director, incoming.director.clone());
    merge_string_list(&mut target.credits, incoming.credits.clone());
    merge_string_list(&mut target.genre, incoming.genre.clone());
    merge_string_list(&mut target.tag, incoming.tag.clone());
    merge_option_string(&mut target.plot, incoming.plot.clone());
    merge_option_string(&mut target.outline, incoming.outline.clone());
    merge_option_string(&mut target.tagline, incoming.tagline.clone());
    merge_ratings(&mut target.ratings, incoming.ratings.clone());
    merge_option_copy(&mut target.userrating, incoming.userrating);
    merge_unique_ids(&mut target.uniqueid, incoming.uniqueid.clone());
    merge_option_string(&mut target.studio, incoming.studio.clone());
    merge_string_list(&mut target.country, incoming.country.clone());
    merge_set_info(&mut target.set_info, incoming.set_info.clone());
    merge_actors(&mut target.actor, incoming.actor.clone());
    merge_option_string(&mut target.trailer, incoming.trailer.clone());
    merge_option_string(&mut target.fileinfo, incoming.fileinfo.clone());
    merge_option_string(&mut target.dateadded, incoming.dateadded.clone());
}

pub(crate) fn merge_movie_images(target: &mut Movie, incoming: &Movie) {
    merge_thumb(&mut target.thumb, incoming.thumb.clone());
    merge_fanart(&mut target.fanart, incoming.fanart.clone());
}

fn merge_ratings(target: &mut Option<Ratings>, incoming: Option<Ratings>) {
    match (target.as_mut(), incoming) {
        (None, Some(value)) => {
            *target = Some(value);
        }
        (Some(_), None) | (None, None) => {}
        (Some(current), Some(source)) => {
            for rating in source.rating {
                if let Some(existing) = find_rating_mut(&mut current.rating, &rating) {
                    merge_option_string(&mut existing.name, rating.name);
                    merge_option_copy(&mut existing.max, rating.max);
                    merge_option_copy(&mut existing.is_default, rating.is_default);
                    merge_option_copy(&mut existing.value, rating.value);
                    merge_option_copy(&mut existing.votes, rating.votes);
                } else {
                    current.rating.push(rating);
                }
            }
        }
    }
}

fn merge_thumb(target: &mut Option<Thumb>, incoming: Option<Thumb>) {
    match (target.as_mut(), incoming) {
        (None, Some(value)) => {
            *target = Some(value);
        }
        (Some(_), None) | (None, None) => {}
        (Some(current), Some(source)) => {
            merge_option_string(&mut current.aspect, source.aspect);
            merge_option_string(&mut current.preview, source.preview);
            merge_option_string(&mut current.value, source.value);
        }
    }
}

fn merge_fanart(target: &mut Option<Fanart>, incoming: Option<Fanart>) {
    let incoming_thumb = incoming.and_then(|fanart| select_one_fanart_thumb(fanart.thumb));

    match target.as_mut() {
        None => {
            if let Some(thumb) = incoming_thumb {
                *target = Some(Fanart { thumb: vec![thumb] });
            }
        }
        Some(current) => {
            if current.thumb.len() > 1 {
                current.thumb.truncate(1);
            }

            if current.thumb.is_empty() {
                if let Some(thumb) = incoming_thumb {
                    current.thumb.push(thumb);
                }
                return;
            }

            if let Some(source_thumb) = incoming_thumb {
                merge_option_string(&mut current.thumb[0].aspect, source_thumb.aspect);
                merge_option_string(&mut current.thumb[0].preview, source_thumb.preview);
                merge_option_string(&mut current.thumb[0].value, source_thumb.value);
            }
        }
    }
}

fn merge_set_info(target: &mut Option<SetInfo>, incoming: Option<SetInfo>) {
    match (target.as_mut(), incoming) {
        (None, Some(value)) => {
            *target = Some(value);
        }
        (Some(_), None) | (None, None) => {}
        (Some(current), Some(source)) => {
            merge_option_string(&mut current.name, source.name);
            merge_option_string(&mut current.overview, source.overview);
        }
    }
}

fn merge_actors(target: &mut Vec<Actor>, incoming: Vec<Actor>) {
    for actor in incoming {
        if let Some(existing) = find_actor_mut(target, &actor) {
            merge_option_string(&mut existing.role, actor.role);
            merge_option_string(&mut existing.gender, actor.gender);
            merge_option_copy(&mut existing.order, actor.order);
            merge_option_string(&mut existing.name, actor.name);
        } else {
            target.push(actor);
        }
    }

    reindex_actor_orders(target);
}

fn reindex_actor_orders(actors: &mut [Actor]) {
    for (index, actor) in actors.iter_mut().enumerate() {
        let next_order = u32::try_from(index + 1).unwrap_or(u32::MAX);
        actor.order = Some(next_order);
    }
}

fn merge_unique_ids(target: &mut Vec<UniqueId>, incoming: Vec<UniqueId>) {
    for unique_id in incoming {
        if let Some(existing) = find_unique_id_mut(target, &unique_id) {
            merge_option_string(&mut existing.id_type, unique_id.id_type);
            merge_option_copy(&mut existing.is_default, unique_id.is_default);
            merge_option_string(&mut existing.value, unique_id.value);
        } else {
            target.push(unique_id);
        }
    }
}

fn merge_string_list(target: &mut Vec<String>, incoming: Vec<String>) {
    for value in incoming {
        let normalized = normalized_text(&value);
        if normalized.is_empty() {
            continue;
        }
        if !target
            .iter()
            .any(|existing| normalized_text(existing) == normalized)
        {
            target.push(value.trim().to_string());
        }
    }
}

fn find_rating_mut<'a>(ratings: &'a mut [Rating], incoming: &Rating) -> Option<&'a mut Rating> {
    let incoming_name = normalized_option(incoming.name.as_deref());
    for rating in ratings {
        let current_name = normalized_option(rating.name.as_deref());
        if current_name == incoming_name {
            return Some(rating);
        }
    }
    None
}

fn find_actor_mut<'a>(actors: &'a mut [Actor], incoming: &Actor) -> Option<&'a mut Actor> {
    let incoming_name = normalized_option(incoming.name.as_deref());
    let mut same_name_index = None;

    for (index, actor) in actors.iter().enumerate() {
        let current_name = normalized_option(actor.name.as_deref());
        if current_name != incoming_name {
            continue;
        }
        if actor.order == incoming.order {
            return actors.get_mut(index);
        }
        if same_name_index.is_none() && incoming_name.is_some() {
            same_name_index = Some(index);
        }
    }

    same_name_index.and_then(|index| actors.get_mut(index))
}

fn find_unique_id_mut<'a>(
    unique_ids: &'a mut [UniqueId],
    incoming: &UniqueId,
) -> Option<&'a mut UniqueId> {
    let incoming_type = normalized_option(incoming.id_type.as_deref());
    let incoming_value = normalized_option(incoming.value.as_deref());

    for unique_id in unique_ids {
        let current_type = normalized_option(unique_id.id_type.as_deref());
        let current_value = normalized_option(unique_id.value.as_deref());
        if current_type == incoming_type && current_value == incoming_value {
            return Some(unique_id);
        }
    }

    None
}

fn merge_option_string(target: &mut Option<String>, incoming: Option<String>) {
    if target.as_deref().is_some_and(has_content) {
        return;
    }
    if let Some(value) = incoming {
        if has_content(value.as_str()) {
            *target = Some(value.trim().to_string());
        }
    }
}

fn merge_option_copy<T: Copy>(target: &mut Option<T>, incoming: Option<T>) {
    if target.is_none() {
        *target = incoming;
    }
}

fn normalized_text(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}

fn normalized_option(value: Option<&str>) -> Option<String> {
    value.and_then(|text| {
        if has_content(text) {
            Some(normalized_text(text))
        } else {
            None
        }
    })
}

fn has_content(value: &str) -> bool {
    !value.trim().is_empty()
}

fn thumb_has_content(thumb: &Thumb) -> bool {
    thumb.value.as_deref().is_some_and(has_content)
        || thumb.preview.as_deref().is_some_and(has_content)
        || thumb.aspect.as_deref().is_some_and(has_content)
}

fn select_one_fanart_thumb(thumbs: Vec<Thumb>) -> Option<Thumb> {
    let mut iter = thumbs.into_iter();
    let first = iter.next();
    if first.as_ref().is_some_and(thumb_has_content) {
        return first;
    }
    for thumb in iter {
        if thumb_has_content(&thumb) {
            return Some(thumb);
        }
    }
    first
}

#[cfg(test)]
mod tests {
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
    fn merge_movie_keeps_single_fanart_thumb() {
        let mut target = Movie {
            fanart: Some(Fanart {
                thumb: vec![Thumb {
                    value: Some("fanart-a.jpg".to_string()),
                    ..Thumb::default()
                }],
            }),
            ..Movie::default()
        };

        let incoming = Movie {
            fanart: Some(Fanart {
                thumb: vec![Thumb {
                    value: Some("fanart-b.jpg".to_string()),
                    ..Thumb::default()
                }],
            }),
            ..Movie::default()
        };

        merge_movie(&mut target, incoming);

        let fanart = target.fanart.expect("fanart");
        assert_eq!(fanart.thumb.len(), 1);
        assert_eq!(fanart.thumb[0].value, Some("fanart-a.jpg".to_string()));
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
}
