use html5ever::{LocalName, local_name, ns};
use rustc_hash::FxHashMap;

use crate::node::{ElementAttr, NodeId, normalize_tag_name};

/// Fast tag/id/class index for a parsed HTML tree.
#[derive(Debug, Default)]
pub struct HtmlIndex {
    by_id: FxHashMap<Box<str>, NodeId>,
    by_tag: FxHashMap<LocalName, Vec<NodeId>>,
    by_class: FxHashMap<Box<str>, Vec<NodeId>>,
}

impl HtmlIndex {
    pub(crate) fn with_capacity(estimated_nodes: usize) -> Self {
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

    pub(crate) fn insert_tag(&mut self, name: LocalName, node_id: NodeId) {
        self.by_tag.entry(name).or_default().push(node_id);
    }
}

pub(crate) fn index_attr(index: &mut HtmlIndex, node_id: NodeId, attr: &ElementAttr) {
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
