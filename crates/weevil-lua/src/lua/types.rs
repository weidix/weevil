use std::sync::Arc;

use mlua::{AnyUserData, FromLua, Lua, UserData, UserDataMethods, Value};
use weevil_core::{ElementData, HtmlIndex, HtmlTree, NodeId, NodeKind, Selector, XPath};

#[derive(Clone)]
pub struct LuaHtmlTree(pub(crate) Arc<HtmlTree>);

#[derive(Clone, Copy, Debug)]
pub struct LuaNodeId(pub(crate) NodeId);

#[derive(Clone)]
pub struct LuaSelector(pub(crate) Selector);

#[derive(Clone)]
pub struct LuaXPath(pub(crate) XPath);

impl LuaHtmlTree {
    pub fn new(tree: HtmlTree) -> Self {
        Self(Arc::new(tree))
    }

    pub fn tree(&self) -> &HtmlTree {
        &self.0
    }
}

impl LuaNodeId {
    pub fn new(id: NodeId) -> Self {
        Self(id)
    }

    pub fn id(self) -> NodeId {
        self.0
    }
}

impl FromLua for LuaNodeId {
    fn from_lua(value: Value, lua: &Lua) -> mlua::Result<Self> {
        let userdata = AnyUserData::from_lua(value, lua)?;
        let id = userdata.borrow::<LuaNodeId>()?;
        Ok(*id)
    }
}

impl FromLua for LuaHtmlTree {
    fn from_lua(value: Value, lua: &Lua) -> mlua::Result<Self> {
        let userdata = AnyUserData::from_lua(value, lua)?;
        let tree = userdata.borrow::<LuaHtmlTree>()?;
        Ok(tree.clone())
    }
}

impl UserData for LuaNodeId {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("__tostring", |_, this, ()| Ok(format!("{:?}", this.0)));
    }
}

impl UserData for LuaHtmlTree {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("document", |_, this, ()| {
            Ok(LuaNodeId::new(this.0.document()))
        });
        methods.add_method("root_element", |_, this, ()| {
            Ok(this.0.root_element().map(LuaNodeId::new))
        });
        methods.add_method("quirks_mode", |_, this, ()| {
            Ok(format!("{:?}", this.0.quirks_mode()))
        });
        methods.add_method("attr", |_, this, (node, name): (LuaNodeId, String)| {
            Ok(this.0.attr(node.id(), &name).map(str::to_string))
        });
        methods.add_method("text_content", |_, this, node: LuaNodeId| {
            Ok(this.0.text_content(node.id()))
        });
        methods.add_method("text", |_, this, node: LuaNodeId| {
            Ok(this.0.text(node.id()))
        });
        methods.add_method("html", |_, this, node: LuaNodeId| {
            Ok(this.0.html(node.id()))
        });
        methods.add_method("inner_html", |_, this, node: LuaNodeId| {
            Ok(this.0.inner_html(node.id()))
        });
        methods.add_method("outer_html", |_, this, node: LuaNodeId| {
            Ok(this.0.outer_html(node.id()))
        });
        methods.add_method("children", |_, this, node: LuaNodeId| {
            Ok(this
                .0
                .children(node.id())
                .map(LuaNodeId::new)
                .collect::<Vec<_>>())
        });
        methods.add_method("descendants", |_, this, node: LuaNodeId| {
            Ok(this
                .0
                .descendants(node.id())
                .map(LuaNodeId::new)
                .collect::<Vec<_>>())
        });
        methods.add_method("subtree", |_, this, node: LuaNodeId| {
            Ok(this
                .0
                .subtree(node.id())
                .map(LuaNodeId::new)
                .collect::<Vec<_>>())
        });
        methods.add_method("parent", |_, this, node: LuaNodeId| {
            Ok(this.0.node(node.id()).parent().map(LuaNodeId::new))
        });
        methods.add_method("first_child", |_, this, node: LuaNodeId| {
            Ok(this.0.node(node.id()).first_child().map(LuaNodeId::new))
        });
        methods.add_method("last_child", |_, this, node: LuaNodeId| {
            Ok(this.0.node(node.id()).last_child().map(LuaNodeId::new))
        });
        methods.add_method("prev_sibling", |_, this, node: LuaNodeId| {
            Ok(this.0.node(node.id()).prev_sibling().map(LuaNodeId::new))
        });
        methods.add_method("next_sibling", |_, this, node: LuaNodeId| {
            Ok(this.0.node(node.id()).next_sibling().map(LuaNodeId::new))
        });
        methods.add_method("node_kind", |_, this, node: LuaNodeId| {
            let kind = match this.0.node(node.id()).kind() {
                NodeKind::Document => "document",
                NodeKind::Doctype { .. } => "doctype",
                NodeKind::Text(_) => "text",
                NodeKind::Comment(_) => "comment",
                NodeKind::Element(_) => "element",
                NodeKind::ProcessingInstruction { .. } => "processing-instruction",
            };
            Ok(kind.to_string())
        });
        methods.add_method("node_text", |_, this, node: LuaNodeId| {
            let text = match this.0.node(node.id()).kind() {
                NodeKind::Text(contents) => Some(contents.as_ref().to_string()),
                _ => None,
            };
            Ok(text)
        });
        methods.add_method("comment_text", |_, this, node: LuaNodeId| {
            let text = match this.0.node(node.id()).kind() {
                NodeKind::Comment(contents) => Some(contents.as_ref().to_string()),
                _ => None,
            };
            Ok(text)
        });
        methods.add_method("doctype", |lua, this, node: LuaNodeId| {
            let NodeKind::Doctype {
                name,
                public_id,
                system_id,
            } = this.0.node(node.id()).kind()
            else {
                return Ok(None);
            };
            let table = lua.create_table()?;
            table.set("name", name.as_ref())?;
            table.set("public_id", public_id.as_ref())?;
            table.set("system_id", system_id.as_ref())?;
            Ok(Some(table))
        });
        methods.add_method("processing_instruction", |lua, this, node: LuaNodeId| {
            let NodeKind::ProcessingInstruction { target, data } = this.0.node(node.id()).kind()
            else {
                return Ok(None);
            };
            let table = lua.create_table()?;
            table.set("target", target.as_ref())?;
            table.set("data", data.as_ref())?;
            Ok(Some(table))
        });
        methods.add_method("tag_name", |_, this, node: LuaNodeId| {
            Ok(this
                .0
                .element(node.id())
                .map(|element| element.tag_name().to_string()))
        });
        methods.add_method("namespace", |_, this, node: LuaNodeId| {
            Ok(this
                .0
                .element(node.id())
                .map(|element| element.namespace().to_string()))
        });
        methods.add_method("is_tag", |_, this, (node, name): (LuaNodeId, String)| {
            let result = this
                .0
                .element(node.id())
                .map(|element| element.is_tag(&name))
                .unwrap_or(false);
            Ok(result)
        });
        methods.add_method("attr_names", |_, this, node: LuaNodeId| {
            Ok(element_attr_names(this.0.element(node.id())))
        });
        methods.add_method("attrs", |lua, this, node: LuaNodeId| {
            let Some(element) = this.0.element(node.id()) else {
                return Ok(None);
            };
            let table = lua.create_table()?;
            for (name, value) in element.attrs() {
                table.set(name, value)?;
            }
            Ok(Some(table))
        });
        methods.add_method(
            "attr_tokens",
            |_, this, (node, name): (LuaNodeId, String)| {
                let tokens = this
                    .0
                    .element(node.id())
                    .map(|element| element.attr_tokens(&name).map(str::to_string).collect())
                    .unwrap_or_else(Vec::new);
                Ok(tokens)
            },
        );
        methods.add_method("classes", |_, this, node: LuaNodeId| {
            let classes = this
                .0
                .element(node.id())
                .map(|element| element.classes().map(str::to_string).collect())
                .unwrap_or_else(Vec::new);
            Ok(classes)
        });
        methods.add_method("class_list", |_, this, node: LuaNodeId| {
            let classes = this
                .0
                .element(node.id())
                .map(|element| {
                    element
                        .class_list()
                        .into_iter()
                        .map(str::to_string)
                        .collect()
                })
                .unwrap_or_else(Vec::new);
            Ok(classes)
        });
        methods.add_method("has_class", |_, this, (node, name): (LuaNodeId, String)| {
            let result = this
                .0
                .element(node.id())
                .map(|element| element.has_class(&name))
                .unwrap_or(false);
            Ok(result)
        });
        methods.add_method("has_attr", |_, this, (node, name): (LuaNodeId, String)| {
            let result = this
                .0
                .element(node.id())
                .map(|element| element.has_attr(&name))
                .unwrap_or(false);
            Ok(result)
        });
        methods.add_method("id", |_, this, node: LuaNodeId| {
            Ok(this
                .0
                .element(node.id())
                .and_then(ElementData::id)
                .map(str::to_string))
        });
        methods.add_method("class_value", |_, this, node: LuaNodeId| {
            Ok(this
                .0
                .element(node.id())
                .and_then(ElementData::class_value)
                .map(str::to_string))
        });
        methods.add_method("data_attr", |_, this, (node, key): (LuaNodeId, String)| {
            Ok(this
                .0
                .element(node.id())
                .and_then(|element| element.data(&key))
                .map(str::to_string))
        });
        methods.add_method("aria_attr", |_, this, (node, key): (LuaNodeId, String)| {
            Ok(this
                .0
                .element(node.id())
                .and_then(|element| element.aria(&key))
                .map(str::to_string))
        });
        methods.add_method("index_by_id", |_, this, id: String| {
            Ok(this.0.index().by_id(&id).map(LuaNodeId::new))
        });
        methods.add_method("index_by_tag", |_, this, name: String| {
            Ok(index_nodes(this.0.index(), IndexQuery::Tag(name)))
        });
        methods.add_method("index_by_class", |_, this, name: String| {
            Ok(index_nodes(this.0.index(), IndexQuery::Class(name)))
        });
    }
}

impl UserData for LuaSelector {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("find", |_, this, tree: LuaHtmlTree| {
            this.0
                .find(tree.tree())
                .map(|matches| matches.into_iter().map(LuaNodeId::new).collect::<Vec<_>>())
                .map_err(mlua::Error::external)
        });
        methods.add_method(
            "find_in",
            |_, this, (tree, node): (LuaHtmlTree, LuaNodeId)| {
                this.0
                    .find((tree.tree(), node.id()))
                    .map(|matches| matches.into_iter().map(LuaNodeId::new).collect::<Vec<_>>())
                    .map_err(mlua::Error::external)
            },
        );
        methods.add_method("find_first", |_, this, tree: LuaHtmlTree| {
            this.0
                .find_first(tree.tree())
                .map(|node| node.map(LuaNodeId::new))
                .map_err(mlua::Error::external)
        });
        methods.add_method(
            "find_first_in",
            |_, this, (tree, node): (LuaHtmlTree, LuaNodeId)| {
                this.0
                    .find_first((tree.tree(), node.id()))
                    .map(|node| node.map(LuaNodeId::new))
                    .map_err(mlua::Error::external)
            },
        );
        methods.add_method("select_one", |_, this, tree: LuaHtmlTree| {
            this.0
                .select_one(tree.tree())
                .map(|node| node.map(LuaNodeId::new))
                .map_err(mlua::Error::external)
        });
        methods.add_method("first_match", |_, this, tree: LuaHtmlTree| {
            this.0
                .first_match(tree.tree())
                .map(|node| node.map(LuaNodeId::new))
                .map_err(mlua::Error::external)
        });
    }
}

impl UserData for LuaXPath {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("find", |_, this, tree: LuaHtmlTree| {
            this.0
                .find(tree.tree())
                .map(|matches| matches.into_iter().map(LuaNodeId::new).collect::<Vec<_>>())
                .map_err(mlua::Error::external)
        });
        methods.add_method(
            "find_in",
            |_, this, (tree, node): (LuaHtmlTree, LuaNodeId)| {
                this.0
                    .find((tree.tree(), node.id()))
                    .map(|matches| matches.into_iter().map(LuaNodeId::new).collect::<Vec<_>>())
                    .map_err(mlua::Error::external)
            },
        );
        methods.add_method("find_first", |_, this, tree: LuaHtmlTree| {
            this.0
                .find_first(tree.tree())
                .map(|node| node.map(LuaNodeId::new))
                .map_err(mlua::Error::external)
        });
        methods.add_method(
            "find_first_in",
            |_, this, (tree, node): (LuaHtmlTree, LuaNodeId)| {
                this.0
                    .find_first((tree.tree(), node.id()))
                    .map(|node| node.map(LuaNodeId::new))
                    .map_err(mlua::Error::external)
            },
        );
        methods.add_method("select_one", |_, this, tree: LuaHtmlTree| {
            this.0
                .select_one(tree.tree())
                .map(|node| node.map(LuaNodeId::new))
                .map_err(mlua::Error::external)
        });
        methods.add_method("first_match", |_, this, tree: LuaHtmlTree| {
            this.0
                .first_match(tree.tree())
                .map(|node| node.map(LuaNodeId::new))
                .map_err(mlua::Error::external)
        });
    }
}

fn element_attr_names(element: Option<&ElementData>) -> Vec<String> {
    element
        .map(|element| element.attr_names().map(str::to_string).collect())
        .unwrap_or_else(Vec::new)
}

enum IndexQuery {
    Tag(String),
    Class(String),
}

fn index_nodes(index: &HtmlIndex, query: IndexQuery) -> Option<Vec<LuaNodeId>> {
    let ids = match query {
        IndexQuery::Tag(name) => index.by_tag(&name),
        IndexQuery::Class(name) => index.by_class(&name),
    }?;
    Some(ids.iter().copied().map(LuaNodeId::new).collect())
}
