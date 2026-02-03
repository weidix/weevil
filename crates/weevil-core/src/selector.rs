//! CSS selector parsing for weevil core.

use std::convert::TryFrom;
use std::fmt;

pub use cssparser::ToCss;
use cssparser::{BasicParseErrorKind, ParseErrorKind, SourceLocation, Token};
use html5ever::{LocalName, Namespace};
use precomputed_hash::PrecomputedHash;
use selectors::parser::{self, ParseRelative, SelectorList, SelectorParseErrorKind};

#[cfg(test)]
mod tests;

/// Wrapper around CSS selectors.
///
/// Represents a comma-separated list of selectors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Selector {
    selectors: SelectorList<Simple>,
}

impl Selector {
    /// Parses a CSS selector group.
    pub fn parse(selectors: &str) -> Result<Self, SelectorError> {
        let mut parser_input = cssparser::ParserInput::new(selectors);
        let mut parser = cssparser::Parser::new(&mut parser_input);

        SelectorList::parse(&Parser, &mut parser, ParseRelative::No)
            .map(|selectors| Self { selectors })
            .map_err(|err| SelectorError::from_parse_error(selectors, err))
    }

    pub(crate) fn selector_list(&self) -> &SelectorList<Simple> {
        &self.selectors
    }
}

impl ToCss for Selector {
    fn to_css<W>(&self, dest: &mut W) -> fmt::Result
    where
        W: fmt::Write,
    {
        self.selectors.to_css(dest)
    }
}

impl<'i> TryFrom<&'i str> for Selector {
    type Error = SelectorError;

    fn try_from(s: &'i str) -> Result<Self, Self::Error> {
        Selector::parse(s)
    }
}

/// An implementation of `Parser` for `selectors`.
#[derive(Clone, Copy, Debug)]
pub struct Parser;

impl<'i> parser::Parser<'i> for Parser {
    type Impl = Simple;
    type Error = SelectorParseErrorKind<'i>;

    fn parse_is_and_where(&self) -> bool {
        true
    }

    fn parse_has(&self) -> bool {
        true
    }
}

/// A simple implementation of `SelectorImpl` with no pseudo-classes or pseudo-elements.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Simple;

impl parser::SelectorImpl for Simple {
    type AttrValue = CssString;
    type Identifier = CssLocalName;
    type LocalName = CssLocalName;
    type NamespacePrefix = CssLocalName;
    type NamespaceUrl = Namespace;
    type BorrowedNamespaceUrl = Namespace;
    type BorrowedLocalName = CssLocalName;

    type NonTSPseudoClass = NonTSPseudoClass;
    type PseudoElement = PseudoElement;

    type ExtraMatchingData<'a> = ();
}

/// Wraps [`String`] so that it can be used with [`selectors`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CssString(pub String);

impl<'a> From<&'a str> for CssString {
    fn from(val: &'a str) -> Self {
        Self(val.to_owned())
    }
}

impl AsRef<str> for CssString {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl ToCss for CssString {
    fn to_css<W>(&self, dest: &mut W) -> fmt::Result
    where
        W: fmt::Write,
    {
        cssparser::serialize_string(&self.0, dest)
    }
}

/// Wraps [`LocalName`] so that it can be used with [`selectors`].
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct CssLocalName(pub LocalName);

impl<'a> From<&'a str> for CssLocalName {
    fn from(val: &'a str) -> Self {
        Self(val.into())
    }
}

impl ToCss for CssLocalName {
    fn to_css<W>(&self, dest: &mut W) -> fmt::Result
    where
        W: fmt::Write,
    {
        dest.write_str(&self.0)
    }
}

impl PrecomputedHash for CssLocalName {
    fn precomputed_hash(&self) -> u32 {
        self.0.precomputed_hash()
    }
}

/// Non tree-structural pseudo-class.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NonTSPseudoClass {}

impl parser::NonTSPseudoClass for NonTSPseudoClass {
    type Impl = Simple;

    fn is_active_or_hover(&self) -> bool {
        false
    }

    fn is_user_action_state(&self) -> bool {
        false
    }
}

impl ToCss for NonTSPseudoClass {
    fn to_css<W>(&self, dest: &mut W) -> fmt::Result
    where
        W: fmt::Write,
    {
        dest.write_str("")
    }
}

/// CSS pseudo-element.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PseudoElement {}

impl parser::PseudoElement for PseudoElement {
    type Impl = Simple;
}

impl ToCss for PseudoElement {
    fn to_css<W>(&self, dest: &mut W) -> fmt::Result
    where
        W: fmt::Write,
    {
        dest.write_str("")
    }
}

/// Location information for selector parsing errors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SelectorLocation {
    line: u32,
    column: u32,
}

impl SelectorLocation {
    /// Returns the 1-based line number.
    pub fn line(self) -> u32 {
        self.line
    }

    /// Returns the 1-based column number.
    pub fn column(self) -> u32 {
        self.column
    }
}

impl From<SourceLocation> for SelectorLocation {
    fn from(location: SourceLocation) -> Self {
        Self {
            line: location.line,
            column: location.column,
        }
    }
}

/// Error returned when parsing CSS selectors.
#[derive(Debug, Clone)]
pub struct SelectorError {
    kind: SelectorErrorKind,
    location: Option<SelectorLocation>,
    snippet: Option<String>,
}

impl SelectorError {
    fn from_parse_error(
        input: &str,
        err: cssparser::ParseError<'_, SelectorParseErrorKind<'_>>,
    ) -> Self {
        let kind = SelectorErrorKind::from_parse_error_kind(err.kind);
        let location = SelectorLocation::from(err.location);
        let snippet = snippet_for_location(input, location);
        Self {
            kind,
            location: Some(location),
            snippet,
        }
    }

    /// Returns the underlying error kind.
    pub fn kind(&self) -> &SelectorErrorKind {
        &self.kind
    }

    /// Returns the source location of the error, if available.
    pub fn location(&self) -> Option<SelectorLocation> {
        self.location
    }

    /// Returns a snippet of the selector input at the error location, if available.
    pub fn snippet(&self) -> Option<&str> {
        self.snippet.as_deref()
    }
}

impl fmt::Display for SelectorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.kind)?;
        if let Some(location) = self.location {
            write!(
                f,
                " at line {line}, column {column}",
                line = location.line,
                column = location.column
            )?;
        }
        if let Some(snippet) = &self.snippet {
            write!(f, "\n{snippet}")?;
        }
        Ok(())
    }
}

impl std::error::Error for SelectorError {}

/// Errors returned when parsing selectors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SelectorErrorKind {
    /// A token was not expected.
    UnexpectedToken(String),

    /// End-of-line was unexpected.
    EndOfLine,

    /// `@` rule is invalid.
    InvalidAtRule(String),

    /// The body of an `@` rule is invalid.
    InvalidAtRuleBody,

    /// The qualified rule is invalid.
    QualRuleInvalid,

    /// Expected a `::` for a pseudo-element.
    ExpectedColonOnPseudoElement(String),

    /// Expected an identity for a pseudo-element.
    ExpectedIdentityOnPseudoElement(String),

    /// Unexpected parser error.
    UnexpectedSelectorParseError(String),
}

impl SelectorErrorKind {
    fn from_parse_error_kind(err: ParseErrorKind<SelectorParseErrorKind<'_>>) -> Self {
        match err {
            ParseErrorKind::Basic(err) => SelectorErrorKind::from(err),
            ParseErrorKind::Custom(err) => SelectorErrorKind::from(err),
        }
    }
}

impl From<BasicParseErrorKind<'_>> for SelectorErrorKind {
    fn from(err: BasicParseErrorKind<'_>) -> Self {
        match err {
            BasicParseErrorKind::UnexpectedToken(token) => {
                Self::UnexpectedToken(render_token(&token))
            }
            BasicParseErrorKind::EndOfInput => Self::EndOfLine,
            BasicParseErrorKind::AtRuleInvalid(rule) => Self::InvalidAtRule(rule.to_string()),
            BasicParseErrorKind::AtRuleBodyInvalid => Self::InvalidAtRuleBody,
            BasicParseErrorKind::QualifiedRuleInvalid => Self::QualRuleInvalid,
        }
    }
}

impl From<SelectorParseErrorKind<'_>> for SelectorErrorKind {
    fn from(err: SelectorParseErrorKind<'_>) -> Self {
        match err {
            SelectorParseErrorKind::PseudoElementExpectedColon(token) => {
                Self::ExpectedColonOnPseudoElement(render_token(&token))
            }
            SelectorParseErrorKind::PseudoElementExpectedIdent(token) => {
                Self::ExpectedIdentityOnPseudoElement(render_token(&token))
            }
            other => Self::UnexpectedSelectorParseError(format!("{other:?}")),
        }
    }
}

impl fmt::Display for SelectorErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnexpectedToken(token) => write!(f, "Token {token:?} was not expected"),
            Self::EndOfLine => write!(f, "Unexpected EOL"),
            Self::InvalidAtRule(rule) => write!(f, "Invalid @-rule {rule:?}"),
            Self::InvalidAtRuleBody => write!(f, "The body of an @-rule was invalid"),
            Self::QualRuleInvalid => write!(f, "The qualified name was invalid"),
            Self::ExpectedColonOnPseudoElement(token) => write!(
                f,
                "Expected a ':' token for pseudoelement, got {token:?} instead"
            ),
            Self::ExpectedIdentityOnPseudoElement(token) => write!(
                f,
                "Expected identity for pseudoelement, got {token:?} instead"
            ),
            Self::UnexpectedSelectorParseError(err) => {
                write!(f, "Unexpected selector parser error: {err}")
            }
        }
    }
}
impl std::error::Error for SelectorErrorKind {}

fn render_token(token: &Token<'_>) -> String {
    match token {
        Token::Ident(ident) => ident.to_string(),
        Token::AtKeyword(value) => format!("@{value}"),
        Token::Hash(name) | Token::IDHash(name) => format!("#{name}"),
        Token::QuotedString(value) => format!("\"{value}\""),
        Token::UnquotedUrl(value) => value.to_string(),
        Token::Number {
            has_sign: signed,
            value: num,
            int_value: _,
        }
        | Token::Percentage {
            has_sign: signed,
            unit_value: num,
            int_value: _,
        } => render_number(*signed, *num, token),
        Token::Dimension {
            has_sign: signed,
            value: num,
            int_value: _,
            unit,
        } => {
            let rendered = render_int(*signed, *num);
            format!("{rendered}{unit}")
        }
        Token::WhiteSpace(_) => String::from(" "),
        Token::Comment(comment) => format!("/* {comment} */"),
        Token::Function(name) => format!("{name}()"),
        Token::BadString(string) => format!("<Bad String {string:?}>"),
        Token::BadUrl(url) => format!("<Bad URL {url:?}>"),
        Token::Colon => ":".into(),
        Token::Semicolon => ";".into(),
        Token::Comma => ",".into(),
        Token::IncludeMatch => "~=".into(),
        Token::DashMatch => "|=".into(),
        Token::PrefixMatch => "^=".into(),
        Token::SuffixMatch => "$=".into(),
        Token::SubstringMatch => "*=".into(),
        Token::CDO => "<!--".into(),
        Token::CDC => "-->".into(),
        Token::ParenthesisBlock => "<(".into(),
        Token::SquareBracketBlock => "<[".into(),
        Token::CurlyBracketBlock => "<{".into(),
        Token::CloseParenthesis => "<)".into(),
        Token::CloseSquareBracket => "<]".into(),
        Token::CloseCurlyBracket => "<}".into(),
        Token::Delim(delim) => (*delim).into(),
    }
}

fn render_number(signed: bool, num: f32, token: &Token<'_>) -> String {
    let num = render_int(signed, num);

    match token {
        Token::Number { .. } => num,
        Token::Percentage { .. } => format!("{num}%"),
        _ => unreachable!("render_number called with non-numerical token"),
    }
}

fn render_int(signed: bool, num: f32) -> String {
    if signed {
        render_int_signed(num)
    } else {
        render_int_unsigned(num)
    }
}

fn render_int_signed(num: f32) -> String {
    if num > 0.0 {
        format!("+{num}")
    } else {
        format!("-{num}")
    }
}

fn render_int_unsigned(num: f32) -> String {
    format!("{num}")
}

fn snippet_for_location(input: &str, location: SelectorLocation) -> Option<String> {
    let line_index = if location.line == 0 {
        0
    } else {
        usize::try_from(location.line).ok()?.checked_sub(1)?
    };
    let line = input.lines().nth(line_index)?;
    let column = usize::try_from(location.column).ok()?;
    let mut caret_pos = column.saturating_sub(1);
    if caret_pos > line.len() {
        caret_pos = line.len();
    }
    let mut caret = String::new();
    caret.push_str(&" ".repeat(caret_pos));
    caret.push('^');
    Some(format!("{line}\n{caret}"))
}
