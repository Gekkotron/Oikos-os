use serde::{Deserialize, Serialize};

pub const HOUSE_ID: &str = "house";
pub const HOUSE_NAME: &str = "House";

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct Dashboard {
    pub id: String,
    pub name: String,
    pub order: u32,
    pub widgets: Vec<Widget>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct Widget {
    pub id: String,
    pub kind: WidgetKind,
    pub span: Span,
    pub config: WidgetConfig,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum WidgetKind {
    Light,
    Cover,
    Climate,
    Lock,
    Sensor,
    RoomGroup,
    FlowButton,
    EnergyStat,
    HistoryChart,
    Camera,
    WaterHeater,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum Span {
    #[serde(rename = "1x1")]
    X1x1,
    #[serde(rename = "2x1")]
    X2x1,
    #[serde(rename = "2x2")]
    X2x2,
    #[serde(rename = "1x2")]
    X1x2,
}

impl Span {
    pub fn as_str(&self) -> &'static str {
        match self {
            Span::X1x1 => "1x1",
            Span::X2x1 => "2x1",
            Span::X2x2 => "2x2",
            Span::X1x2 => "1x2",
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum DashboardHistoryRange {
    #[serde(rename = "1h")]
    H1,
    #[serde(rename = "6h")]
    H6,
    #[serde(rename = "24h")]
    H24,
    #[serde(rename = "7d")]
    D7,
    #[serde(rename = "30d")]
    D30,
}

/// Untagged: `History` is listed before `Entity` because both carry
/// `entity_id`, and `History` is the one that requires `range`.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(untagged)]
pub enum WidgetConfig {
    History {
        entity_id: String,
        range: DashboardHistoryRange,
    },
    Entity {
        entity_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        title: Option<String>,
    },
    Area {
        area_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        title: Option<String>,
    },
    Flow {
        flow_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        title: Option<String>,
    },
    Energy {
        metric: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        title: Option<String>,
    },
}

pub fn allowed_spans(kind: WidgetKind) -> &'static [Span] {
    use Span::*;
    match kind {
        WidgetKind::Light
        | WidgetKind::Cover
        | WidgetKind::Climate
        | WidgetKind::Lock
        | WidgetKind::Sensor
        | WidgetKind::FlowButton
        | WidgetKind::WaterHeater
        | WidgetKind::EnergyStat => &[X1x1, X2x1],
        WidgetKind::RoomGroup => &[X1x1, X2x1, X2x2],
        WidgetKind::HistoryChart | WidgetKind::Camera => &[X2x1, X2x2, X1x2],
    }
}

pub fn is_valid_dashboard_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id != HOUSE_ID
        && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn span_round_trips_as_str() {
        for (s, w) in [
            (Span::X1x1, "1x1"),
            (Span::X2x1, "2x1"),
            (Span::X2x2, "2x2"),
            (Span::X1x2, "1x2"),
        ] {
            assert_eq!(s.as_str(), w);
            assert_eq!(serde_json::to_string(&s).unwrap(), format!("\"{w}\""));
            assert_eq!(serde_json::from_str::<Span>(&format!("\"{w}\"")).unwrap(), s);
        }
    }

    #[test]
    fn allowed_spans_match_spec() {
        assert_eq!(allowed_spans(WidgetKind::Light), &[Span::X1x1, Span::X2x1]);
        assert_eq!(
            allowed_spans(WidgetKind::RoomGroup),
            &[Span::X1x1, Span::X2x1, Span::X2x2]
        );
        assert_eq!(
            allowed_spans(WidgetKind::HistoryChart),
            &[Span::X2x1, Span::X2x2, Span::X1x2]
        );
        assert_eq!(
            allowed_spans(WidgetKind::Camera),
            &[Span::X2x1, Span::X2x2, Span::X1x2]
        );
    }

    #[test]
    fn house_id_reserved() {
        assert!(!is_valid_dashboard_id("house"));
        assert!(is_valid_dashboard_id("morning_glance"));
        assert!(!is_valid_dashboard_id(""));
        assert!(!is_valid_dashboard_id("bad id"));
        assert!(is_valid_dashboard_id(&"a".repeat(64)));
        assert!(!is_valid_dashboard_id(&"a".repeat(65)));
    }

    #[test]
    fn dashboard_serde_round_trip() {
        let d = Dashboard {
            id: "d_1".into(),
            name: "Morning".into(),
            order: 1,
            widgets: vec![
                Widget {
                    id: "w1".into(),
                    kind: WidgetKind::Light,
                    span: Span::X1x1,
                    config: WidgetConfig::Entity {
                        entity_id: "lamp".into(),
                        title: Some("Lamp".into()),
                    },
                },
                Widget {
                    id: "w2".into(),
                    kind: WidgetKind::HistoryChart,
                    span: Span::X2x2,
                    config: WidgetConfig::History {
                        entity_id: "temp".into(),
                        range: DashboardHistoryRange::D7,
                    },
                },
            ],
        };
        let json = serde_json::to_string(&d).unwrap();
        assert_eq!(serde_json::from_str::<Dashboard>(&json).unwrap(), d);
    }

    #[test]
    fn widget_config_tagged_by_kind_shape() {
        let w: Widget = serde_json::from_str(
            r#"{ "id": "w", "kind": "light", "span": "1x1", "config": { "entity_id": "lamp" } }"#,
        )
        .unwrap();
        assert_eq!(w.kind, WidgetKind::Light);
        assert_eq!(w.span, Span::X1x1);
        assert_eq!(
            w.config,
            WidgetConfig::Entity { entity_id: "lamp".into(), title: None }
        );
    }

    #[test]
    fn history_range_round_trips_wire_strings() {
        for (r, w) in [
            (DashboardHistoryRange::H1, "1h"),
            (DashboardHistoryRange::H6, "6h"),
            (DashboardHistoryRange::H24, "24h"),
            (DashboardHistoryRange::D7, "7d"),
            (DashboardHistoryRange::D30, "30d"),
        ] {
            assert_eq!(serde_json::to_string(&r).unwrap(), format!("\"{w}\""));
            assert_eq!(
                serde_json::from_str::<DashboardHistoryRange>(&format!("\"{w}\"")).unwrap(),
                r
            );
        }
    }
}
