//! Human-readable state values for cards.

use oikos_core::model::{EntityState, StateValue};

const DASH: &str = "—";

/// The value a card shows: "On", "21.5 °C", "—" while nothing usable is known.
pub fn display_value(state: &EntityState, unit: Option<&str>) -> String {
    match &state.value {
        StateValue::OnOff(true) => "On".into(),
        StateValue::OnOff(false) => "Off".into(),
        StateValue::Number(n) => with_unit(number(*n), unit),
        StateValue::Text(s) if is_unusable(s) => DASH.into(),
        StateValue::Text(s) => with_unit(s.clone(), unit),
        StateValue::Light(l) => match (l.on, l.brightness) {
            (true, Some(b)) => format!("On · {} %", (u32::from(b) * 100 + 127) / 255),
            (true, None) => "On".into(),
            (false, _) => "Off".into(),
        },
        StateValue::Cover(c) => {
            let word = c.state.as_deref().map(word);
            match (word, c.position) {
                (Some(w), Some(p)) => format!("{w} · {p} %"),
                (Some(w), None) => w,
                (None, Some(p)) => format!("{p} %"),
                (None, None) => DASH.into(),
            }
        }
        StateValue::Climate(c) => {
            // The hub sends no unit for climates; Home Assistant's default is Celsius.
            let unit = unit.filter(|u| !u.is_empty()).unwrap_or("°C");
            match (c.current, c.target) {
                (Some(cur), Some(tgt)) => {
                    format!("{} → {}", number(cur), with_unit(number(tgt), Some(unit)))
                }
                (Some(v), None) | (None, Some(v)) => with_unit(number(v), Some(unit)),
                (None, None) => c.mode.as_deref().map_or_else(|| DASH.into(), word),
            }
        }
        StateValue::Alarm(s) => word(s),
        StateValue::Vacuum(v) => v.state.as_deref().map_or_else(|| DASH.into(), word),
        StateValue::WaterHeater(w) => match (w.current, w.target) {
            (Some(v), _) | (None, Some(v)) => with_unit(number(v), unit),
            (None, None) => w.mode.as_deref().map_or_else(|| DASH.into(), word),
        },
        StateValue::Unknown => DASH.into(),
    }
}

/// True when the entity is in an "on" state, which cards highlight.
pub fn is_on(state: &EntityState) -> bool {
    match &state.value {
        StateValue::OnOff(on) => *on,
        StateValue::Light(l) => l.on,
        StateValue::Cover(c) => {
            c.state.as_deref() == Some("open") || c.position.is_some_and(|p| p > 0)
        }
        _ => false,
    }
}

fn is_unusable(s: &str) -> bool {
    s.eq_ignore_ascii_case("unknown") || s.eq_ignore_ascii_case("unavailable")
}

/// Integers without a decimal, everything else with one.
fn number(n: f64) -> String {
    if !n.is_finite() {
        DASH.into()
    } else if n.fract() == 0.0 {
        format!("{n:.0}")
    } else {
        format!("{n:.1}")
    }
}

fn with_unit(value: String, unit: Option<&str>) -> String {
    match unit {
        Some(u) if !u.is_empty() && value != DASH => format!("{value} {u}"),
        _ => value,
    }
}

/// `heat_cool` becomes "Heat cool".
fn word(s: &str) -> String {
    let spaced = s.replace('_', " ");
    let mut chars = spaced.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => DASH.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::display_value;
    use oikos_core::model::{ClimateState, EntityState, LightState, StateValue};
    #[test]
    fn unknown_text_renders_as_dash() {
        let s = EntityState {
            value: StateValue::Text("unknown".into()),
            ..Default::default()
        };
        assert_eq!(display_value(&s, None), "—");
    }
    #[test]
    fn unavailable_text_renders_as_dash() {
        let s = EntityState {
            value: StateValue::Text("UnAvAiLaBlE".into()),
            ..Default::default()
        };
        assert_eq!(display_value(&s, None), "—");
    }
    #[test]
    fn integer_number_omits_decimal() {
        let s = EntityState {
            value: StateValue::Number(21.0),
            ..Default::default()
        };
        assert_eq!(display_value(&s, Some("°C")), "21 °C");
    }
    #[test]
    fn fractional_number_keeps_one_decimal() {
        let s = EntityState {
            value: StateValue::Number(21.47),
            ..Default::default()
        };
        assert_eq!(display_value(&s, Some("°C")), "21.5 °C");
    }
    #[test]
    fn on_off_renders_on_and_off() {
        let s_on = EntityState {
            value: StateValue::OnOff(true),
            ..Default::default()
        };
        let s_off = EntityState {
            value: StateValue::OnOff(false),
            ..Default::default()
        };
        assert_eq!(display_value(&s_on, None), "On");
        assert_eq!(display_value(&s_off, None), "Off");
    }
    #[test]
    fn unknown_variant_renders_as_dash() {
        let s = EntityState {
            value: StateValue::Unknown,
            ..Default::default()
        };
        assert_eq!(display_value(&s, None), "—");
    }
    #[test]
    fn is_on_covers_switches_and_lights() {
        let on = EntityState {
            value: StateValue::OnOff(true),
            ..Default::default()
        };
        let l = LightState {
            on: false,
            ..Default::default()
        };
        let off = EntityState {
            value: StateValue::Light(l),
            ..Default::default()
        };
        assert!(super::is_on(&on));
        assert!(!super::is_on(&off));
    }
    #[test]
    fn percent_unit_is_spaced() {
        let s = EntityState {
            value: StateValue::Number(80.0),
            ..Default::default()
        };
        assert_eq!(display_value(&s, Some("%")), "80 %");
    }
    #[test]
    fn light_shows_brightness_when_on() {
        let l = LightState {
            on: true,
            brightness: Some(128),
            ..Default::default()
        };
        let s = EntityState {
            value: StateValue::Light(l),
            ..Default::default()
        };
        assert_eq!(display_value(&s, None), "On · 50 %");
    }
    #[test]
    fn alarm_state_is_capitalised_without_underscores() {
        let s = EntityState {
            value: StateValue::Alarm("armed_away".into()),
            ..Default::default()
        };
        assert_eq!(display_value(&s, None), "Armed away");
    }
    #[test]
    fn climate_defaults_to_celsius_when_unit_unknown() {
        let c = ClimateState {
            current: Some(22.0),
            target: Some(15.0),
            ..Default::default()
        };
        let s = EntityState {
            value: StateValue::Climate(c),
            ..Default::default()
        };
        assert_eq!(display_value(&s, None), "22 → 15 °C");
    }
    #[test]
    fn climate_shows_current_and_target() {
        let c = ClimateState {
            current: Some(20.5),
            target: Some(21.0),
            ..Default::default()
        };
        let s = EntityState {
            value: StateValue::Climate(c),
            ..Default::default()
        };
        assert_eq!(display_value(&s, Some("°C")), "20.5 → 21 °C");
    }
}
