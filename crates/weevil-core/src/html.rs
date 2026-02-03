//! High-performance HTML tree parsing and indexing.

use html5ever::interface::tree_builder::{ElementFlags, NodeOrText, QuirksMode, TreeSink};
use html5ever::tendril::{StrTendril, TendrilSink};
use html5ever::{Attribute, LocalName, ParseOpts, QualName, local_name, ns, parse_document};
use rustc_hash::FxHashMap;
use std::borrow::Cow;
use std::cell::{Cell, RefCell, UnsafeCell};

/// Identifier for a node in the HTML tree.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct NodeId(u32);

impl NodeId {
    #[inline]
    fn new(index: usize) -> Self {
        let value = u32::try_from(index).expect("html tree exceeds u32::MAX nodes");
        Self(value)
    }

    #[inline]
    fn index(self) -> usize {
        usize::try_from(self.0).expect("node id exceeds usize::MAX")
    }
}

/// DOM-like HTML tree with fast tag/id/class indexes.
#[derive(Debug)]
pub struct HtmlTree {
    nodes: Vec<Node>,
    document: NodeId,
    quirks_mode: QuirksMode,
    index: HtmlIndex,
}

impl HtmlTree {
    /// Parses UTF-8 HTML into a tree.
    pub fn parse(html: &str) -> Self {
        Self::parse_bytes(html.as_bytes())
    }

    /// Parses UTF-8 HTML from bytes into a tree.
    pub fn parse_bytes(bytes: &[u8]) -> Self {
        let sink = HtmlTreeBuilder::with_capacity(bytes.len());
        parse_document(sink, ParseOpts::default())
            .from_utf8()
            .one(bytes)
    }

    /// Returns the document node id.
    pub fn document(&self) -> NodeId {
        self.document
    }

    /// Returns the quirks mode chosen by the parser.
    pub fn quirks_mode(&self) -> QuirksMode {
        self.quirks_mode
    }

    /// Returns a node by id.
    pub fn node(&self, id: NodeId) -> &Node {
        &self.nodes[id.index()]
    }

    /// Returns the index for fast lookups.
    pub fn index(&self) -> &HtmlIndex {
        &self.index
    }

    /// Returns the first element child under the document node.
    pub fn root_element(&self) -> Option<NodeId> {
        let mut current = self.node(self.document).first_child;
        while let Some(node_id) = current {
            if matches!(self.node(node_id).kind(), NodeKind::Element(_)) {
                return Some(node_id);
            }
            current = self.node(node_id).next_sibling;
        }
        None
    }

    /// Iterates over the direct children of a node in document order.
    pub fn children(&self, id: NodeId) -> Children<'_> {
        Children {
            tree: self,
            next: self.node(id).first_child,
        }
    }
}

/// Iterator over child nodes.
pub struct Children<'a> {
    tree: &'a HtmlTree,
    next: Option<NodeId>,
}

impl<'a> Iterator for Children<'a> {
    type Item = NodeId;

    fn next(&mut self) -> Option<Self::Item> {
        let current = self.next?;
        self.next = self.tree.node(current).next_sibling;
        Some(current)
    }
}

/// A parsed node in the HTML tree.
#[derive(Debug)]
pub struct Node {
    parent: Option<NodeId>,
    first_child: Option<NodeId>,
    last_child: Option<NodeId>,
    prev_sibling: Option<NodeId>,
    next_sibling: Option<NodeId>,
    kind: NodeKind,
}

impl Node {
    fn new(kind: NodeKind) -> Self {
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

/// Fast tag/id/class index for a parsed HTML tree.
#[derive(Debug, Default)]
pub struct HtmlIndex {
    by_id: FxHashMap<Box<str>, NodeId>,
    by_tag: FxHashMap<LocalName, Vec<NodeId>>,
    by_class: FxHashMap<Box<str>, Vec<NodeId>>,
}

impl HtmlIndex {
    fn with_capacity(estimated_nodes: usize) -> Self {
        Self {
            by_id: FxHashMap::with_capacity_and_hasher(estimated_nodes / 8, Default::default()),
            by_tag: FxHashMap::with_capacity_and_hasher(estimated_nodes / 2, Default::default()),
            by_class: FxHashMap::with_capacity_and_hasher(estimated_nodes / 4, Default::default()),
        }
    }

    /// Looks up a node by id attribute.
    pub fn by_id(&self, id: &str) -> Option<NodeId> {
        self.by_id.get(id).copied()
    }

    /// Looks up nodes by a tag name (case-insensitive for ASCII).
    pub fn by_tag(&self, name: &str) -> Option<&[NodeId]> {
        let normalized = normalize_tag_name(name);
        let local = LocalName::from(normalized.as_ref());
        self.by_tag.get(&local).map(Vec::as_slice)
    }

    /// Looks up nodes by class name.
    pub fn by_class(&self, class: &str) -> Option<&[NodeId]> {
        self.by_class.get(class).map(Vec::as_slice)
    }

    /// Looks up nodes by an interned LocalName without extra allocations.
    pub fn by_tag_local(&self, name: &LocalName) -> Option<&[NodeId]> {
        self.by_tag.get(name).map(Vec::as_slice)
    }
}

struct HtmlTreeBuilder {
    // UnsafeCell enables &self mutation required by TreeSink with minimal overhead.
    nodes: UnsafeCell<Vec<Node>>,
    index: RefCell<HtmlIndex>,
    document: NodeId,
    quirks_mode: Cell<QuirksMode>,
}

impl HtmlTreeBuilder {
    fn with_capacity(byte_len: usize) -> Self {
        let estimated_nodes = (byte_len / 32).max(16);
        let mut nodes = Vec::with_capacity(estimated_nodes);
        let document = NodeId::new(0);
        nodes.push(Node::new(NodeKind::Document));
        Self {
            nodes: UnsafeCell::new(nodes),
            index: RefCell::new(HtmlIndex::with_capacity(estimated_nodes)),
            document,
            quirks_mode: Cell::new(QuirksMode::NoQuirks),
        }
    }

    #[inline]
    fn nodes(&self) -> &Vec<Node> {
        unsafe { &*self.nodes.get() }
    }

    #[inline]
    fn nodes_mut(&self) -> &mut Vec<Node> {
        unsafe { &mut *self.nodes.get() }
    }

    fn push_node(&self, kind: NodeKind) -> NodeId {
        let nodes = self.nodes_mut();
        let id = NodeId::new(nodes.len());
        nodes.push(Node::new(kind));
        id
    }

    fn index_element(&self, id: NodeId) {
        let nodes = self.nodes();
        let node = &nodes[id.index()];
        let element = match &node.kind {
            NodeKind::Element(data) => data,
            _ => return,
        };

        let mut index = self.index.borrow_mut();
        index
            .by_tag
            .entry(element.name.local.clone())
            .or_default()
            .push(id);
        for attr in &element.attrs {
            index_attr(&mut index, id, attr);
        }
    }
}

impl TreeSink for HtmlTreeBuilder {
    type Handle = NodeId;
    type Output = HtmlTree;
    type ElemName<'a>
        = &'a QualName
    where
        Self: 'a;

    fn finish(self) -> Self::Output {
        HtmlTree {
            nodes: self.nodes.into_inner(),
            document: self.document,
            quirks_mode: self.quirks_mode.get(),
            index: self.index.into_inner(),
        }
    }

    fn parse_error(&self, _msg: Cow<'static, str>) {}

    fn get_document(&self) -> Self::Handle {
        self.document
    }

    fn elem_name<'a>(&'a self, target: &'a Self::Handle) -> Self::ElemName<'a> {
        let nodes = self.nodes();
        match &nodes[target.index()].kind {
            NodeKind::Element(data) => &data.name,
            _ => panic!("not an element"),
        }
    }

    fn create_element(
        &self,
        name: QualName,
        attrs: Vec<Attribute>,
        flags: ElementFlags,
    ) -> Self::Handle {
        let template_contents = if flags.template {
            Some(self.push_node(NodeKind::Document))
        } else {
            None
        };

        let data = ElementData {
            name,
            attrs,
            template_contents,
            mathml_annotation_xml_integration_point: flags.mathml_annotation_xml_integration_point,
        };
        let id = self.push_node(NodeKind::Element(data));
        self.index_element(id);
        id
    }

    fn create_comment(&self, text: StrTendril) -> Self::Handle {
        self.push_node(NodeKind::Comment(text))
    }

    fn create_pi(&self, target: StrTendril, data: StrTendril) -> Self::Handle {
        self.push_node(NodeKind::ProcessingInstruction { target, data })
    }

    fn append(&self, parent: &Self::Handle, child: NodeOrText<Self::Handle>) {
        let nodes = self.nodes_mut();
        append_child(nodes, *parent, child);
    }

    fn append_based_on_parent_node(
        &self,
        element: &Self::Handle,
        prev_element: &Self::Handle,
        child: NodeOrText<Self::Handle>,
    ) {
        let nodes = self.nodes_mut();
        if nodes[element.index()].parent.is_some() {
            append_before_sibling(nodes, *element, child);
        } else {
            append_child(nodes, *prev_element, child);
        }
    }

    fn append_doctype_to_document(
        &self,
        name: StrTendril,
        public_id: StrTendril,
        system_id: StrTendril,
    ) {
        let nodes = self.nodes_mut();
        let doctype = push_node(
            nodes,
            NodeKind::Doctype {
                name,
                public_id,
                system_id,
            },
        );
        append_node(nodes, self.document, doctype);
    }

    fn add_attrs_if_missing(&self, target: &Self::Handle, attrs: Vec<Attribute>) {
        let nodes = self.nodes_mut();
        let node = &mut nodes[target.index()];
        let element = match &mut node.kind {
            NodeKind::Element(data) => data,
            _ => panic!("not an element"),
        };

        if attrs.is_empty() {
            return;
        }

        let mut index = self.index.borrow_mut();
        for attr in attrs {
            if element
                .attrs
                .iter()
                .any(|existing| existing.name == attr.name)
            {
                continue;
            }
            index_attr(&mut index, *target, &attr);
            element.attrs.push(attr);
        }
    }

    fn remove_from_parent(&self, target: &Self::Handle) {
        let nodes = self.nodes_mut();
        detach_node(nodes, *target);
    }

    fn reparent_children(&self, node: &Self::Handle, new_parent: &Self::Handle) {
        let nodes = self.nodes_mut();
        let mut child = nodes[node.index()].first_child;
        while let Some(child_id) = child {
            let next = nodes[child_id.index()].next_sibling;
            append_node(nodes, *new_parent, child_id);
            child = next;
        }
    }

    fn set_quirks_mode(&self, mode: QuirksMode) {
        self.quirks_mode.set(mode);
    }

    fn get_template_contents(&self, target: &Self::Handle) -> Self::Handle {
        let nodes = self.nodes();
        match &nodes[target.index()].kind {
            NodeKind::Element(data) => data.template_contents.expect("template contents missing"),
            _ => panic!("not a template element"),
        }
    }

    fn same_node(&self, x: &Self::Handle, y: &Self::Handle) -> bool {
        x == y
    }

    fn append_before_sibling(&self, sibling: &Self::Handle, child: NodeOrText<Self::Handle>) {
        let nodes = self.nodes_mut();
        append_before_sibling(nodes, *sibling, child);
    }

    fn is_mathml_annotation_xml_integration_point(&self, handle: &Self::Handle) -> bool {
        let nodes = self.nodes();
        match &nodes[handle.index()].kind {
            NodeKind::Element(data) => data.mathml_annotation_xml_integration_point,
            _ => panic!("not an element"),
        }
    }
}

fn push_node(nodes: &mut Vec<Node>, kind: NodeKind) -> NodeId {
    let id = NodeId::new(nodes.len());
    nodes.push(Node::new(kind));
    id
}

fn append_child(nodes: &mut Vec<Node>, parent: NodeId, child: NodeOrText<NodeId>) {
    match child {
        NodeOrText::AppendText(text) => {
            let last_child = nodes[parent.index()].last_child;
            if append_text_to_previous(nodes, last_child, &text) {
                return;
            }
            let text_id = push_node(nodes, NodeKind::Text(text));
            append_node(nodes, parent, text_id);
        }
        NodeOrText::AppendNode(node) => append_node(nodes, parent, node),
    }
}

fn append_before_sibling(nodes: &mut Vec<Node>, sibling: NodeId, child: NodeOrText<NodeId>) {
    match child {
        NodeOrText::AppendText(text) => {
            let prev = nodes[sibling.index()].prev_sibling;
            if append_text_to_previous(nodes, prev, &text) {
                return;
            }
            let text_id = push_node(nodes, NodeKind::Text(text));
            insert_before(nodes, sibling, text_id);
        }
        NodeOrText::AppendNode(node) => insert_before(nodes, sibling, node),
    }
}

fn append_text_to_previous(
    nodes: &mut Vec<Node>,
    previous: Option<NodeId>,
    text: &StrTendril,
) -> bool {
    let Some(prev_id) = previous else {
        return false;
    };

    match &mut nodes[prev_id.index()].kind {
        NodeKind::Text(existing) => {
            existing.push_tendril(text);
            true
        }
        _ => false,
    }
}

fn append_node(nodes: &mut Vec<Node>, parent: NodeId, child: NodeId) {
    detach_node(nodes, child);

    let parent_idx = parent.index();
    let child_idx = child.index();
    let last_child = nodes[parent_idx].last_child;

    nodes[child_idx].parent = Some(parent);
    nodes[child_idx].prev_sibling = last_child;
    nodes[child_idx].next_sibling = None;

    if let Some(last_id) = last_child {
        nodes[last_id.index()].next_sibling = Some(child);
    } else {
        nodes[parent_idx].first_child = Some(child);
    }
    nodes[parent_idx].last_child = Some(child);
}

fn insert_before(nodes: &mut Vec<Node>, sibling: NodeId, new_node: NodeId) {
    detach_node(nodes, new_node);

    let sibling_idx = sibling.index();
    let new_idx = new_node.index();
    let parent = nodes[sibling_idx].parent;
    let prev = nodes[sibling_idx].prev_sibling;

    nodes[new_idx].parent = parent;
    nodes[new_idx].next_sibling = Some(sibling);
    nodes[new_idx].prev_sibling = prev;

    if let Some(prev_id) = prev {
        nodes[prev_id.index()].next_sibling = Some(new_node);
    } else if let Some(parent_id) = parent {
        nodes[parent_id.index()].first_child = Some(new_node);
    }

    nodes[sibling_idx].prev_sibling = Some(new_node);
}

fn detach_node(nodes: &mut Vec<Node>, target: NodeId) {
    let idx = target.index();
    let parent = nodes[idx].parent;
    let prev = nodes[idx].prev_sibling;
    let next = nodes[idx].next_sibling;

    nodes[idx].parent = None;
    nodes[idx].prev_sibling = None;
    nodes[idx].next_sibling = None;

    if let Some(next_id) = next {
        nodes[next_id.index()].prev_sibling = prev;
    } else if let Some(parent_id) = parent {
        nodes[parent_id.index()].last_child = prev;
    }

    if let Some(prev_id) = prev {
        nodes[prev_id.index()].next_sibling = next;
    } else if let Some(parent_id) = parent {
        nodes[parent_id.index()].first_child = next;
    }
}

fn index_attr(index: &mut HtmlIndex, node_id: NodeId, attr: &Attribute) {
    if attr.name.ns == ns!() && attr.name.local == local_name!("id") {
        let value = attr.value.as_ref();
        if value.is_empty() {
            return;
        }
        index
            .by_id
            .entry(value.to_string().into_boxed_str())
            .or_insert(node_id);
        return;
    }

    if attr.name.ns == ns!() && attr.name.local == local_name!("class") {
        for class in attr.value.split_ascii_whitespace() {
            if class.is_empty() {
                continue;
            }
            index
                .by_class
                .entry(class.to_string().into_boxed_str())
                .or_default()
                .push(node_id);
        }
    }
}

fn normalize_tag_name(name: &str) -> Cow<'_, str> {
    if name.bytes().any(|byte| byte.is_ascii_uppercase()) {
        Cow::Owned(name.to_ascii_lowercase())
    } else {
        Cow::Borrowed(name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_builds_fast_indexes() {
        let html = r#"<html><body><div id="hero" class="a b"></div></body></html>"#;
        let tree = HtmlTree::parse(html);
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
        let tree = HtmlTree::parse(html);
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
        let tree = HtmlTree::parse(html);
        let index = tree.index();
        assert!(index.by_class("a").is_some());
        assert!(index.by_class("b").is_some());
    }
}
