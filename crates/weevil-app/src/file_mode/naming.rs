use std::path::{Path, PathBuf};

use crate::errors::AppError;
use crate::nfo::{Actor, Movie};

pub(crate) fn build_file_name(base: &str, extension: Option<&str>) -> String {
    match extension {
        Some(ext) => format!("{base}.{ext}"),
        None => base.to_string(),
    }
}

pub(crate) fn format_file_base(
    template: &str,
    movie: &Movie,
    fallback: &str,
) -> Result<String, AppError> {
    let rendered = render_template(template, movie)?;
    let resolved = if rendered.trim().is_empty() {
        fallback
    } else {
        rendered.as_str()
    };
    let sanitized = sanitize_component(resolved);
    if sanitized.is_empty() {
        return Err(AppError::TemplateEmpty {
            template: template.to_string(),
        });
    }
    Ok(sanitized)
}

pub(crate) fn format_folder_path(
    template: &str,
    movie: &Movie,
    fallback: &str,
) -> Result<PathBuf, AppError> {
    let rendered = render_template(template, movie)?;
    let resolved = if rendered.trim().is_empty() {
        fallback.to_string()
    } else {
        rendered
    };
    if Path::new(&resolved).is_absolute() {
        return Err(AppError::TemplateAbsolutePath {
            template: template.to_string(),
        });
    }
    let mut path = PathBuf::new();
    for segment in resolved.split('/') {
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
        path.push(sanitized);
    }
    if path.as_os_str().is_empty() {
        return Err(AppError::TemplateEmpty {
            template: template.to_string(),
        });
    }
    Ok(path)
}

pub(crate) fn format_input_name(input: &str, remove: &[String]) -> Result<String, AppError> {
    if remove.is_empty() {
        return Ok(input.to_string());
    }

    let mut current = input.to_string();
    for token in remove {
        let trimmed = token.trim();
        if trimmed.is_empty() {
            continue;
        }
        current = current.replace(trimmed, "");
    }

    let cleaned = collapse_whitespace(&current);
    if cleaned.is_empty() {
        return Err(AppError::InputNameFormatEmpty {
            input: input.to_string(),
            rules: remove.to_vec(),
        });
    }
    Ok(cleaned)
}

fn render_template(template: &str, movie: &Movie) -> Result<String, AppError> {
    let mut out = String::new();
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
                let value = lookup_field(movie, field, template)?;
                if let Some(value) = value {
                    out.push_str(&value);
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
    Ok(out)
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
        "actor" => join_actor_names(&movie.actor),
        "actor.gender" => join_actor_genders(&movie.actor),
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

fn join_actor_names(actors: &[Actor]) -> Option<String> {
    if actors.is_empty() {
        return None;
    }
    let mut names = Vec::new();
    for actor in actors {
        if let Some(name) = actor.name.as_ref() {
            names.push(name.clone());
        }
    }
    if names.is_empty() {
        None
    } else {
        Some(names.join(", "))
    }
}

fn join_actor_genders(actors: &[Actor]) -> Option<String> {
    if actors.is_empty() {
        return None;
    }
    let mut genders = Vec::new();
    for actor in actors {
        if let Some(gender) = actor.gender.as_ref() {
            if !gender.trim().is_empty() {
                genders.push(gender.trim().to_string());
            }
        }
    }
    if genders.is_empty() {
        None
    } else {
        Some(genders.join(", "))
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

fn collapse_whitespace(value: &str) -> String {
    let mut out = String::new();
    let mut last_space = false;
    for ch in value.chars() {
        if ch.is_whitespace() {
            if !last_space {
                out.push(' ');
                last_space = true;
            }
            continue;
        }
        out.push(ch);
        last_space = false;
    }
    out.trim().to_string()
}

#[cfg(test)]
#[path = "../tests/naming.rs"]
mod tests;
