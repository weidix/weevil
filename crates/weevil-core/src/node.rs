//! Node types for the HTML tree.

use std::borrow::Cow;

use html5ever::tendril::{Atomic, Tendril, fmt};
use html5ever::{LocalName, QualName};

pub type SendStrTendril = Tendril<fmt::UTF8, Atomic>;

#[derive(Clone, Debug)]
pub struct ElementAttr {
    pub name: QualName,
    pub value: SendStrTendril,
}

/// Identifier for a node in the HTML tree.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct NodeId(u32);

impl NodeId {
    #[inline]
    pub(crate) fn new(index: usize) -> Self {
        let value = u32::try_from(index).expect("html tree exceeds u32::MAX nodes");
        Self(value)
    }

    #[inline]
    pub(crate) fn index(self) -> usize {
        usize::try_from(self.0).expect("node id exceeds usize::MAX")
    }
}

/// A parsed node in the HTML tree.
#[derive(Debug)]
pub struct Node {
    pub(crate) parent: Option<NodeId>,
    pub(crate) first_child: Option<NodeId>,
    pub(crate) last_child: Option<NodeId>,
    pub(crate) prev_sibling: Option<NodeId>,
    pub(crate) next_sibling: Option<NodeId>,
    pub(crate) kind: NodeKind,
}

impl Node {
    pub(crate) fn new(kind: NodeKind) -> Self {
        Self {
            parent: None,
            first_child: None,
            last_child: None,
            prev_sibling: None,
            next_sibling: None,
            kind,
        }
    }

    /// Returns the parent node id, if any.
    pub fn parent(&self) -> Option<NodeId> {
        self.parent
    }

    /// Returns the first child node id, if any.
    pub fn first_child(&self) -> Option<NodeId> {
        self.first_child
    }

    /// Returns the last child node id, if any.
    pub fn last_child(&self) -> Option<NodeId> {
        self.last_child
    }

    /// Returns the previous sibling node id, if any.
    pub fn prev_sibling(&self) -> Option<NodeId> {
        self.prev_sibling
    }

    /// Returns the next sibling node id, if any.
    pub fn next_sibling(&self) -> Option<NodeId> {
        self.next_sibling
    }

    /// Returns the node kind.
    pub fn kind(&self) -> &NodeKind {
        &self.kind
    }

    /// Returns the attribute value if this is an element node.
    pub fn attr(&self, name: &str) -> Option<&str> {
        self.kind.as_element()?.attr_value(name)
    }

    /// Returns the text contents if this is a text node.
    pub fn text(&self) -> Option<&str> {
        match &self.kind {
            NodeKind::Text(text) => Some(text.as_ref()),
            _ => None,
        }
    }
}

/// Node contents.
#[derive(Debug)]
pub enum NodeKind {
    Document,
    Doctype {
        name: SendStrTendril,
        public_id: SendStrTendril,
        system_id: SendStrTendril,
    },
    Text(SendStrTendril),
    Comment(SendStrTendril),
    Element(ElementData),
    ProcessingInstruction {
        target: SendStrTendril,
        data: SendStrTendril,
    },
}

impl NodeKind {
    /// Returns the element data if this is an element.
    pub fn as_element(&self) -> Option<&ElementData> {
        match self {
            NodeKind::Element(data) => Some(data),
            _ => None,
        }
    }
}

/// Element data for an HTML node.
#[derive(Debug)]
pub struct ElementData {
    pub name: QualName,
    pub attrs: Vec<ElementAttr>,
    pub template_contents: Option<NodeId>,
    pub mathml_annotation_xml_integration_point: bool,
}

impl ElementData {
    /// Returns the element local tag name (e.g., "div").
    pub fn tag_name(&self) -> &str {
        self.name.local.as_ref()
    }

    /// Returns the element namespace URL.
    pub fn namespace(&self) -> &str {
        self.name.ns.as_ref()
    }

    /// Returns true if the element local name matches the provided name.
    pub fn is_tag(&self, name: &str) -> bool {
        let normalized = normalize_tag_name(name);
        self.name.local.as_ref() == normalized.as_ref()
    }

    /// Returns the attribute value for a name, if present.
    pub fn attr_value(&self, name: &str) -> Option<&str> {
        let normalized = normalize_tag_name(name);
        let local = LocalName::from(normalized.as_ref());
        self.attrs
            .iter()
            .find(|attr| attr.name.local == local)
            .map(|attr| attr.value.as_ref())
    }

    /// Alias for [`ElementData::attr_value`].
    pub fn attr(&self, name: &str) -> Option<&str> {
        self.attr_value(name)
    }

    /// Returns true if the element has the attribute name.
    pub fn has_attr(&self, name: &str) -> bool {
        self.attr_value(name).is_some()
    }

    /// Returns the id attribute value if present.
    pub fn id(&self) -> Option<&str> {
        self.attr_value("id")
    }

    /// Returns the class attribute value if present.
    pub fn class_value(&self) -> Option<&str> {
        self.attr_value("class")
    }

    /// Returns an iterator of attribute local names.
    pub fn attr_names(&self) -> impl Iterator<Item = &str> + '_ {
        self.attrs.iter().map(|attr| attr.name.local.as_ref())
    }

    /// Returns an iterator of attribute name/value pairs.
    pub fn attrs(&self) -> impl Iterator<Item = (&str, &str)> + '_ {
        self.attrs
            .iter()
            .map(|attr| (attr.name.local.as_ref(), attr.value.as_ref()))
    }

    /// Returns an iterator of whitespace-separated tokens for an attribute.
    pub fn attr_tokens(&self, name: &str) -> impl Iterator<Item = &str> + '_ {
        self.attr_value(name)
            .into_iter()
            .flat_map(|value| value.split_ascii_whitespace())
    }

    /// Returns an iterator of classes from the class attribute.
    pub fn classes(&self) -> impl Iterator<Item = &str> + '_ {
        self.attr_tokens("class")
    }

    /// Returns the class list as a vector.
    pub fn class_list(&self) -> Vec<&str> {
        self.classes().collect()
    }

    /// Returns true if the element class list contains the provided class.
    pub fn has_class(&self, name: &str) -> bool {
        self.classes().any(|class| class == name)
    }

    /// Returns the data-* attribute value for the provided key.
    pub fn data(&self, key: &str) -> Option<&str> {
        let name = format!("data-{key}");
        self.attr_value(&name)
    }

    /// Returns the aria-* attribute value for the provided key.
    pub fn aria(&self, key: &str) -> Option<&str> {
        let name = format!("aria-{key}");
        self.attr_value(&name)
    }
}

pub(crate) fn normalize_tag_name(name: &str) -> Cow<'_, str> {
    if name.bytes().any(|byte| byte.is_ascii_uppercase()) {
        Cow::Owned(name.to_ascii_lowercase())
    } else {
        Cow::Borrowed(name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use html5ever::{LocalName, QualName, local_name, ns};

    fn make_attr(name: &str, value: &str) -> ElementAttr {
        ElementAttr {
            name: QualName::new(None, ns!(), LocalName::from(name)),
            value: SendStrTendril::from(value),
        }
    }

    #[test]
    fn node_id_round_trip() {
        let id = NodeId::new(42);
        assert_eq!(id.index(), 42);
    }

    #[cfg(target_pointer_width = "64")]
    #[test]
    #[should_panic(expected = "html tree exceeds u32::MAX nodes")]
    fn node_id_panics_on_overflow() {
        let _ = NodeId::new(u32::MAX as usize + 1);
    }

    #[test]
    fn node_kind_as_element() {
        let data = ElementData {
            name: QualName::new(None, ns!(html), local_name!("div")),
            attrs: Vec::new(),
            template_contents: None,
            mathml_annotation_xml_integration_point: false,
        };
        let element_kind = NodeKind::Element(data);
        assert!(element_kind.as_element().is_some());

        let text_kind = NodeKind::Text(SendStrTendril::from("hi"));
        assert!(text_kind.as_element().is_none());
    }

    #[test]
    fn element_attr_value_normalizes_name() {
        let data = ElementData {
            name: QualName::new(None, ns!(html), local_name!("div")),
            attrs: vec![make_attr("id", "Hero"), make_attr("class", "a b")],
            template_contents: None,
            mathml_annotation_xml_integration_point: false,
        };

        assert_eq!(data.attr_value("ID"), Some("Hero"));
        assert_eq!(data.attr_value("class"), Some("a b"));
        assert_eq!(data.attr_value("missing"), None);
    }

    #[test]
    fn element_tag_helpers() {
        let data = ElementData {
            name: QualName::new(None, ns!(html), local_name!("div")),
            attrs: Vec::new(),
            template_contents: None,
            mathml_annotation_xml_integration_point: false,
        };

        assert_eq!(data.tag_name(), "div");
        assert_eq!(data.namespace(), ns!(html).as_ref());
        assert!(data.is_tag("DIV"));
        assert!(!data.is_tag("span"));
    }

    #[test]
    fn element_attr_helpers() {
        let data = ElementData {
            name: QualName::new(None, ns!(html), local_name!("div")),
            attrs: vec![
                make_attr("id", "hero"),
                make_attr("class", "  a   b  "),
                make_attr("data-role", "main"),
                make_attr("aria-label", "Hero"),
                make_attr("rel", "nofollow noopener"),
            ],
            template_contents: None,
            mathml_annotation_xml_integration_point: false,
        };

        assert_eq!(data.attr("ID"), Some("hero"));
        assert!(data.has_attr("data-role"));
        assert_eq!(data.id(), Some("hero"));
        assert_eq!(data.class_value(), Some("  a   b  "));

        let classes: Vec<_> = data.classes().collect();
        assert_eq!(classes, vec!["a", "b"]);
        assert_eq!(data.class_list(), vec!["a", "b"]);
        assert!(data.has_class("b"));
        assert!(!data.has_class("c"));

        assert_eq!(data.data("role"), Some("main"));
        assert_eq!(data.aria("label"), Some("Hero"));

        let rel: Vec<_> = data.attr_tokens("rel").collect();
        assert_eq!(rel, vec!["nofollow", "noopener"]);

        let mut names: Vec<_> = data.attr_names().collect();
        names.sort_unstable();
        assert_eq!(names, vec!["aria-label", "class", "data-role", "id", "rel"]);

        let mut attrs: Vec<_> = data.attrs().collect();
        attrs.sort_by(|left, right| left.0.cmp(right.0));
        assert_eq!(
            attrs,
            vec![
                ("aria-label", "Hero"),
                ("class", "  a   b  "),
                ("data-role", "main"),
                ("id", "hero"),
                ("rel", "nofollow noopener"),
            ]
        );
    }

    #[test]
    fn normalize_tag_name_tracks_case() {
        let lower = normalize_tag_name("div");
        assert!(matches!(lower, Cow::Borrowed("div")));

        let mixed = normalize_tag_name("DiV");
        assert!(matches!(mixed, Cow::Owned(ref val) if val == "div"));
    }

    #[test]
    fn node_attr_and_text_helpers() {
        let data = ElementData {
            name: QualName::new(None, ns!(html), local_name!("div")),
            attrs: vec![make_attr("id", "Hero")],
            template_contents: None,
            mathml_annotation_xml_integration_point: false,
        };
        let element_node = Node::new(NodeKind::Element(data));
        assert_eq!(element_node.attr("id"), Some("Hero"));
        assert_eq!(element_node.attr("class"), None);

        let text_node = Node::new(NodeKind::Text(SendStrTendril::from("hi")));
        assert_eq!(text_node.text(), Some("hi"));
        assert_eq!(text_node.attr("id"), None);
    }
}
