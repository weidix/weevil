mod actor;

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::errors::AppError;
use crate::nfo::{Actor, Movie};

use actor::{parse_actor_field, render_actor_field};

#[cfg(test)]
pub(crate) use super::input_name::format_input_name;

struct TemplateRender {
    rendered: String,
    missing_actor_value: bool,
}

pub(crate) fn build_file_name(base: &str, extension: Option<&str>) -> String {
    match extension {
        Some(ext) => format!("{base}.{ext}"),
        None => base.to_string(),
    }
}

pub(crate) fn format_output_paths(
    template: &str,
    movie: &Movie,
    fallback: &str,
) -> Result<Vec<PathBuf>, AppError> {
    let default_render = render_template_with_meta(template, movie, None, true)?;
    let fields = collect_template_fields(template)?;
    let mut expand_actor = false;
    let mut expand_genre = false;
    let mut expand_tag = false;
    let mut expand_country = false;
    let mut expand_credits = false;
    for field in &fields {
        if parse_actor_field(field, template)?.is_some() {
            expand_actor = true;
            continue;
        }
        match field.as_str() {
            "genre" => expand_genre = true,
            "tag" => expand_tag = true,
            "country" => expand_country = true,
            "credits" => expand_credits = true,
            _ => {}
        }
    }

    let mut paths = Vec::new();
    let mut seen = HashSet::new();
    let actor_options: Vec<Option<&Actor>> = if expand_actor {
        movie.actor.iter().map(Some).collect()
    } else {
        vec![None]
    };
    let genre_values = list_values(expand_genre, &movie.genre);
    let tag_values = list_values(expand_tag, &movie.tag);
    let country_values = list_values(expand_country, &movie.country);
    let credits_values = list_values(expand_credits, &movie.credits);

    for actor in actor_options {
        for genre in &genre_values {
            for tag in &tag_values {
                for country in &country_values {
                    for credits in &credits_values {
                        let mut scoped = movie.clone();
                        if let Some(value) = genre.as_ref() {
                            scoped.genre = vec![value.clone()];
                        }
                        if let Some(value) = tag.as_ref() {
                            scoped.tag = vec![value.clone()];
                        }
                        if let Some(value) = country.as_ref() {
                            scoped.country = vec![value.clone()];
                        }
                        if let Some(value) = credits.as_ref() {
                            scoped.credits = vec![value.clone()];
                        }
                        let rendered = render_template_with_meta(template, &scoped, actor, true)?;
                        if rendered.missing_actor_value {
                            continue;
                        }
                        if rendered.rendered.trim().is_empty() {
                            continue;
                        }
                        let path = format_output_path_resolved(template, &rendered.rendered)?;
                        if seen.insert(path.clone()) {
                            paths.push(path);
                        }
                    }
                }
            }
        }
    }

    if paths.is_empty() {
        let resolved =
            if default_render.rendered.trim().is_empty() || default_render.missing_actor_value {
                fallback
            } else {
                default_render.rendered.as_str()
            };
        let path = format_output_path_resolved(template, resolved)?;
        paths.push(path);
    }
    Ok(paths)
}

fn list_values(expand: bool, values: &[String]) -> Vec<Option<String>> {
    if !expand {
        return vec![None];
    }
    values
        .iter()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .map(|value| Some(value.to_string()))
        .collect()
}

fn collect_template_fields(template: &str) -> Result<Vec<String>, AppError> {
    let mut fields = Vec::new();
    let mut chars = template.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '{' => {
                if matches!(chars.peek(), Some('{')) {
                    chars.next();
                    continue;
                }
                let mut key = String::new();
                let mut closed = false;
                while let Some(next) = chars.next() {
                    if next == '}' {
                        closed = true;
                        break;
                    }
                    key.push(next);
                }
                if !closed {
                    return Err(AppError::TemplateInvalid {
                        template: template.to_string(),
                        reason: "missing closing '}'".to_string(),
                    });
                }
                let field = key.trim();
                if field.is_empty() {
                    return Err(AppError::TemplateInvalid {
                        template: template.to_string(),
                        reason: "empty field".to_string(),
                    });
                }
                fields.push(field.to_string());
            }
            '}' => {
                if matches!(chars.peek(), Some('}')) {
                    chars.next();
                } else {
                    return Err(AppError::TemplateInvalid {
                        template: template.to_string(),
                        reason: "unmatched '}'".to_string(),
                    });
                }
            }
            _ => {}
        }
    }
    Ok(fields)
}

#[cfg(test)]
pub(crate) fn render_template(template: &str, movie: &Movie) -> Result<String, AppError> {
    render_template_with_meta(template, movie, None, false).map(|rendered| rendered.rendered)
}

fn render_template_with_meta(
    template: &str,
    movie: &Movie,
    actor: Option<&Actor>,
    sanitize_values: bool,
) -> Result<TemplateRender, AppError> {
    let mut out = String::new();
    let mut missing_actor_value = false;
    let mut chars = template.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '{' => {
                if matches!(chars.peek(), Some('{')) {
                    chars.next();
                    out.push('{');
                    continue;
                }
                let mut key = String::new();
                let mut closed = false;
                while let Some(next) = chars.next() {
                    if next == '}' {
                        closed = true;
                        break;
                    }
                    key.push(next);
                }
                if !closed {
                    return Err(AppError::TemplateInvalid {
                        template: template.to_string(),
                        reason: "missing closing '}'".to_string(),
                    });
                }
                let field = key.trim();
                if field.is_empty() {
                    return Err(AppError::TemplateInvalid {
                        template: template.to_string(),
                        reason: "empty field".to_string(),
                    });
                }
                if let Some(spec) = parse_actor_field(field, template)? {
                    if let Some(mut value) = render_actor_field(&movie.actor, actor, &spec) {
                        if sanitize_values {
                            value = sanitize_component(&value);
                        }
                        if value.is_empty() {
                            missing_actor_value = true;
                            continue;
                        }
                        out.push_str(&value);
                    } else {
                        missing_actor_value = true;
                    }
                } else {
                    let value = lookup_field(movie, field, template)?;
                    if let Some(mut value) = value {
                        if sanitize_values {
                            value = sanitize_component(&value);
                        }
                        out.push_str(&value);
                    }
                }
            }
            '}' => {
                if matches!(chars.peek(), Some('}')) {
                    chars.next();
                    out.push('}');
                } else {
                    return Err(AppError::TemplateInvalid {
                        template: template.to_string(),
                        reason: "unmatched '}'".to_string(),
                    });
                }
            }
            other => out.push(other),
        }
    }
    Ok(TemplateRender {
        rendered: out,
        missing_actor_value,
    })
}

fn lookup_field(movie: &Movie, field: &str, template: &str) -> Result<Option<String>, AppError> {
    let value = match field {
        "title" => movie.title.clone(),
        "originaltitle" => movie.originaltitle.clone(),
        "sorttitle" => movie.sorttitle.clone(),
        "year" => movie.year.map(|value| value.to_string()),
        "premiered" => movie.premiered.clone(),
        "runtime" => movie.runtime.map(|value| value.to_string()),
        "director" => movie.director.clone(),
        "studio" => movie.studio.clone(),
        "tagline" => movie.tagline.clone(),
        "plot" => movie.plot.clone(),
        "outline" => movie.outline.clone(),
        "fileinfo" => movie.fileinfo.clone(),
        "trailer" => movie.trailer.clone(),
        "dateadded" => movie.dateadded.clone(),
        "userrating" => movie.userrating.map(|value| format!("{value}")),
        "set.name" => movie.set_info.as_ref().and_then(|set| set.name.clone()),
        "set.overview" => movie.set_info.as_ref().and_then(|set| set.overview.clone()),
        "genre" => join_list(&movie.genre),
        "tag" => join_list(&movie.tag),
        "country" => join_list(&movie.country),
        "credits" => join_list(&movie.credits),
        "uniqueid" => select_uniqueid(movie),
        _ if field.starts_with("uniqueid.") => {
            let id_type = field.trim_start_matches("uniqueid.");
            uniqueid_by_type(movie, id_type)
        }
        _ => {
            return Err(AppError::TemplateUnknownField {
                template: template.to_string(),
                field: field.to_string(),
            });
        }
    };
    Ok(value)
}

fn join_list(values: &[String]) -> Option<String> {
    if values.is_empty() {
        None
    } else {
        Some(values.join(", "))
    }
}

fn select_uniqueid(movie: &Movie) -> Option<String> {
    let default = movie
        .uniqueid
        .iter()
        .find(|entry| entry.is_default == Some(true));
    default
        .and_then(|entry| entry.value.clone())
        .or_else(|| movie.uniqueid.first().and_then(|entry| entry.value.clone()))
}

fn uniqueid_by_type(movie: &Movie, id_type: &str) -> Option<String> {
    movie
        .uniqueid
        .iter()
        .find(|entry| entry.id_type.as_deref() == Some(id_type))
        .and_then(|entry| entry.value.clone())
}

fn format_output_path_resolved(template: &str, resolved: &str) -> Result<PathBuf, AppError> {
    let trimmed = resolved.trim();
    if trimmed.is_empty() {
        return Err(AppError::TemplateEmpty {
            template: template.to_string(),
        });
    }

    let mut path = PathBuf::new();
    let mut segments = trimmed.split('/').peekable();
    if let Some(first) = segments.peek() {
        if first.is_empty() {
            path.push(Path::new("/"));
            segments.next();
        } else if cfg!(windows) && is_windows_drive_prefix(first) {
            let drive = segments.next().unwrap_or_default();
            path.push(format!("{drive}\\"));
            if matches!(segments.peek(), Some(next) if next.is_empty()) {
                segments.next();
            }
        }
    }

    let mut sanitized_segments = Vec::new();
    for segment in segments {
        if segment.is_empty() {
            continue;
        }
        let sanitized = sanitize_component(segment);
        if sanitized.is_empty() {
            return Err(AppError::TemplateEmptySegment {
                template: template.to_string(),
            });
        }
        if sanitized == "." || sanitized == ".." {
            return Err(AppError::TemplateInvalid {
                template: template.to_string(),
                reason: format!("invalid path segment {sanitized:?}"),
            });
        }
        sanitized_segments.push(sanitized);
    }

    if sanitized_segments.is_empty() {
        return Err(AppError::TemplateEmpty {
            template: template.to_string(),
        });
    }

    let file_name = sanitized_segments.pop().unwrap_or_default();
    for segment in sanitized_segments {
        path.push(segment);
    }
    path.push(file_name);

    Ok(path)
}

fn is_windows_drive_prefix(segment: &str) -> bool {
    let bytes = segment.as_bytes();
    bytes.len() == 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':'
}

fn sanitize_component(value: &str) -> String {
    let mut out = String::new();
    let mut last_space = false;
    for ch in value.chars() {
        if ch.is_control() {
            continue;
        }
        let mapped = match ch {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            _ => ch,
        };
        if mapped.is_whitespace() {
            if !last_space {
                out.push(' ');
                last_space = true;
            }
            continue;
        }
        out.push(mapped);
        last_space = false;
    }
    out.trim().trim_matches('.').to_string()
}

#[cfg(test)]
#[path = "../tests/naming.rs"]
mod tests;
