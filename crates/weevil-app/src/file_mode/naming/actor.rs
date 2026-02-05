use crate::errors::AppError;
use crate::nfo::Actor;

#[derive(Debug, Clone, Default)]
pub(crate) struct ActorFilters {
    filters: Vec<ActorFilter>,
}

#[derive(Debug, Clone)]
enum ActorFilter {
    Name(String),
    Gender(String),
    Role(String),
    Order(u32),
}

#[derive(Debug, Clone, Copy)]
enum ActorValue {
    Name,
    Gender,
    Role,
    Order,
}

pub(super) struct ActorFieldSpec {
    value: ActorValue,
    filters: ActorFilters,
}

pub(super) fn parse_actor_field(
    field: &str,
    template: &str,
) -> Result<Option<ActorFieldSpec>, AppError> {
    let (base, filters) = if let Some((base, rest)) = field.split_once('[') {
        if !rest.ends_with(']') {
            return Err(AppError::TemplateInvalid {
                template: template.to_string(),
                reason: format!("actor filter {field:?} is missing closing ']'"),
            });
        }
        let inner = &rest[..rest.len() - 1];
        if inner.contains('[') || inner.contains(']') {
            return Err(AppError::TemplateInvalid {
                template: template.to_string(),
                reason: format!("actor filter {field:?} contains nested brackets"),
            });
        }
        let trimmed = base.trim();
        if trimmed.is_empty() {
            return Err(AppError::TemplateInvalid {
                template: template.to_string(),
                reason: format!("actor filter {field:?} is missing a field name"),
            });
        }
        let filters = ActorFilters::parse(inner, template)?;
        (trimmed, filters)
    } else {
        (field.trim(), ActorFilters::default())
    };

    let value = match base {
        "actor" | "actor.name" => ActorValue::Name,
        "actor.gender" => ActorValue::Gender,
        "actor.role" => ActorValue::Role,
        "actor.order" => ActorValue::Order,
        _ => {
            if !filters.filters.is_empty() {
                return Err(AppError::TemplateInvalid {
                    template: template.to_string(),
                    reason: format!(
                        "actor filters are only supported for actor fields, got {base:?}"
                    ),
                });
            }
            return Ok(None);
        }
    };
    Ok(Some(ActorFieldSpec { value, filters }))
}

pub(super) fn render_actor_field(
    actors: &[Actor],
    actor: Option<&Actor>,
    spec: &ActorFieldSpec,
) -> Option<String> {
    match actor {
        Some(actor) => render_actor_value(actor, spec),
        None => render_actor_values(actors, spec),
    }
}

impl ActorFilters {
    pub(crate) fn parse(expression: &str, template: &str) -> Result<Self, AppError> {
        let trimmed = expression.trim();
        if trimmed.is_empty() {
            return Err(AppError::TemplateInvalid {
                template: template.to_string(),
                reason: "actor filter is empty".to_string(),
            });
        }
        let mut filters = Vec::new();
        for raw in trimmed.split(',') {
            let item = raw.trim();
            if item.is_empty() {
                return Err(AppError::TemplateInvalid {
                    template: template.to_string(),
                    reason: format!("actor filter {expression:?} contains an empty segment"),
                });
            }
            let (key, value) = item
                .split_once('=')
                .ok_or_else(|| AppError::TemplateInvalid {
                    template: template.to_string(),
                    reason: format!("actor filter {item:?} is missing '='"),
                })?;
            let key = key.trim();
            if key.is_empty() {
                return Err(AppError::TemplateInvalid {
                    template: template.to_string(),
                    reason: format!("actor filter {item:?} has an empty key"),
                });
            }
            let value = normalize_filter_value(value, template, item)?;
            match key {
                "name" => filters.push(ActorFilter::Name(value)),
                "gender" => filters.push(ActorFilter::Gender(value)),
                "role" => filters.push(ActorFilter::Role(value)),
                "order" => {
                    let parsed = value
                        .parse::<u32>()
                        .map_err(|_| AppError::TemplateInvalid {
                            template: template.to_string(),
                            reason: format!("actor filter {item:?} has invalid order {value:?}"),
                        })?;
                    filters.push(ActorFilter::Order(parsed));
                }
                _ => {
                    return Err(AppError::TemplateInvalid {
                        template: template.to_string(),
                        reason: format!(
                            "actor filter {item:?} uses unknown key {key:?} (expected name, gender, role, order)"
                        ),
                    });
                }
            }
        }
        Ok(Self { filters })
    }

    pub(crate) fn matches(&self, actor: &Actor) -> bool {
        if self.filters.is_empty() {
            return true;
        }
        self.filters.iter().all(|filter| match filter {
            ActorFilter::Name(expected) => actor_text_equals(&actor.name, expected),
            ActorFilter::Gender(expected) => actor_text_equals(&actor.gender, expected),
            ActorFilter::Role(expected) => actor_text_equals(&actor.role, expected),
            ActorFilter::Order(expected) => actor.order == Some(*expected),
        })
    }
}

fn render_actor_value(actor: &Actor, spec: &ActorFieldSpec) -> Option<String> {
    if !spec.filters.matches(actor) {
        return None;
    }
    actor_value(actor, spec.value)
}

fn render_actor_values(actors: &[Actor], spec: &ActorFieldSpec) -> Option<String> {
    if actors.is_empty() {
        return None;
    }
    let mut values = Vec::new();
    for actor in actors {
        if !spec.filters.matches(actor) {
            continue;
        }
        if let Some(value) = actor_value(actor, spec.value) {
            values.push(value);
        }
    }
    if values.is_empty() {
        None
    } else {
        Some(values.join(", "))
    }
}

fn actor_value(actor: &Actor, value: ActorValue) -> Option<String> {
    match value {
        ActorValue::Name => actor
            .name
            .as_ref()
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
            .map(str::to_string),
        ActorValue::Gender => actor
            .gender
            .as_ref()
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
            .map(str::to_string),
        ActorValue::Role => actor
            .role
            .as_ref()
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
            .map(str::to_string),
        ActorValue::Order => actor.order.map(|value| value.to_string()),
    }
}

fn normalize_filter_value(raw: &str, template: &str, item: &str) -> Result<String, AppError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(AppError::TemplateInvalid {
            template: template.to_string(),
            reason: format!("actor filter {item:?} has an empty value"),
        });
    }
    let unquoted = strip_quotes(trimmed);
    let normalized = unquoted.trim();
    if normalized.is_empty() {
        return Err(AppError::TemplateInvalid {
            template: template.to_string(),
            reason: format!("actor filter {item:?} has an empty value"),
        });
    }
    Ok(normalized.to_string())
}

fn strip_quotes(value: &str) -> &str {
    let bytes = value.as_bytes();
    if bytes.len() >= 2
        && ((bytes[0] == b'"' && bytes[bytes.len() - 1] == b'"')
            || (bytes[0] == b'\'' && bytes[bytes.len() - 1] == b'\''))
    {
        &value[1..value.len() - 1]
    } else {
        value
    }
}

fn actor_text_equals(value: &Option<String>, expected: &str) -> bool {
    value
        .as_ref()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .map(|value| value.eq_ignore_ascii_case(expected))
        .unwrap_or(false)
}
