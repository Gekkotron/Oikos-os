//! The sensor card: the big value and its unit.

use leptos::prelude::*;
use leptos_router::components::A;
use oikos_api::EntityView;
use oikos_core::model::{EntityState, Kind, StateValue};

use super::Sender;
use crate::components::card_frame::{CardFrame, open_entity};
use crate::format::{degrees, display_value, short_name, value_class};
use crate::house::House;
use crate::icons::DROP;
use crate::rooms::shows_on_home;

/// The text a sensor shows: the value with its unit, "—" while unknown or unavailable.
pub fn sensor_text(state: &EntityState, unit: Option<&str>) -> String {
    display_value(state, unit)
}

/// The number a sensor reports, whether sent as a number or as numeric text.
fn numeric(state: &EntityState) -> Option<f64> {
    match &state.value {
        StateValue::Number(n) if n.is_finite() => Some(*n),
        StateValue::Text(t) => t.trim().parse::<f64>().ok().filter(|n| n.is_finite()),
        _ => None,
    }
}

/// The comfort tint of a temperature: below 18 cold, 18 to 24 comfortable, above 24 warm.
fn comfort_class(celsius: f64) -> &'static str {
    if celsius < 18.0 {
        "readout__temp--cold"
    } else if celsius <= 24.0 {
        "readout__temp--ok"
    } else {
        "readout__temp--warm"
    }
}

/// The width of the humidity bar fill, clamped to 0..=100.
fn bar_percent(humidity: Option<f64>) -> u32 {
    humidity.map_or(0, |h| h.round().clamp(0.0, 100.0) as u32)
}

/// The humidity as "66 %", "—" when unknown.
fn humidity_text(humidity: Option<f64>) -> String {
    humidity.map_or_else(|| "—".into(), |h| format!("{} %", h.round()))
}

/// The sensor of `class` on `device_id` that Home shows, if any.
fn partner<'a>(
    entities: impl Iterator<Item = &'a EntityView>,
    device_id: Option<&str>,
    class: &str,
) -> Option<&'a EntityView> {
    let device_id = device_id?;
    entities.into_iter().find(|e| {
        e.kind == Kind::Sensor
            && e.device_class.as_deref() == Some(class)
            && e.device_id.as_deref() == Some(device_id)
            && shows_on_home(e)
    })
}

/// The humidity bar under a reading.
fn humidity_view(
    humidity: impl Fn() -> Option<f64> + Send + Sync + Copy + 'static,
) -> impl IntoView {
    view! {
        <div class="readout__humidity">
            <div class="readout__humidity-row">
                <span class="readout__drop" inner_html=DROP></span>
                <span>{move || humidity_text(humidity())}</span>
            </div>
            <div class="readout__bar">
                <div
                    class="readout__fill"
                    style:width=move || format!("{}%", bar_percent(humidity()))
                ></div>
            </div>
        </div>
    }
}

/// The href of the history chart for `id`.
fn history_href(id: &str) -> String {
    format!("/history/{}", crate::api::encode_id(id))
}

/// The body of a temperature card: the temperature, tinted by comfort, and the humidity of the
/// same device under it when Home shows one. The temperature readout and the humidity bar are
/// separate links so each opens its own history chart.
fn temperature_body(entity: &EntityView, house: House) -> impl IntoView + use<> {
    let value = numeric(&entity.state);
    let class = match value {
        Some(v) => format!("readout__temp {}", comfort_class(v)),
        None => "readout__temp".into(),
    };
    let text = degrees(value, entity.unit.as_deref());
    let temp_href = history_href(&entity.id);
    let device_id = entity.device_id.clone();
    let device_id_val = device_id.clone();
    let humidity_id: Memo<Option<String>> = Memo::new(move |_| {
        house
            .entities
            .with(|m| partner(m.values(), device_id.as_deref(), "humidity").map(|e| e.id.clone()))
    });
    let humidity_value: Memo<Option<f64>> = Memo::new(move |_| {
        house.entities.with(|m| {
            partner(m.values(), device_id_val.as_deref(), "humidity")
                .and_then(|e| numeric(&e.state))
        })
    });
    view! {
        <div class="readout">
            <A href=temp_href attr:class="readout__link">
                <span class=class>{text}</span>
            </A>
            {move || humidity_id.get().map(|id| {
                let href = history_href(&id);
                view! {
                    <A href=href attr:class="readout__link">
                        {humidity_view(move || humidity_value.get())}
                    </A>
                }
            })}
        </div>
    }
}

/// The body of a lone humidity card. The readout itself links to the history chart.
fn humidity_body(entity: &EntityView) -> impl IntoView + use<> {
    let value = numeric(&entity.state);
    let href = history_href(&entity.id);
    view! {
        <div class="readout">
            <A href=href attr:class="readout__link">
                <span class="readout__temp">{humidity_text(value)}</span>
                {humidity_view(move || value)}
            </A>
        </div>
    }
}

#[component]
pub fn SensorCard(entity: EntityView) -> impl IntoView {
    let sender = Sender::from_context();
    let text = sensor_text(&entity.state, entity.unit.as_deref());
    let class = match value_class(&text) {
        "card__value" => "card__value card__value--big",
        long => long,
    };
    let pending = sender.pending(&entity.id);
    let on_click = open_entity(&entity.id);
    if entity.kind == Kind::Sensor {
        match entity.device_class.as_deref() {
            Some("temperature") => {
                let house = expect_context::<House>();
                let paired = house.entities.with_untracked(|m| {
                    partner(m.values(), entity.device_id.as_deref(), "humidity").is_some()
                });
                let title = if paired {
                    short_name(&entity.name, entity.device_name.as_deref())
                } else {
                    String::new()
                };
                let body = temperature_body(&entity, house);
                return view! {
                    <CardFrame entity=entity on_click=on_click pending=pending title=title>
                        {body}
                    </CardFrame>
                }
                .into_any();
            }
            Some("humidity") => {
                let body = humidity_body(&entity);
                return view! {
                    <CardFrame entity=entity on_click=on_click pending=pending>{body}</CardFrame>
                }
                .into_any();
            }
            _ => {}
        }
    }
    view! {
        <CardFrame entity=entity on_click=on_click pending=pending>
            <span class=class>{text}</span>
        </CardFrame>
    }
    .into_any()
}

#[cfg(test)]
mod tests {
    use super::{bar_percent, comfort_class, humidity_text, numeric, partner, sensor_text};
    use oikos_api::EntityView;
    use oikos_core::model::{
        Capabilities, EntityCategory, EntityState, Kind, StateValue, UiOverride,
    };

    fn sensor(id: &str, device: Option<&str>, class: &str, value: f64) -> EntityView {
        EntityView {
            id: id.into(),
            device_id: device.map(Into::into),
            name: id.into(),
            device_name: None,
            label: None,
            kind: Kind::Sensor,
            component: "sensor".to_string(),
            protocol_kind: Kind::Sensor,
            device_class: Some(class.into()),
            unit: None,
            icon: None,
            category: EntityCategory::Primary,
            enabled_by_default: true,
            area: "living".into(),
            part_of: None,
            capabilities: Capabilities::default(),
            ui: UiOverride::default(),
            state: EntityState {
                value: StateValue::Number(value),
                ..Default::default()
            },
        }
    }

    #[test]
    fn comfort_bands() {
        assert_eq!(comfort_class(17.9), "readout__temp--cold");
        assert_eq!(comfort_class(18.0), "readout__temp--ok");
        assert_eq!(comfort_class(24.0), "readout__temp--ok");
        assert_eq!(comfort_class(24.1), "readout__temp--warm");
    }

    #[test]
    fn bar_is_clamped_and_empty_when_unknown() {
        assert_eq!(bar_percent(None), 0);
        assert_eq!(bar_percent(Some(-5.0)), 0);
        assert_eq!(bar_percent(Some(66.4)), 66);
        assert_eq!(bar_percent(Some(140.0)), 100);
    }

    #[test]
    fn humidity_text_rounds() {
        assert_eq!(humidity_text(Some(65.6)), "66 %");
        assert_eq!(humidity_text(None), "—");
    }

    #[test]
    fn numeric_reads_numbers_and_numeric_text() {
        let text = |t: &str| EntityState {
            value: StateValue::Text(t.into()),
            ..Default::default()
        };
        assert_eq!(numeric(&text("21.5")), Some(21.5));
        assert_eq!(numeric(&text("unknown")), None);
        assert_eq!(numeric(&EntityState::default()), None);
    }

    #[test]
    fn partner_matches_class_and_device() {
        let all = [
            sensor("t", Some("dev"), "temperature", 20.0),
            sensor("h_other", Some("other"), "humidity", 50.0),
            sensor("h", Some("dev"), "humidity", 60.0),
        ];
        let found = partner(all.iter(), Some("dev"), "humidity");
        assert_eq!(found.map(|e| e.id.as_str()), Some("h"));
        assert!(partner(all.iter(), None, "humidity").is_none());
        assert!(partner(all.iter(), Some("none"), "humidity").is_none());
    }

    #[test]
    fn partner_must_be_shown_on_home() {
        let mut hidden = sensor("h", Some("dev"), "humidity", 60.0);
        hidden.ui.hidden = true;
        assert!(partner([hidden].iter(), Some("dev"), "humidity").is_none());
    }

    #[test]
    fn unknown_sensor_text() {
        let s = EntityState {
            value: StateValue::Text("unknown".into()),
            ..Default::default()
        };
        assert_eq!(sensor_text(&s, Some("W")), "—");
    }
    #[test]
    fn numeric_sensor_text() {
        let s = EntityState {
            value: StateValue::Number(42.0),
            ..Default::default()
        };
        assert_eq!(sensor_text(&s, Some("W")), "42 W");
    }
}
