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
        StateValue::Text(s) => with_unit(date_time(s).unwrap_or_else(|| s.clone()), unit),
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

/// A temperature as "21 °C" (Celsius when `unit` is unknown), "—" when there is none.
pub fn degrees(value: Option<f64>, unit: Option<&str>) -> String {
    let unit = unit.filter(|u| !u.is_empty()).unwrap_or("°C");
    value.map_or_else(|| DASH.into(), |v| with_unit(number(v), Some(unit)))
}

/// The CSS classes of a card's value line: values over nine characters get the smaller,
/// wrapping `card__value--long` instead of being ellipsised.
pub fn value_class(text: &str) -> &'static str {
    if text.chars().count() > 9 {
        "card__value card__value--long"
    } else {
        "card__value"
    }
}

/// The card title: the entity name without its device name prefix ("powers/ac energy" on
/// device "powers/ac" becomes "Energy"); the full name when there is no such prefix.
pub fn short_name(name: &str, device_name: Option<&str>) -> String {
    const SEPARATORS: &[char] = &[' ', '_', '/', '-'];
    let Some(device) = device_name.filter(|d| !d.is_empty()) else {
        return name.to_owned();
    };
    let mut rest = name.chars();
    for expected in device.chars() {
        match rest.next() {
            Some(c) if c.to_lowercase().eq(expected.to_lowercase()) => {}
            _ => return name.to_owned(),
        }
    }
    let rest = rest.as_str();
    if !rest.starts_with(SEPARATORS) {
        return name.to_owned();
    }
    let rest = rest.trim_matches(SEPARATORS);
    let mut chars = rest.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => name.to_owned(),
    }
}

/// An RFC 3339 date-time as "2026-10-19 09:41" (the time as sent, no zone conversion).
fn date_time(s: &str) -> Option<String> {
    let b = s.as_bytes();
    let digits = [0, 1, 2, 3, 5, 6, 8, 9, 11, 12, 14, 15];
    let shaped = b.len() >= 19
        && digits.iter().all(|&i| b[i].is_ascii_digit())
        && b[4] == b'-'
        && b[7] == b'-'
        && matches!(b[10], b'T' | b't')
        && b[13] == b':'
        && b[16] == b':'
        && b[17].is_ascii_digit()
        && b[18].is_ascii_digit()
        && s.is_char_boundary(16);
    shaped.then(|| format!("{} {}", &s[..10], &s[11..16]))
}

/// Integers without a decimal, everything else with one; from 1000 up no decimals and a thin
/// space between thousands.
fn number(n: f64) -> String {
    if !n.is_finite() {
        DASH.into()
    } else if n.abs() >= 1000.0 {
        let digits = format!("{:.0}", n.abs());
        let mut out = String::new();
        for (i, c) in digits.chars().enumerate() {
            if i > 0 && (digits.len() - i) % 3 == 0 {
                out.push('\u{2009}');
            }
            out.push(c);
        }
        if n < 0.0 { format!("-{out}") } else { out }
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
pub fn word(s: &str) -> String {
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

    #[test]
    fn thousands_have_no_decimals_and_a_thin_space() {
        let s = EntityState {
            value: StateValue::Number(2199.34),
            ..Default::default()
        };
        assert_eq!(display_value(&s, Some("kWh")), "2\u{2009}199 kWh");
        let big = EntityState {
            value: StateValue::Number(-1234567.0),
            ..Default::default()
        };
        assert_eq!(display_value(&big, None), "-1\u{2009}234\u{2009}567");
        let under = EntityState {
            value: StateValue::Number(999.5),
            ..Default::default()
        };
        assert_eq!(display_value(&under, None), "999.5");
    }

    #[test]
    fn rfc3339_text_renders_as_date_and_time() {
        let s = EntityState {
            value: StateValue::Text("2026-10-19T09:41:12+02:00".into()),
            ..Default::default()
        };
        assert_eq!(display_value(&s, None), "2026-10-19 09:41");
        let plain = EntityState {
            value: StateValue::Text("2026-10-19 is a date".into()),
            ..Default::default()
        };
        assert_eq!(display_value(&plain, None), "2026-10-19 is a date");
    }

    #[test]
    fn degrees_defaults_to_celsius() {
        assert_eq!(super::degrees(Some(21.0), None), "21 °C");
        assert_eq!(super::degrees(None, None), "—");
    }

    #[test]
    fn long_values_get_the_long_class() {
        assert_eq!(super::value_class("42 W"), "card__value");
        assert_eq!(
            super::value_class("2026-10-19 09:41"),
            "card__value card__value--long"
        );
    }

    #[test]
    fn short_name_drops_the_device_prefix() {
        use super::short_name;
        assert_eq!(short_name("powers/ac energy", Some("powers/ac")), "Energy");
        assert_eq!(short_name("Lumière table", Some("lumière")), "Table");
        assert_eq!(short_name("Salonette", Some("Salon")), "Salonette");
        assert_eq!(short_name("Salon", Some("Salon")), "Salon");
        assert_eq!(short_name("Lamp", None), "Lamp");
        assert_eq!(short_name("Lamp", Some("Other device")), "Lamp");
    }
}
