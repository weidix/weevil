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
