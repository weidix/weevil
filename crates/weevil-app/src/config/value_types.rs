use std::path::PathBuf;

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub(super) enum StringList {
    One(String),
    Many(Vec<String>),
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub(super) enum StringPathList {
    One(PathBuf),
    Many(Vec<PathBuf>),
}

impl StringPathList {
    pub(super) fn to_vec(&self) -> Vec<PathBuf> {
        match self {
            StringPathList::One(value) => vec![value.clone()],
            StringPathList::Many(values) => values.clone(),
        }
    }
}

impl StringList {
    pub(super) fn to_vec(&self) -> Vec<String> {
        match self {
            StringList::One(value) => vec![value.clone()],
            StringList::Many(values) => values.clone(),
        }
    }
}
