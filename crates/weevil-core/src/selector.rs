//! CSS selector parsing for weevil core.

use std::convert::TryFrom;
use std::fmt;

pub use cssparser::ToCss;
use cssparser::{BasicParseErrorKind, ParseErrorKind, Token};
use html5ever::{LocalName, Namespace};
use precomputed_hash::PrecomputedHash;
use selectors::parser::{self, ParseRelative, SelectorList, SelectorParseErrorKind};

/// Wrapper around CSS selectors.
///
/// Represents a comma-separated list of selectors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Selector {
    selectors: SelectorList<Simple>,
}

impl Selector {
    /// Parses a CSS selector group.
    pub fn parse(selectors: &str) -> Result<Self, SelectorErrorKind<'_>> {
        let mut parser_input = cssparser::ParserInput::new(selectors);
        let mut parser = cssparser::Parser::new(&mut parser_input);

        SelectorList::parse(&Parser, &mut parser, ParseRelative::No)
            .map(|selectors| Self { selectors })
            .map_err(SelectorErrorKind::from)
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
    type Error = SelectorErrorKind<'i>;

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

/// Errors returned when parsing selectors.
#[derive(Debug, Clone)]
pub enum SelectorErrorKind<'a> {
    /// A token was not expected.
    UnexpectedToken(Token<'a>),

    /// End-of-line was unexpected.
    EndOfLine,

    /// `@` rule is invalid.
    InvalidAtRule(String),

    /// The body of an `@` rule is invalid.
    InvalidAtRuleBody,

    /// The qualified rule is invalid.
    QualRuleInvalid,

    /// Expected a `::` for a pseudo-element.
    ExpectedColonOnPseudoElement(Token<'a>),

    /// Expected an identity for a pseudo-element.
    ExpectedIdentityOnPseudoElement(Token<'a>),

    /// Unexpected parser error.
    UnexpectedSelectorParseError(SelectorParseErrorKind<'a>),
}

impl<'a> From<cssparser::ParseError<'a, SelectorParseErrorKind<'a>>> for SelectorErrorKind<'a> {
    fn from(original: cssparser::ParseError<'a, SelectorParseErrorKind<'a>>) -> Self {
        match original.kind {
            ParseErrorKind::Basic(err) => SelectorErrorKind::from(err),
            ParseErrorKind::Custom(err) => SelectorErrorKind::from(err),
        }
    }
}

impl<'a> From<BasicParseErrorKind<'a>> for SelectorErrorKind<'a> {
    fn from(err: BasicParseErrorKind<'a>) -> Self {
        match err {
            BasicParseErrorKind::UnexpectedToken(token) => Self::UnexpectedToken(token),
            BasicParseErrorKind::EndOfInput => Self::EndOfLine,
            BasicParseErrorKind::AtRuleInvalid(rule) => Self::InvalidAtRule(rule.to_string()),
            BasicParseErrorKind::AtRuleBodyInvalid => Self::InvalidAtRuleBody,
            BasicParseErrorKind::QualifiedRuleInvalid => Self::QualRuleInvalid,
        }
    }
}

impl<'a> From<SelectorParseErrorKind<'a>> for SelectorErrorKind<'a> {
    fn from(err: SelectorParseErrorKind<'a>) -> Self {
        match err {
            SelectorParseErrorKind::PseudoElementExpectedColon(token) => {
                Self::ExpectedColonOnPseudoElement(token)
            }
            SelectorParseErrorKind::PseudoElementExpectedIdent(token) => {
                Self::ExpectedIdentityOnPseudoElement(token)
            }
            other => Self::UnexpectedSelectorParseError(other),
        }
    }
}

impl fmt::Display for SelectorErrorKind<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnexpectedToken(token) => {
                let rendered = render_token(token);
                write!(f, "Token {rendered:?} was not expected")
            }
            Self::EndOfLine => write!(f, "Unexpected EOL"),
            Self::InvalidAtRule(rule) => write!(f, "Invalid @-rule {rule:?}"),
            Self::InvalidAtRuleBody => write!(f, "The body of an @-rule was invalid"),
            Self::QualRuleInvalid => write!(f, "The qualified name was invalid"),
            Self::ExpectedColonOnPseudoElement(token) => {
                let rendered = render_token(token);
                write!(
                    f,
                    "Expected a ':' token for pseudoelement, got {rendered:?} instead"
                )
            }
            Self::ExpectedIdentityOnPseudoElement(token) => {
                let rendered = render_token(token);
                write!(
                    f,
                    "Expected identity for pseudoelement, got {rendered:?} instead"
                )
            }
            Self::UnexpectedSelectorParseError(err) => write!(
                f,
                "Unexpected error occurred. Please report this to the developer\n{err:#?}"
            ),
        }
    }
}

impl std::error::Error for SelectorErrorKind<'_> {}

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

#[cfg(test)]
mod tests {
    use super::*;
    use std::convert::TryInto;

    #[test]
    fn parse_simple_selector() {
        let selector = Selector::parse("h1.foo").unwrap();
        assert_eq!(selector.to_css_string(), "h1.foo");
    }

    #[test]
    fn parse_selector_group() {
        let selector = Selector::parse("h1, h2, h3").unwrap();
        let css = selector.to_css_string();
        assert!(css.contains("h1"));
        assert!(css.contains("h2"));
        assert!(css.contains("h3"));
    }

    #[test]
    fn selector_conversions() {
        let s = "#testid.testclass";
        let _sel: Selector = s.try_into().unwrap();

        let s = s.to_owned();
        let _sel: Selector = (*s).try_into().unwrap();
    }

    #[test]
    fn invalid_selector_conversions() {
        let s = "<failing selector>";
        assert!(Selector::parse(s).is_err());
    }

    #[test]
    fn has_is_where_selectors() {
        let has = Selector::parse(":has(a)");
        let is = Selector::parse(":is(a)");
        let where_ = Selector::parse(":where(a)");

        assert!(has.is_ok());
        assert!(is.is_ok());
        assert!(where_.is_ok());
    }

    #[test]
    fn error_message_includes_token() {
        let err = Selector::parse("div138293@!#@!!@#").unwrap_err();
        assert_eq!(err.to_string(), "Token \"@\" was not expected");
    }
}
