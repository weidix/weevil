//! Node types for the HTML tree.

use std::borrow::Cow;

use html5ever::tendril::StrTendril;
use html5ever::{Attribute, LocalName, QualName};

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
        name: StrTendril,
        public_id: StrTendril,
        system_id: StrTendril,
    },
    Text(StrTendril),
    Comment(StrTendril),
    Element(ElementData),
    ProcessingInstruction {
        target: StrTendril,
        data: StrTendril,
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
    pub attrs: Vec<Attribute>,
    pub template_contents: Option<NodeId>,
    pub mathml_annotation_xml_integration_point: bool,
}

impl ElementData {
    /// Returns the attribute value for a name, if present.
    pub fn attr_value(&self, name: &str) -> Option<&str> {
        let normalized = normalize_tag_name(name);
        let local = LocalName::from(normalized.as_ref());
        self.attrs
            .iter()
            .find(|attr| attr.name.local == local)
            .map(|attr| attr.value.as_ref())
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
    use html5ever::{Attribute, LocalName, QualName, local_name, ns};

    fn make_attr(name: &str, value: &str) -> Attribute {
        Attribute {
            name: QualName::new(None, ns!(), LocalName::from(name)),
            value: value.into(),
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

        let text_kind = NodeKind::Text(StrTendril::from("hi"));
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

        let text_node = Node::new(NodeKind::Text(StrTendril::from("hi")));
        assert_eq!(text_node.text(), Some("hi"));
        assert_eq!(text_node.attr("id"), None);
    }
}
