use regex::Regex;

use crate::errors::AppError;

#[derive(Debug, Clone)]
enum InputNameRule {
    RemoveLiteral { value: String },
    RemoveRegex { pattern: Regex },
    ReplaceLiteral { from: String, to: String },
    ReplaceRegex { pattern: Regex, to: String },
}

pub(crate) fn format_input_name(input: &str, rules: &[String]) -> Result<String, AppError> {
    if rules.is_empty() {
        return Ok(input.to_string());
    }

    let expanded = expand_rules(rules);
    let compiled = expanded
        .iter()
        .map(|rule| parse_rule(rule))
        .collect::<Result<Vec<_>, AppError>>()?;

    let mut current = input.to_string();
    for rule in &compiled {
        current = apply_rule(&current, rule);
    }

    let cleaned = collapse_whitespace(&current);
    if cleaned.is_empty() {
        return Err(AppError::InputNameRuleResultEmpty {
            input: input.to_string(),
            rules: expanded,
        });
    }

    Ok(cleaned)
}

fn expand_rules(rules: &[String]) -> Vec<String> {
    let mut expanded = Vec::new();
    for rule in rules {
        let trimmed = rule.trim();
        if trimmed.is_empty() {
            continue;
        }

        let start_trimmed = rule.trim_start();
        if has_prefixed_syntax(start_trimmed) {
            expanded.push(start_trimmed.to_string());
            continue;
        }

        if trimmed.contains(',') {
            expanded.extend(
                trimmed
                    .split(',')
                    .map(str::trim)
                    .filter(|part| !part.is_empty())
                    .map(ToString::to_string),
            );
            continue;
        }

        expanded.push(trimmed.to_string());
    }
    expanded
}

fn has_prefixed_syntax(rule: &str) -> bool {
    rule.starts_with("literal:")
        || rule.starts_with("regex:")
        || rule.starts_with("replace:")
        || rule.starts_with("regex-replace:")
}

fn parse_rule(raw: &str) -> Result<InputNameRule, AppError> {
    let trimmed = raw.trim();
    let start_trimmed = raw.trim_start();
    if let Some(rest) = start_trimmed.strip_prefix("literal:") {
        return Ok(InputNameRule::RemoveLiteral {
            value: rest.trim().to_string(),
        });
    }

    if let Some(rest) = start_trimmed.strip_prefix("regex:") {
        let pattern = rest.trim();
        let pattern = compile_regex(pattern, start_trimmed)?;
        return Ok(InputNameRule::RemoveRegex { pattern });
    }

    if let Some(rest) = start_trimmed.strip_prefix("replace:") {
        let (from, to) = parse_replace_parts(rest, start_trimmed)?;
        return Ok(InputNameRule::ReplaceLiteral {
            from: from.to_string(),
            to: to.to_string(),
        });
    }

    if let Some(rest) = start_trimmed.strip_prefix("regex-replace:") {
        let (pattern, to) = parse_replace_parts(rest, start_trimmed)?;
        let pattern = compile_regex(pattern, start_trimmed)?;
        return Ok(InputNameRule::ReplaceRegex {
            pattern,
            to: to.to_string(),
        });
    }

    Ok(InputNameRule::RemoveLiteral {
        value: trimmed.to_string(),
    })
}

fn parse_replace_parts<'a>(input: &'a str, raw_rule: &str) -> Result<(&'a str, &'a str), AppError> {
    let Some((from, to)) = input.split_once("=>") else {
        return Err(AppError::InputNameRuleInvalid {
            rule: raw_rule.to_string(),
            reason: "missing '=>' separator".to_string(),
        });
    };
    let from = from.trim();
    if from.is_empty() {
        return Err(AppError::InputNameRuleInvalid {
            rule: raw_rule.to_string(),
            reason: "left side cannot be empty".to_string(),
        });
    }
    Ok((from, to))
}

fn compile_regex(pattern: &str, raw_rule: &str) -> Result<Regex, AppError> {
    if pattern.is_empty() {
        return Err(AppError::InputNameRuleInvalid {
            rule: raw_rule.to_string(),
            reason: "regex pattern cannot be empty".to_string(),
        });
    }

    Regex::new(pattern).map_err(|error| AppError::InputNameRuleInvalid {
        rule: raw_rule.to_string(),
        reason: format!("invalid regex pattern: {error}"),
    })
}

fn apply_rule(input: &str, rule: &InputNameRule) -> String {
    match rule {
        InputNameRule::RemoveLiteral { value } => {
            if value.is_empty() {
                input.to_string()
            } else {
                input.replace(value, "")
            }
        }
        InputNameRule::RemoveRegex { pattern } => pattern.replace_all(input, "").into_owned(),
        InputNameRule::ReplaceLiteral { from, to } => input.replace(from, to),
        InputNameRule::ReplaceRegex { pattern, to } => {
            pattern.replace_all(input, to.as_str()).into_owned()
        }
    }
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
mod tests {
    use super::format_input_name;
    use crate::errors::AppError;

    #[test]
    fn keeps_legacy_rule_behavior() {
        let formatted = format_input_name(
            "Movie 1080p WEB-DL",
            &["1080p".to_string(), "WEB-DL".to_string()],
        )
        .expect("formatted");
        assert_eq!(formatted, "Movie");
    }

    #[test]
    fn keeps_legacy_csv_behavior() {
        let formatted = format_input_name("Movie 1080p WEB-DL", &["1080p,WEB-DL".to_string()])
            .expect("formatted");
        assert_eq!(formatted, "Movie");
    }

    #[test]
    fn removes_with_regex_rule() {
        let formatted = format_input_name("Movie [1080p]", &["regex:\\[[^\\]]+\\]".to_string()])
            .expect("formatted");
        assert_eq!(formatted, "Movie");
    }

    #[test]
    fn replaces_with_literal_rule() {
        let formatted = format_input_name(
            "Movie.S01E01",
            &[
                "replace:.=> ".to_string(),
                "replace:S01E01=>S1E1".to_string(),
            ],
        )
        .expect("formatted");
        assert_eq!(formatted, "Movie S1E1");
    }

    #[test]
    fn replaces_with_regex_rule() {
        let formatted = format_input_name(
            "Movie_2024_1080p",
            &[
                "regex-replace:_+=> ".to_string(),
                "regex:\\b\\d{4}\\b".to_string(),
            ],
        )
        .expect("formatted");
        assert_eq!(formatted, "Movie 1080p");
    }

    #[test]
    fn rejects_invalid_regex_rule() {
        let error = format_input_name("Movie", &["regex:(".to_string()]).expect_err("error");
        assert!(matches!(error, AppError::InputNameRuleInvalid { .. }));
    }

    #[test]
    fn rejects_invalid_replace_rule() {
        let error = format_input_name("Movie", &["replace:title".to_string()]).expect_err("error");
        assert!(matches!(error, AppError::InputNameRuleInvalid { .. }));
    }

    #[test]
    fn empty_after_rules_is_error() {
        let error = format_input_name("1080p", &["regex:\\w+".to_string()]).expect_err("error");
        assert!(matches!(error, AppError::InputNameRuleResultEmpty { .. }));
    }
}
