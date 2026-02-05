use serde::{Deserialize, Serialize};

/// NFO movie root element.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename = "movie")]
pub struct Movie {
    pub title: Option<String>,
    pub originaltitle: Option<String>,
    pub sorttitle: Option<String>,
    pub year: Option<u16>,
    pub premiered: Option<String>,
    pub runtime: Option<u16>,
    pub director: Option<String>,
    #[serde(default)]
    pub credits: Vec<String>,
    #[serde(default)]
    pub genre: Vec<String>,
    #[serde(default)]
    pub tag: Vec<String>,
    pub plot: Option<String>,
    pub outline: Option<String>,
    pub tagline: Option<String>,
    pub ratings: Option<Ratings>,
    pub userrating: Option<f32>,
    #[serde(default)]
    pub uniqueid: Vec<UniqueId>,
    pub thumb: Option<Thumb>,
    pub fanart: Option<Fanart>,
    pub studio: Option<String>,
    #[serde(default)]
    pub country: Vec<String>,
    #[serde(rename = "set")]
    pub set_info: Option<SetInfo>,
    #[serde(default)]
    pub actor: Vec<Actor>,
    pub trailer: Option<String>,
    pub fileinfo: Option<String>,
    pub dateadded: Option<String>,
}

/// Ratings container.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Ratings {
    #[serde(default, rename = "rating")]
    pub rating: Vec<Rating>,
}

/// Single rating with optional attributes.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Rating {
    #[serde(rename = "@name")]
    pub name: Option<String>,
    #[serde(rename = "@max")]
    pub max: Option<f32>,
    #[serde(rename = "@default")]
    pub is_default: Option<bool>,
    pub value: Option<f32>,
    pub votes: Option<u32>,
}

/// Unique ID entry such as IMDb or TMDb.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UniqueId {
    #[serde(rename = "@type")]
    pub id_type: Option<String>,
    #[serde(rename = "@default")]
    pub is_default: Option<bool>,
    #[serde(rename = "$text")]
    pub value: Option<String>,
}

/// Poster or fanart thumbnail.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Thumb {
    #[serde(rename = "@aspect")]
    pub aspect: Option<String>,
    #[serde(rename = "@preview")]
    pub preview: Option<String>,
    #[serde(rename = "$text")]
    pub value: Option<String>,
}

/// Fanart collection.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Fanart {
    #[serde(default, rename = "thumb")]
    pub thumb: Vec<Thumb>,
}

/// Movie collection/set info.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SetInfo {
    pub name: Option<String>,
    pub overview: Option<String>,
}

/// Actor entry.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Actor {
    pub name: Option<String>,
    pub role: Option<String>,
    pub gender: Option<String>,
    pub order: Option<u32>,
}
