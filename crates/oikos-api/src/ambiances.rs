//! Ambiance wire types: one-tap house states made of actions. Shape validation lives here
//! (no house needed); checks against the house belong to the hub.

use oikos_core::model::Command;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// The icon names an ambiance may carry; the UI draws one glyph per name.
pub const AMBIANCE_ICONS: [&str; 12] = [
    "sofa", "moon", "door_out", "book", "sun", "sparkle", "film", "plate", "bath", "leaf", "flame",
    "house",
];

const MAX_ID: usize = 48;
const MAX_NAME: usize = 60;
const MAX_SUBTITLE: usize = 80;
const MAX_HOURS: u32 = 720;

/// A named set of actions the house applies in one tap. Unknown top-level fields are kept on
/// a round trip (`extra`) so a client can add display fields.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Ambiance {
    /// Taken from the topic or the request path when a payload omits it.
    #[serde(default)]
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subtitle: Option<String>,
    /// One of `AMBIANCE_ICONS`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(default)]
    pub pinned: bool,
    #[serde(default)]
    pub order: u32,
    #[serde(default)]
    pub actions: Vec<AmbianceAction>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// One step of an ambiance.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AmbianceAction {
    Category(CategoryAction),
    Device(DeviceAction),
    Climate(ClimateAction),
    Flow(FlowAction),
}

/// Sets every entity of a category, optionally limited to one area.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CategoryAction {
    pub what: Category,
    pub to: Target,
    /// In percent, 1..=100; lights turned on only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub brightness: Option<u8>,
    /// An area id; `None` is the whole house.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub area: Option<String>,
    /// Entity ids left alone.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub except: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    Lights,
    Plugs,
    Covers,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Target {
    On,
    Off,
    Open,
    Close,
}

/// Sends one command to one entity (the same command `send_command` takes).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DeviceAction {
    pub entity: String,
    pub command: Command,
}

/// Overrides a climate zone for a while, or clears the override.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ClimateAction {
    /// A zone id; `None` is every zone.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub zone: Option<String>,
    /// A number or the string `"off"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub setpoint: Option<Value>,
    /// `heat` or `cool`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,
    /// How long the override lasts, 1..=720.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hours: Option<u32>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub clear: bool,
}

fn is_false(b: &bool) -> bool {
    !*b
}

/// Enables, disables or runs a flow.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct FlowAction {
    pub flow: String,
    #[serde(rename = "do")]
    pub action: FlowDo,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum FlowDo {
    Enable,
    Disable,
    Run,
}

/// An ambiance as listed: valid, or kept visible with the reason it cannot be read.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct AmbianceView {
    pub id: String,
    pub ambiance: Option<Ambiance>,
    pub invalid: Option<String>,
}

/// What applying an ambiance did.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ApplyReport {
    pub ambiance: String,
    /// Unix seconds.
    pub started_at: u64,
    pub actions: Vec<ActionReport>,
}

/// One line of an apply report, for the action at `index`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ActionReport {
    pub index: usize,
    pub outcome: Outcome,
    /// Entities or zones targeted.
    pub total: usize,
    pub failed: Vec<Problem>,
    pub skipped: Vec<Problem>,
    pub reason: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Ok,
    Failed,
    Skipped,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Problem {
    pub id: String,
    pub reason: String,
}

/// An id is 1 to 48 characters from `[a-z0-9_]`.
pub fn is_valid_ambiance_id(id: &str) -> bool {
    (1..=MAX_ID).contains(&id.len())
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
}

/// Checks the shape of an ambiance, without a house: ids, lengths, icon and each action's
/// own rules. An action's error names its index.
pub fn validate_ambiance(a: &Ambiance) -> Result<(), String> {
    if !is_valid_ambiance_id(&a.id) {
        return Err(format!(
            "id must be 1 to {MAX_ID} characters from a-z, 0-9 and _"
        ));
    }
    if a.name.trim().is_empty() {
        return Err("name must not be empty".into());
    }
    if a.name.chars().count() > MAX_NAME {
        return Err(format!("name is longer than {MAX_NAME} characters"));
    }
    if a.subtitle
        .as_ref()
        .is_some_and(|s| s.chars().count() > MAX_SUBTITLE)
    {
        return Err(format!("subtitle is longer than {MAX_SUBTITLE} characters"));
    }
    if let Some(icon) = &a.icon
        && !AMBIANCE_ICONS.contains(&icon.as_str())
    {
        return Err(format!("unknown icon {icon:?}"));
    }
    for (i, action) in a.actions.iter().enumerate() {
        validate_action(action).map_err(|e| format!("action {i}: {e}"))?;
    }
    Ok(())
}

fn validate_action(action: &AmbianceAction) -> Result<(), String> {
    match action {
        AmbianceAction::Category(c) => validate_category(c),
        AmbianceAction::Climate(c) => validate_climate(c),
        AmbianceAction::Device(_) | AmbianceAction::Flow(_) => Ok(()),
    }
}

fn validate_category(c: &CategoryAction) -> Result<(), String> {
    match (c.what, c.to) {
        (Category::Lights | Category::Plugs, Target::On | Target::Off) => {}
        (Category::Lights | Category::Plugs, _) => {
            return Err("lights and plugs take on or off".into());
        }
        (Category::Covers, Target::Open | Target::Close) => {}
        (Category::Covers, _) => return Err("covers take open or close".into()),
    }
    if let Some(b) = c.brightness {
        if c.what != Category::Lights || c.to != Target::On {
            return Err("brightness only applies to lights turned on".into());
        }
        if !(1..=100).contains(&b) {
            return Err("brightness must be between 1 and 100".into());
        }
    }
    Ok(())
}

fn validate_climate(c: &ClimateAction) -> Result<(), String> {
    if let Some(mode) = &c.mode
        && mode != "heat"
        && mode != "cool"
    {
        return Err("mode must be heat or cool".into());
    }
    if let Some(setpoint) = &c.setpoint
        && !(setpoint.is_number() || setpoint.as_str() == Some("off"))
    {
        return Err("setpoint must be a number or \"off\"".into());
    }
    if c.clear {
        if c.setpoint.is_some() || c.mode.is_some() {
            return Err("clear cannot come with a setpoint or a mode".into());
        }
        return Ok(());
    }
    if c.setpoint.is_none() && c.mode.is_none() {
        return Err("climate needs clear, a setpoint or a mode".into());
    }
    match c.hours {
        Some(h) if (1..=MAX_HOURS).contains(&h) => Ok(()),
        _ => Err(format!("hours must be between 1 and {MAX_HOURS}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Spec Appendix A.2, without the comment. The command is the real `select_option`
    /// shape (the spec's `select` was a placeholder).
    fn spec_example() -> Value {
        json!({
          "id": "bonne_nuit",
          "name": "Bonne nuit",
          "subtitle": "La maison s'endort",
          "icon": "moon",
          "pinned": true,
          "order": 1,
          "actions": [
            { "type": "category", "what": "lights", "to": "off", "area": null, "except": [] },
            { "type": "category", "what": "lights", "to": "on", "brightness": 30, "area": "living_room" },
            { "type": "category", "what": "plugs", "to": "off" },
            { "type": "category", "what": "covers", "to": "close" },
            { "type": "device", "entity": "select.house_mode", "command": { "type": "select_option", "option": "night" } },
            { "type": "climate", "zone": null, "setpoint": 16.0, "mode": null, "hours": 12 },
            { "type": "climate", "zone": null, "clear": true },
            { "type": "flow", "flow": "away_lights", "do": "enable" }
          ]
        })
    }

    /// Null and absent are the same on the wire: optional fields are not serialized when
    /// empty, so the expected value drops its nulls and empty `except` lists.
    fn without_empty(v: &Value) -> Value {
        match v {
            Value::Object(m) => Value::Object(
                m.iter()
                    .filter(|(k, v)| !v.is_null() && !(k.as_str() == "except" && *v == &json!([])))
                    .map(|(k, v)| (k.clone(), without_empty(v)))
                    .collect(),
            ),
            Value::Array(a) => Value::Array(a.iter().map(without_empty).collect()),
            other => other.clone(),
        }
    }

    fn ambiance(actions: Value) -> Ambiance {
        serde_json::from_value(json!({"id": "a", "name": "A", "actions": actions})).unwrap()
    }

    #[test]
    fn spec_example_round_trips() {
        let value = spec_example();
        let a: Ambiance = serde_json::from_value(value.clone()).unwrap();
        assert_eq!(a.actions.len(), 8);
        assert_eq!(serde_json::to_value(&a).unwrap(), without_empty(&value));
    }

    #[test]
    fn unknown_top_level_fields_survive() {
        let value = json!({"id": "a", "name": "A", "pinned": false, "order": 0, "actions": [], "color": "#fff"});
        let a: Ambiance = serde_json::from_value(value.clone()).unwrap();
        assert_eq!(a.extra["color"], "#fff");
        assert_eq!(serde_json::to_value(&a).unwrap(), value);
    }

    #[test]
    fn minimal_ambiance_defaults() {
        let a: Ambiance = serde_json::from_value(json!({"id": "a", "name": "A"})).unwrap();
        assert!(!a.pinned);
        assert_eq!(a.order, 0);
        assert!(a.actions.is_empty());
        let no_id: Ambiance = serde_json::from_value(json!({"name": "A"})).unwrap();
        assert_eq!(no_id.id, "");
    }

    #[test]
    fn validate_accepts_spec_example() {
        let a: Ambiance = serde_json::from_value(spec_example()).unwrap();
        assert_eq!(validate_ambiance(&a), Ok(()));
    }

    #[test]
    fn validate_rejects() {
        let cat = |v: Value| ambiance(json!([v]));
        let climate = |v: Value| {
            let mut m = json!({"type": "climate"});
            m.as_object_mut()
                .unwrap()
                .extend(v.as_object().unwrap().clone());
            ambiance(json!([m]))
        };
        let base = || ambiance(json!([]));
        let mut cases: Vec<(&str, Ambiance)> = Vec::new();
        let mut bad = base();
        bad.id = "Bonne Nuit".into();
        cases.push(("id with caps and space", bad));
        let mut bad = base();
        bad.id = "a".repeat(49);
        cases.push(("id too long", bad));
        let mut bad = base();
        bad.name = "  ".into();
        cases.push(("blank name", bad));
        let mut bad = base();
        bad.name = "n".repeat(61);
        cases.push(("name too long", bad));
        let mut bad = base();
        bad.subtitle = Some("s".repeat(81));
        cases.push(("subtitle too long", bad));
        let mut bad = base();
        bad.icon = Some("rocket".into());
        cases.push(("unknown icon", bad));
        cases.push((
            "lights open",
            cat(json!({"type": "category", "what": "lights", "to": "open"})),
        ));
        cases.push((
            "covers off",
            cat(json!({"type": "category", "what": "covers", "to": "off"})),
        ));
        cases.push((
            "plugs brightness",
            cat(json!({"type": "category", "what": "plugs", "to": "on", "brightness": 50})),
        ));
        cases.push((
            "lights off brightness",
            cat(json!({"type": "category", "what": "lights", "to": "off", "brightness": 50})),
        ));
        for b in [0, 101] {
            cases.push((
                "brightness out of range",
                cat(json!({"type": "category", "what": "lights", "to": "on", "brightness": b})),
            ));
        }
        cases.push((
            "clear with setpoint",
            climate(json!({"clear": true, "setpoint": 16})),
        ));
        cases.push(("empty climate", climate(json!({"zone": "z"}))));
        cases.push(("setpoint without hours", climate(json!({"setpoint": 16}))));
        for h in [0, 721] {
            cases.push((
                "hours out of range",
                climate(json!({"setpoint": 16, "hours": h})),
            ));
        }
        cases.push((
            "bad mode",
            climate(json!({"mode": "dry", "setpoint": 16, "hours": 2})),
        ));
        cases.push((
            "bad setpoint",
            climate(json!({"setpoint": "warm", "hours": 2})),
        ));
        for (label, a) in &cases {
            assert!(validate_ambiance(a).is_err(), "{label} should be refused");
        }
    }

    #[test]
    fn action_errors_name_the_index() {
        let a = ambiance(json!([
            {"type": "category", "what": "plugs", "to": "off"},
            {"type": "category", "what": "covers", "to": "off"},
        ]));
        assert_eq!(
            validate_ambiance(&a),
            Err("action 1: covers take open or close".to_string())
        );
    }
}
