use super::*;
use html5ever::local_name;

fn parse(html: &str) -> HtmlTree {
    HtmlTree::parse(html)
}

#[test]
fn parse_builds_fast_indexes() {
    let html = r#"<html><body><div id="hero" class="a b"></div></body></html>"#;
    let tree = parse(html);
    let index = tree.index();

    let hero = index.by_id("hero").expect("id not indexed");
    let hero_node = tree.node(hero);
    match hero_node.kind() {
        NodeKind::Element(data) => assert_eq!(data.name.local, local_name!("div")),
        _ => panic!("expected element"),
    }

    let class_a = index.by_class("a").expect("class a missing");
    assert!(class_a.contains(&hero));
    let class_b = index.by_class("b").expect("class b missing");
    assert!(class_b.contains(&hero));

    let divs = index.by_tag("DIV").expect("div index missing");
    assert!(divs.contains(&hero));
}

#[test]
fn siblings_and_parents_are_linked() {
    let html = r#"<div><span></span><em></em></div>"#;
    let tree = parse(html);
    let div_id = tree.index().by_tag("div").unwrap()[0];
    let mut children = tree.children(div_id);
    let span = children.next().expect("missing span");
    let em = children.next().expect("missing em");
    assert!(children.next().is_none());

    assert_eq!(tree.node(span).parent(), Some(div_id));
    assert_eq!(tree.node(em).parent(), Some(div_id));
    assert_eq!(tree.node(span).next_sibling(), Some(em));
    assert_eq!(tree.node(em).prev_sibling(), Some(span));

    match tree.node(span).kind() {
        NodeKind::Element(data) => assert_eq!(data.name.local, local_name!("span")),
        _ => panic!("expected span"),
    }
    match tree.node(em).kind() {
        NodeKind::Element(data) => assert_eq!(data.name.local, local_name!("em")),
        _ => panic!("expected em"),
    }
}

#[test]
fn class_index_splits_tokens() {
    let html = r#"<div class="  a   b  "/>"#;
    let tree = parse(html);
    let index = tree.index();
    assert!(index.by_class("a").is_some());
    assert!(index.by_class("b").is_some());
}

#[test]
fn text_content_aggregates_text_nodes() {
    let html = r#"<div id="root">hi<span>there</span>!</div>"#;
    let tree = parse(html);
    let root = tree.index().by_id("root").expect("missing root");
    assert_eq!(tree.text_content(root), "hithere!");
    assert_eq!(tree.attr(root, "id"), Some("root"));
    assert_eq!(tree.attr(root, "missing"), None);
}

#[test]
fn subtree_and_descendants_match_document_order() {
    let html = r#"<div id="root"><span id="child"></span><em></em></div>"#;
    let tree = parse(html);
    let root = tree.index().by_id("root").expect("missing root");

    let mut subtree = tree.subtree(root);
    assert_eq!(subtree.next(), Some(root));
    let subtree_tail: Vec<NodeId> = subtree.collect();

    let descendants: Vec<NodeId> = tree.descendants(root).collect();
    assert_eq!(descendants, subtree_tail);
}

#[test]
fn parse_with_errors_reports_issues() {
    let html = "<div>\u{0}</div>";
    let output = HtmlTree::parse_with_errors(html);
    assert!(!output.errors.is_empty());

    let err = HtmlTree::parse_checked(html).unwrap_err();
    assert!(!err.errors().is_empty());
}

#[test]
fn html_and_text_helpers_render_content() {
    let html = r#"<div id="root"><span>hi</span><!--c--></div>"#;
    let tree = parse(html);
    let root = tree.index().by_id("root").expect("missing root");

    assert_eq!(tree.html(root), "<span>hi</span><!--c-->");
    assert_eq!(
        tree.outer_html(root),
        r#"<div id="root"><span>hi</span><!--c--></div>"#
    );
    assert_eq!(tree.text(root), "hi");
}
