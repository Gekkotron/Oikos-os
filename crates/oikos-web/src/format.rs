//! Human-readable state values for cards.

use oikos_api::{AreaView, EntityView};
use oikos_core::model::{EntityState, GenericValue, Kind, StateValue, SwitchType, UNASSIGNED_AREA};
use oikos_core::types::{GenericCaps, TypeDef};

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
        StateValue::Generic(_) => generic_value(None, None, state),
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

/// `id` with a zero-width space after every `_` and `/`, so a long raw id wraps at those
/// boundaries (with `overflow-wrap: break-word`) instead of in the middle of a word. For display
/// only: never put the result in an attribute, a URL or something the user copies.
pub fn wrap_id(id: &str) -> String {
    let mut out = String::with_capacity(id.len() + 8);
    for c in id.chars() {
        out.push(c);
        if matches!(c, '_' | '/') {
            out.push('\u{200B}');
        }
    }
    out
}

/// "binary_sensor" as "Binary sensor".
pub fn kind_word(kind: Kind) -> String {
    serde_json::to_value(kind)
        .ok()
        .and_then(|v| v.as_str().map(word))
        .unwrap_or_default()
}

/// The word for what an entity is: the type a switch is shown as ("Door opener"), else its kind.
pub fn type_word(entity: &EntityView) -> String {
    shown_type(entity).map_or_else(|| kind_word(entity.kind), str::to_owned)
}

/// The label of the type a switch is shown as, unless it is a plain switch.
fn shown_type(entity: &EntityView) -> Option<&'static str> {
    SwitchType::of(entity.protocol_kind, &entity.ui)
        .filter(|t| *t != SwitchType::Switch)
        .map(SwitchType::label)
}

/// What tells an entity apart from its same-named siblings: set when the entity is named exactly
/// like its device (a siren whose four entities are all "door_siren"), then its device class
/// ("Siren"), else its kind ("Binary sensor"). `None` for an entity with a name of its own, and
/// when the suffix would repeat the name (a "Water heater" device's water heater).
pub fn title_suffix(entity: &EntityView) -> Option<String> {
    let device = entity.device_name.as_deref().filter(|d| !d.is_empty())?;
    if !entity.name.eq_ignore_ascii_case(device) {
        return None;
    }
    let suffix = match shown_type(entity) {
        Some(shown) => shown.to_owned(),
        None => entity
            .device_class
            .as_deref()
            .filter(|c| !c.is_empty())
            .map_or_else(|| kind_word(entity.kind), word),
    };
    (!suffix.eq_ignore_ascii_case(&entity.name)).then_some(suffix)
}

/// The card title: [`short_name`] plus the [`title_suffix`] when there is one.
pub fn entity_title(entity: &EntityView) -> String {
    let short = short_name(&entity.name, entity.device_name.as_deref());
    match title_suffix(entity) {
        Some(suffix) => format!("{short} · {suffix}"),
        None => short,
    }
}

/// The label of an entity in a list of links: its title and its kind, where the suffix of a
/// same-named entity stands in for the kind.
pub fn list_label(entity: &EntityView) -> String {
    let short = short_name(&entity.name, entity.device_name.as_deref());
    let tail = title_suffix(entity).unwrap_or_else(|| type_word(entity));
    if tail.eq_ignore_ascii_case(&short) {
        return short;
    }
    format!("{short} · {tail}")
}

/// The name of room `area`, or `None` outside any room. An id the house no longer lists stands
/// for its own name.
pub fn room_name(areas: &[AreaView], area: &str) -> Option<String> {
    (area != UNASSIGNED_AREA).then(|| {
        areas
            .iter()
            .find(|a| a.id == area)
            .map_or_else(|| area.to_owned(), |a| a.name.clone())
    })
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

// Until every caller passes the UI language (Task 3 of the FR/EN plan), values use English.
fn number(n: f64) -> String {
    crate::i18n::format::number(crate::i18n::Lang::En, n)
}

use crate::i18n::format::with_unit;

/// Unix seconds as ISO 8601 UTC ("2026-10-19T09:41:00Z"); "—" when out of range.
pub fn format_timestamp(seconds: u64) -> String {
    let format =
        time::macros::format_description!("[year]-[month]-[day]T[hour]:[minute]:[second]Z");
    i64::try_from(seconds)
        .ok()
        .and_then(|s| time::OffsetDateTime::from_unix_timestamp(s).ok())
        .and_then(|t| t.format(&format).ok())
        .unwrap_or_else(|| DASH.into())
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

/// A canonical choice value as shown on a card: `not_home` is "Away", `home` "Home", the rest
/// sentence case.
pub fn choice_word(s: &str) -> String {
    match s {
        "not_home" => "Away".into(),
        other => word(other),
    }
}

/// The value a generic card shows: the type's primary field (a lone or `state` field without a
/// type), "••••" for a secret, "" for a picture, "—" while nothing is known.
pub fn generic_value(
    def: Option<&TypeDef>,
    caps: Option<&GenericCaps>,
    state: &EntityState,
) -> String {
    if caps.is_some_and(|c| c.secret) {
        return "••••".into();
    }
    let StateValue::Generic(g) = &state.value else {
        return DASH.into();
    };
    let value = match def {
        Some(d) => {
            if let Some(field) = d.primary.as_deref().and_then(|p| d.fields.get(p))
                && field.shape == oikos_core::types::Shape::Picture
            {
                return String::new();
            }
            oikos_core::types::state::primary(d, g)
        }
        None => g.fields.get("state").or_else(|| {
            (g.fields.len() == 1)
                .then(|| g.fields.values().next())
                .flatten()
        }),
    };
    match value {
        Some(GenericValue::Bool(true)) => "On".into(),
        Some(GenericValue::Bool(false)) => "Off".into(),
        Some(GenericValue::Number(n)) => number(*n),
        Some(GenericValue::Text(s)) if is_unusable(s) || s.is_empty() => DASH.into(),
        Some(GenericValue::Text(s)) => choice_word(s),
        None => DASH.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::{display_value, entity_title, list_label, wrap_id};
    use oikos_api::EntityView;
    use oikos_core::model::{
        Capabilities, ClimateState, EntityCategory, EntityState, Kind, LightState, StateValue,
        UiOverride,
    };

    fn entity(name: &str, device: &str, kind: Kind, class: Option<&str>) -> EntityView {
        EntityView {
            id: "x".into(),
            device_id: None,
            name: name.into(),
            device_name: Some(device.into()),
            label: None,
            kind,
            component: kind.component().to_string(),
            protocol_kind: kind,
            device_class: class.map(Into::into),
            unit: None,
            icon: None,
            category: EntityCategory::Primary,
            enabled_by_default: true,
            area: "salon".into(),
            part_of: None,
            capabilities: Capabilities::default(),
            ui: UiOverride::default(),
            state: EntityState::default(),
        }
    }

    #[test]
    fn unnamed_relay_titles_follow_the_type() {
        let mut gate = entity("gate", "gate", Kind::Switch, Some("outlet"));
        gate.ui.show_as = Some("door".into());
        assert_eq!(entity_title(&gate), "gate · Door opener");
        assert_eq!(list_label(&gate), "gate · Door opener");
        let mut named = entity("Gate relay", "gate", Kind::Switch, None);
        named.ui.show_as = Some("siren".into());
        assert_eq!(list_label(&named), "Relay · Siren");
        let plain = entity("gate", "gate", Kind::Switch, Some("outlet"));
        assert_eq!(entity_title(&plain), "gate · Outlet");
    }

    #[test]
    fn type_word_names_the_switch_type() {
        let mut door = entity("gate", "gate", Kind::Switch, None);
        door.ui.show_as = Some("door".into());
        assert_eq!(super::type_word(&door), "Door opener");
        assert_eq!(
            super::type_word(&entity("s", "s", Kind::Switch, None)),
            "Switch"
        );
        assert_eq!(
            super::type_word(&entity("l", "l", Kind::Light, None)),
            "Light"
        );
    }

    #[test]
    fn ids_break_after_underscores_and_slashes() {
        assert_eq!(wrap_id("door_siren/x"), "door_\u{200B}siren/\u{200B}x");
        assert_eq!(wrap_id("plain"), "plain");
    }

    #[test]
    fn same_named_entity_gets_its_class_or_kind() {
        let siren = entity(
            "door_siren",
            "door_siren",
            Kind::BinarySensor,
            Some("tamper"),
        );
        assert_eq!(entity_title(&siren), "door_siren · Tamper");
        assert_eq!(list_label(&siren), "door_siren · Tamper");
        let plain = entity("door_siren", "Door_Siren", Kind::Switch, None);
        assert_eq!(entity_title(&plain), "door_siren · Switch");
    }

    #[test]
    fn suffix_that_repeats_the_name_is_dropped() {
        let heater = entity("Water heater", "Water heater", Kind::WaterHeater, None);
        assert_eq!(entity_title(&heater), "Water heater");
        assert_eq!(list_label(&heater), "Water heater");
    }

    #[test]
    fn distinct_names_are_untouched() {
        let e = entity(
            "powers/ac energy",
            "powers/ac",
            Kind::Sensor,
            Some("energy"),
        );
        assert_eq!(entity_title(&e), "Energy");
        assert_eq!(list_label(&e), "Energy · Sensor");
    }

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
        assert_eq!(display_value(&s, Some("°C")), "21\u{a0}°C");
    }
    #[test]
    fn fractional_number_keeps_one_decimal() {
        let s = EntityState {
            value: StateValue::Number(21.47),
            ..Default::default()
        };
        assert_eq!(display_value(&s, Some("°C")), "21.5\u{a0}°C");
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
        assert_eq!(display_value(&s, Some("%")), "80\u{a0}%");
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
        assert_eq!(display_value(&s, None), "22 → 15\u{a0}°C");
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
        assert_eq!(display_value(&s, Some("°C")), "20.5 → 21\u{a0}°C");
    }

    #[test]
    fn thousands_have_no_decimals_and_a_thin_space() {
        let s = EntityState {
            value: StateValue::Number(2199.34),
            ..Default::default()
        };
        assert_eq!(display_value(&s, Some("kWh")), "2\u{2009}199\u{a0}kWh");
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
        assert_eq!(super::degrees(Some(21.0), None), "21\u{a0}°C");
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

#[cfg(test)]
mod ts_tests {
    use super::format_timestamp;
    use oikos_core::model::{EntityState, GenericValue, StateValue};

    #[test]
    fn unix_epoch_formats_as_iso() {
        assert_eq!(format_timestamp(0), "1970-01-01T00:00:00Z");
    }

    #[test]
    fn a_recent_timestamp_formats_in_utc() {
        assert_eq!(format_timestamp(1_792_402_860), "2026-10-19T09:41:00Z");
    }

    fn generic_state(field: &str, v: GenericValue) -> EntityState {
        let mut g = oikos_core::model::GenericState::default();
        g.fields.insert(field.into(), v);
        EntityState {
            value: StateValue::Generic(g),
            ..Default::default()
        }
    }

    #[test]
    fn generic_value_humanises_choices() {
        use super::generic_value;
        use oikos_core::model::GenericValue::Text;
        let lock = oikos_core::types::type_def("lock");
        let tracker = oikos_core::types::type_def("device_tracker");
        assert_eq!(
            generic_value(lock, None, &generic_state("state", Text("locked".into()))),
            "Locked"
        );
        assert_eq!(
            generic_value(
                tracker,
                None,
                &generic_state("state", Text("not_home".into()))
            ),
            "Away"
        );
        assert_eq!(
            generic_value(tracker, None, &generic_state("state", Text("home".into()))),
            "Home"
        );
    }

    #[test]
    fn generic_value_hides_secrets() {
        use super::generic_value;
        let caps = oikos_core::types::GenericCaps {
            secret: true,
            ..Default::default()
        };
        let s = generic_state("state", GenericValue::Text("hunter2".into()));
        assert_eq!(
            generic_value(oikos_core::types::type_def("text"), Some(&caps), &s),
            "••••"
        );
    }

    #[test]
    fn room_name_resolves_ids_and_skips_unassigned() {
        use super::room_name;
        use oikos_api::AreaView;
        let areas = vec![AreaView {
            id: "salon".into(),
            name: "Living room".into(),
            icon: None,
            order: None,
            entity_count: 0,
            primary_count: 0,
            group_names: Default::default(),
            group_favorites: Default::default(),
        }];
        assert_eq!(room_name(&areas, "salon").as_deref(), Some("Living room"));
        assert_eq!(room_name(&areas, "gone").as_deref(), Some("gone"));
        assert_eq!(room_name(&areas, oikos_core::model::UNASSIGNED_AREA), None);
    }
}
