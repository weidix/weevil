use std::fmt;

/// A single HTML parse issue reported by the parser.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HtmlParseIssue {
    message: String,
}

impl HtmlParseIssue {
    pub(crate) fn new(message: String) -> Self {
        Self { message }
    }

    /// Returns the parser message for this issue.
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for HtmlParseIssue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{message}", message = self.message)
    }
}

/// Errors returned when HTML parsing reports issues.
#[derive(Debug, Clone)]
pub struct HtmlParseError {
    errors: Vec<HtmlParseIssue>,
}

impl HtmlParseError {
    pub(crate) fn new(errors: Vec<HtmlParseIssue>) -> Self {
        Self { errors }
    }

    /// Returns the parse issues reported by the parser.
    pub fn errors(&self) -> &[HtmlParseIssue] {
        &self.errors
    }

    /// Consumes the error and returns the underlying issues.
    pub fn into_errors(self) -> Vec<HtmlParseIssue> {
        self.errors
    }
}

impl fmt::Display for HtmlParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let total = self.errors.len();
        write!(f, "HTML parse reported {total} error(s)")?;
        let preview_limit = 5usize;
        for (idx, issue) in self.errors.iter().take(preview_limit).enumerate() {
            let display_idx = idx + 1;
            write!(f, "\n  {display_idx}: {issue}")?;
        }
        if total > preview_limit {
            let remaining = total - preview_limit;
            write!(f, "\n  ...and {remaining} more")?;
        }
        Ok(())
    }
}

impl std::error::Error for HtmlParseError {}
