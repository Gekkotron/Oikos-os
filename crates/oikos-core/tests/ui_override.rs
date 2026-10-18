//! `UiOverride` round-trips the app's retained `oikos/ui/<unique_id>` JSON.

mod common;

use oikos_core::model::{Command, PatchError, UiOverride};
use serde_json::{Map, Value, json};

fn patch(pairs: Value) -> Map<String, Value> {
    match pairs {
        Value::Object(map) => map,
        other => panic!("patch must be an object, got {other}"),
    }
}

#[test]
fn decodes_every_recorded_override() {
    let dir = common::fixture_dir().join("ui");
    let mut seen = 0;
    for entry in std::fs::read_dir(&dir).expect("ui fixture dir") {
        let path = entry.expect("dir entry").path();
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        if !name.ends_with(".json") || name == "_areas.json" {
            continue;
        }
        let bytes = std::fs::read(&path).expect("read fixture");
        assert!(
            UiOverride::decode(&bytes).is_some(),
            "{name} must decode to Some"
        );
        seen += 1;
    }
    assert_eq!(seen, 132, "expected every recorded override to be visited");
}

#[test]
fn encode_omits_defaults() {
    assert_eq!(UiOverride::default().encode(), "{}");
    let o = UiOverride {
        security_ask: false,
        ..Default::default()
    };
    assert_eq!(o.encode(), r#"{"securityAsk":false}"#);
}

#[test]
fn unknown_field_round_trips() {
    let o = UiOverride::decode(br#"{"area":"living","futureField":{"a":1}}"#).unwrap();
    assert_eq!(o.area.as_deref(), Some("living"));
    assert_eq!(o.encode(), r#"{"area":"living","futureField":{"a":1}}"#);
}

#[test]
fn unknown_energy_role_is_kept() {
    let o = UiOverride::decode(br#"{"energyRole":"fusion"}"#).unwrap();
    assert_eq!(o.energy_role.as_deref(), Some("fusion"));
    assert_eq!(o.encode(), r#"{"energyRole":"fusion"}"#);
}

#[test]
fn decode_is_lenient() {
    assert!(UiOverride::decode(b"not json").is_none());
    assert!(UiOverride::decode(b"[1,2]").is_none());
    // Nulls read as "absent", as the app's `explicitNulls = false` does.
    let o = UiOverride::decode(br#"{"hidden":null,"name":null,"favorite":true}"#).unwrap();
    assert!(!o.hidden);
    assert_eq!(o.name, None);
    assert!(o.favorite);
    assert!(o.security_ask, "securityAsk defaults to true");
}

#[test]
fn patch_sets_clears_and_rejects() {
    let base = UiOverride {
        area: Some("living".into()),
        ..Default::default()
    };

    let set = base.apply_patch(&patch(json!({"asLight": true}))).unwrap();
    assert!(set.as_light);
    assert_eq!(set.area.as_deref(), Some("living"), "untouched fields stay");

    let cleared = base.apply_patch(&patch(json!({"area": null}))).unwrap();
    assert_eq!(cleared.area, None);

    assert_eq!(
        base.apply_patch(&patch(json!({"nope": 1}))).unwrap_err(),
        PatchError::UnknownField("nope".into())
    );
    match base
        .apply_patch(&patch(json!({"lightOnBrightness": 300})))
        .unwrap_err()
    {
        PatchError::InvalidValue { field, .. } => assert_eq!(field, "lightOnBrightness"),
        other => panic!("expected InvalidValue, got {other:?}"),
    }
    match base
        .apply_patch(&patch(json!({"energyRole": "fusion"})))
        .unwrap_err()
    {
        PatchError::InvalidValue { field, expected } => {
            assert_eq!(field, "energyRole");
            assert!(
                expected.contains("grid_export"),
                "names the options: {expected}"
            );
        }
        other => panic!("expected InvalidValue, got {other:?}"),
    }
    match base
        .apply_patch(&patch(json!({"hidden": "yes"})))
        .unwrap_err()
    {
        PatchError::InvalidValue { field, .. } => assert_eq!(field, "hidden"),
        other => panic!("expected InvalidValue, got {other:?}"),
    }

    let valid = base
        .apply_patch(&patch(json!({
            "energyRole": "grid_export",
            "lightOnPreset": "warm",
            "lightOnBrightness": 255,
            "securityAsk": false
        })))
        .unwrap();
    assert_eq!(valid.energy_role.as_deref(), Some("grid_export"));
    assert_eq!(valid.light_on_preset.as_deref(), Some("warm"));
    assert_eq!(valid.light_on_brightness, Some(255));
    assert!(!valid.security_ask);
}

#[test]
fn is_default_after_clearing_everything() {
    let o = UiOverride::decode(
        br#"{"name":"Lamp","area":"living","hidden":true,"securityAsk":false,"lightOnBrightness":42}"#,
    )
    .unwrap();
    assert!(!o.is_default());
    let cleared = o
        .apply_patch(&patch(json!({
            "name": null, "area": null, "hidden": null, "securityAsk": null, "lightOnBrightness": null
        })))
        .unwrap();
    assert!(cleared.is_default());
    assert_eq!(cleared.encode(), "{}");
    assert!(UiOverride::default().is_default());
}

#[test]
fn fields_lists_the_nineteen_app_fields() {
    assert_eq!(UiOverride::FIELDS.len(), 19);
    assert_eq!(UiOverride::FIELDS[0], "name");
    assert!(UiOverride::FIELDS.contains(&"climatePresets"));
}

#[test]
fn command_json_shape() {
    assert_eq!(
        serde_json::to_string(&Command::SetPosition { position: 40 }).unwrap(),
        r#"{"type":"set_position","position":40}"#
    );
}
