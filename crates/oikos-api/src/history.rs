//! Wire types for the `/api/v1/history/*` routes and the MCP `get_history` tool.
//!
//! The shapes keep the OikosHistory names so the Android app's migration is a URL and
//! parameter change: points are packed tuples (JSON arrays on the wire), windows and times
//! are unix seconds, and absent timestamps serialise as `null`.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub use oikos_core::history::buckets::EnergyKind;

/// Named chart ranges and their window / span, in seconds.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum HistoryRange {
    #[serde(rename = "24h")]
    Day,
    #[serde(rename = "7d")]
    Week,
    #[serde(rename = "30d")]
    Month,
    #[serde(rename = "1y")]
    Year,
}

impl HistoryRange {
    pub fn window_secs(self) -> i64 {
        match self {
            HistoryRange::Day => 600,
            HistoryRange::Week => 3_600,
            HistoryRange::Month => 18_000,
            HistoryRange::Year => 86_400,
        }
    }

    pub fn span_secs(self) -> i64 {
        match self {
            HistoryRange::Day => 86_400,
            HistoryRange::Week => 604_800,
            HistoryRange::Month => 2_592_000,
            HistoryRange::Year => 31_536_000,
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "24h" => Some(HistoryRange::Day),
            "7d" => Some(HistoryRange::Week),
            "30d" => Some(HistoryRange::Month),
            "1y" => Some(HistoryRange::Year),
            _ => None,
        }
    }
}

/// A chart series: window width in seconds, then one `[t, mean, min, max]` per point.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct HistorySeries {
    pub window: i64,
    pub points: Vec<(i64, f64, f64, f64)>,
}

/// kWh per calendar day in the house zone.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct HistoryDaily {
    pub days: Vec<(i64, f64)>,
}

/// kWh since local midnight for the current day.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct HistoryToday {
    pub kwh: Option<f64>,
    pub since: i64,
}

/// One entity's `last_change` row: the latest value time if any, and whether the entity
/// has rows at all.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct LastChangeItem {
    pub changed_at: Option<i64>,
    pub recorded: bool,
}

/// `GET /history/last_change` reply, one item per requested id in order.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct LastChange {
    pub items: Vec<LastChangeItem>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn series_serialises_points_as_arrays() {
        let series = HistorySeries {
            window: 600,
            points: vec![(1, 2.0, 1.0, 3.0)],
        };
        assert_eq!(
            serde_json::to_value(&series).unwrap(),
            json!({"window": 600, "points": [[1, 2.0, 1.0, 3.0]]})
        );
    }

    #[test]
    fn range_parses_wire_names() {
        assert_eq!(HistoryRange::parse("1y"), Some(HistoryRange::Year));
        assert_eq!(HistoryRange::parse("24h"), Some(HistoryRange::Day));
        assert_eq!(HistoryRange::parse("2d"), None);
    }
}
