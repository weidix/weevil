use super::*;
use cssparser::CowRcStr;
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
    let message = err.to_string();
    assert!(message.contains("Token \"@\" was not expected"));
    assert!(message.contains("line"));
}

#[test]
fn selector_error_includes_location_and_snippet() {
    let err = Selector::parse("div span@").unwrap_err();
    let _location = err.location().expect("missing location");
    let snippet = err.snippet().expect("missing snippet");
    assert!(snippet.contains("div span@"));
    assert!(snippet.contains('^'));
}

#[test]
fn css_string_and_local_name_rendering() {
    let css = CssString::from("a b").to_css_string();
    assert!(css.starts_with('\"'));
    assert!(css.ends_with('\"'));
    assert!(css.contains("a b"));

    let name = CssLocalName::from("div");
    assert_eq!(name.to_css_string(), "div");
}

#[test]
fn render_token_numbers_and_units() {
    let token = Token::Number {
        has_sign: true,
        value: 3.0,
        int_value: Some(3),
    };
    assert_eq!(render_token(&token), "+3");

    let token = Token::Percentage {
        has_sign: true,
        unit_value: 1.0,
        int_value: Some(100),
    };
    assert_eq!(render_token(&token), "+1%");

    let token = Token::Dimension {
        has_sign: false,
        value: 12.0,
        int_value: Some(12),
        unit: CowRcStr::from("px"),
    };
    assert_eq!(render_token(&token), "12px");
}

#[test]
fn render_token_misc_variants() {
    let token = Token::Ident(CowRcStr::from("title"));
    assert_eq!(render_token(&token), "title");

    let token = Token::AtKeyword(CowRcStr::from("media"));
    assert_eq!(render_token(&token), "@media");

    let token = Token::Hash(CowRcStr::from("hero"));
    assert_eq!(render_token(&token), "#hero");

    let token = Token::QuotedString(CowRcStr::from("hi"));
    assert_eq!(render_token(&token), "\"hi\"");

    let token = Token::Comment("note");
    assert_eq!(render_token(&token), "/* note */");

    let token = Token::Function(CowRcStr::from("rgb"));
    assert_eq!(render_token(&token), "rgb()");
}

#[test]
fn render_int_helpers() {
    assert_eq!(render_int_signed(1.0), "+1");
    assert_eq!(render_int_signed(0.0), "-0");
    assert_eq!(render_int_unsigned(2.0), "2");
}

#[test]
fn selector_error_messages_cover_variants() {
    let err = SelectorErrorKind::EndOfLine.to_string();
    assert!(err.contains("EOL"));

    let err = SelectorErrorKind::InvalidAtRule("media".to_string()).to_string();
    assert!(err.contains("Invalid @-rule"));

    let err = SelectorErrorKind::InvalidAtRuleBody.to_string();
    assert!(err.contains("body of an @-rule"));

    let err = SelectorErrorKind::QualRuleInvalid.to_string();
    assert!(err.contains("qualified name"));
}
