//! The body of a hand-made group: one entity that fans commands out to its members.

use oikos_core::model::Kind;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// A group's definition.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GroupSpec {
    /// The kind of entity the group presents itself as.
    pub kind: Kind,
    /// The display name.
    pub name: String,
    /// The entity ids the group controls.
    pub members: Vec<String>,
    /// Whether the members disappear from the UI lists, true by default.
    #[serde(rename = "hideMembers", default = "default_hide")]
    pub hide_members: bool,
}

/// The default of `hideMembers`.
pub fn default_hide() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn group_spec_round_trip_matches_appendix_a() {
        let json = r#"{"name":"Living lights","kind":"light","members":["a","b"],"hideMembers":true}"#;
        let spec: GroupSpec = serde_json::from_str(json).unwrap();
        assert_eq!(spec.name, "Living lights");
        assert_eq!(spec.kind, oikos_core::model::Kind::Light);
        assert_eq!(spec.members, vec!["a", "b"]);
        assert!(spec.hide_members);
        assert_eq!(
            serde_json::to_value(&spec).unwrap(),
            serde_json::from_str::<serde_json::Value>(json).unwrap()
        );
    }

    #[test]
    fn hide_members_defaults_to_true() {
        let json = r#"{"name":"X","kind":"switch","members":[]}"#;
        assert!(serde_json::from_str::<GroupSpec>(json).unwrap().hide_members);
    }

    #[test]
    fn unknown_fields_are_refused() {
        let json = r#"{"name":"X","kind":"switch","members":[],"color":"red"}"#;
        assert!(serde_json::from_str::<GroupSpec>(json).is_err());
    }
}
