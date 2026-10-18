//! Areas (rooms) retained on `oikos/ui/_areas`, as the app's `Area` and `AreasDocument`.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::ui_override::strip_nulls;

/// The area of every entity that has no override, suggested area or path area.
pub const UNASSIGNED_AREA: &str = "Unassigned";

/// One room.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Area {
    pub id: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub order: Option<i32>,
    /// Custom group card names by kind key (`cover`, `light`); missing = the app's default.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub group_names: BTreeMap<String, String>,
    /// Kind keys whose room group shows in the Favorites chip on Home.
    #[serde(skip_serializing_if = "BTreeSet::is_empty")]
    pub group_favorites: BTreeSet<String>,
    /// Fields written by a newer app, preserved verbatim.
    #[serde(flatten, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: BTreeMap<String, Value>,
}

/// Retained payload of `oikos/ui/_areas`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AreasDocument {
    pub areas: Vec<Area>,
}

impl AreasDocument {
    /// Decodes the document leniently; `None` when it is not a JSON object or an area is malformed.
    pub fn decode(payload: &[u8]) -> Option<AreasDocument> {
        let mut value: Value = serde_json::from_slice(payload).ok()?;
        let document = value.as_object_mut()?;
        strip_nulls(document);
        if let Some(areas) = document.get_mut("areas").and_then(Value::as_array_mut) {
            for area in areas.iter_mut().filter_map(Value::as_object_mut) {
                strip_nulls(area);
            }
        }
        serde_json::from_value(value).ok()
    }

    /// Encodes with defaults omitted.
    pub fn encode(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| r#"{"areas":[]}"#.to_owned())
    }
}
