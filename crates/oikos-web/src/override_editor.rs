//! The override editor: one input per field of the hub's `UiOverride`, saved as a partial patch.

use leptos::prelude::*;
use leptos::task::spawn_local;
use oikos_api::{EntityView, OverridePatch};
use oikos_core::model::{ENERGY_ROLES, Kind, LIGHT_ON_PRESETS, UiOverride};
use serde_json::json;

use crate::api::{clear_override, set_override};
use crate::components::toast::Toasts;
use crate::house::House;

/// The fields of `after` that differ from `before`, as a patch the hub accepts: camelCase keys,
/// a cleared `Option` as `null`, flags as booleans, `order` and the brightness as numbers.
/// `extra` (fields written by a newer app) is not editable here and is ignored.
pub fn dirty_patch(before: &UiOverride, after: &UiOverride) -> OverridePatch {
    let mut patch = OverridePatch::new();
    macro_rules! diff {
        ($($field:ident => $key:literal),* $(,)?) => {
            $(
                if before.$field != after.$field {
                    patch.insert($key.to_owned(), json!(after.$field));
                }
            )*
        };
    }
    diff! {
        name => "name",
        area => "area",
        hidden => "hidden",
        order => "order",
        card_style => "cardStyle",
        favorite => "favorite",
        energy_role => "energyRole",
        energy_invert => "energyInvert",
        security_hidden => "securityHidden",
        energy_icon => "energyIcon",
        energy_meter => "energyMeter",
        home_pinned => "homePinned",
        security_action => "securityAction",
        security_ask => "securityAsk",
        ungrouped => "ungrouped",
        as_light => "asLight",
        light_on_preset => "lightOnPreset",
        light_on_brightness => "lightOnBrightness",
        climate_presets => "climatePresets",
    }
    patch
}

type Draft = RwSignal<UiOverride>;
type Text = Option<String>;

/// Blank input is "not set".
fn non_blank(s: String) -> Text {
    (!s.trim().is_empty()).then_some(s)
}

#[component]
fn TextField(
    label: &'static str,
    draft: Draft,
    get: fn(&UiOverride) -> Text,
    set: fn(&mut UiOverride, Text),
) -> impl IntoView {
    view! {
        <label class="override__field">
            <span class="override__label">{label}</span>
            <input
                class="field"
                type="text"
                prop:value=move || draft.with(|d| get(d).unwrap_or_default())
                on:input=move |ev| {
                    let value = non_blank(event_target_value(&ev));
                    draft.update(|d| set(d, value));
                }
            />
        </label>
    }
}

#[component]
fn IntField(
    label: &'static str,
    draft: Draft,
    get: fn(&UiOverride) -> Option<i32>,
    set: fn(&mut UiOverride, Option<i32>),
) -> impl IntoView {
    view! {
        <label class="override__field">
            <span class="override__label">{label}</span>
            <input
                class="field"
                type="number"
                step="1"
                prop:value=move || draft.with(|d| get(d).map(|n| n.to_string()).unwrap_or_default())
                on:input=move |ev| {
                    let raw = event_target_value(&ev);
                    let raw = raw.trim();
                    if raw.is_empty() {
                        draft.update(|d| set(d, None));
                    } else if let Ok(n) = raw.parse::<i32>() {
                        draft.update(|d| set(d, Some(n)));
                    }
                }
            />
        </label>
    }
}

#[component]
fn FlagField(
    label: &'static str,
    draft: Draft,
    get: fn(&UiOverride) -> bool,
    set: fn(&mut UiOverride, bool),
) -> impl IntoView {
    let on = move || draft.with(get);
    view! {
        <label class="override__flag">
            <span class="override__label">{label}</span>
            <button
                type="button"
                role="switch"
                class="switch"
                class:switch--on=on
                aria-checked=move || on().to_string()
                on:click=move |_| {
                    let next = !draft.with_untracked(get);
                    draft.update(|d| set(d, next));
                }
            >
                <span class="switch__knob"></span>
            </button>
        </label>
    }
}

/// A `<select>` over a fixed list of values, with "Default" for none. A stored value outside the
/// list (a role a newer app wrote) stays selectable.
#[component]
fn ChoiceField(
    label: &'static str,
    draft: Draft,
    options: &'static [&'static str],
    get: fn(&UiOverride) -> Text,
    set: fn(&mut UiOverride, Text),
) -> impl IntoView {
    let unknown = move || draft.with(get).filter(|v| !options.contains(&v.as_str()));
    view! {
        <label class="override__field">
            <span class="override__label">{label}</span>
            <select
                class="select"
                on:change=move |ev| {
                    let value = non_blank(event_target_value(&ev));
                    draft.update(|d| set(d, value));
                }
            >
                <option value="" prop:selected=move || draft.with(get).is_none()>"Default"</option>
                {options
                    .iter()
                    .map(|o| {
                        view! {
                            <option value=*o prop:selected=move || draft.with(get).as_deref() == Some(*o)>
                                {*o}
                            </option>
                        }
                    })
                    .collect_view()}
                {move || {
                    unknown()
                        .map(|v| view! { <option value=v.clone() selected>{v.clone()}</option> })
                }}
            </select>
        </label>
    }
}

/// The area: a `<select>` of the hub's areas, or free text through "Other...".
#[component]
fn AreaField(draft: Draft) -> impl IntoView {
    let house = expect_context::<House>();
    let known = move |value: &str| house.areas.with(|a| a.iter().any(|a| a.id == value));
    // Free text mode starts on when the stored area is not one of the hub's.
    let other =
        RwSignal::new(draft.with_untracked(|d| d.area.as_deref().is_some_and(|a| !known(a))));
    let selected = move || {
        if other.get() {
            OTHER.to_owned()
        } else {
            draft.with(|d| d.area.clone().unwrap_or_default())
        }
    };
    const OTHER: &str = "__other__";
    view! {
        <div class="override__field">
            <label class="override__label" for="override-area">"Area"</label>
            <select
                id="override-area"
                class="select"
                on:change=move |ev| {
                    let value = event_target_value(&ev);
                    if value == OTHER {
                        other.set(true);
                    } else {
                        other.set(false);
                        let area = non_blank(value);
                        draft.update(|d| d.area = area);
                    }
                }
            >
                <option value="" prop:selected=move || selected().is_empty()>"Default"</option>
                <For
                    each=move || house.areas.get()
                    key=|a| (a.id.clone(), a.name.clone())
                    children=move |area| {
                        let id = area.id.clone();
                        view! {
                            <option value=id.clone() prop:selected=move || selected() == id>
                                {area.name}
                            </option>
                        }
                    }
                />
                <option value=OTHER prop:selected=move || other.get()>"Other…"</option>
            </select>
            <Show when=move || other.get()>
                <input
                    class="field"
                    type="text"
                    aria-label="Area name"
                    prop:value=move || draft.with(|d| d.area.clone().unwrap_or_default())
                    on:input=move |ev| {
                        let area = non_blank(event_target_value(&ev));
                        draft.update(|d| d.area = area);
                    }
                />
            </Show>
        </div>
    }
}

/// The light-on brightness slider (1..=255, the hub rejects 0) with a reset to "Default".
#[component]
fn BrightnessField(draft: Draft) -> impl IntoView {
    view! {
        <div class="override__field">
            <span class="override__label">"Light-on brightness"</span>
            <div class="override__slider">
                <input
                    class="slider"
                    type="range"
                    min="1"
                    max="255"
                    aria-label="Light-on brightness"
                    prop:value=move || {
                        draft.with(|d| d.light_on_brightness.unwrap_or(255)).to_string()
                    }
                    on:input=move |ev| {
                        if let Ok(n) = event_target_value(&ev).parse::<i32>() {
                            draft.update(|d| d.light_on_brightness = Some(n));
                        }
                    }
                />
                <span class="override__value">
                    {move || {
                        draft
                            .with(|d| d.light_on_brightness)
                            .map_or_else(|| "Default".to_owned(), |n| n.to_string())
                    }}
                </span>
                <button
                    type="button"
                    class="btn btn--compact override__reset"
                    disabled=move || draft.with(|d| d.light_on_brightness.is_none())
                    on:click=move |_| draft.update(|d| d.light_on_brightness = None)
                >
                    "Reset"
                </button>
            </div>
        </div>
    }
}

#[component]
fn Group(title: &'static str, children: Children) -> impl IntoView {
    view! {
        <fieldset class="override__group">
            <legend class="override__legend">{title}</legend>
            {children()}
        </fieldset>
    }
}

/// The form for `entity`'s override. The draft starts as `entity.ui`; "Save" sends only what
/// changed, a rejected save shows the hub's message and keeps the draft.
#[component]
pub fn OverrideEditor(entity: EntityView) -> impl IntoView {
    let house = expect_context::<House>();
    let toasts = expect_context::<Toasts>();
    let id = entity.id.clone();
    let is_switch = entity.protocol_kind == Kind::Switch;
    // The light-on options only mean something for lights, and for switches that can be shown
    // as one.
    let has_light = is_switch || entity.protocol_kind == Kind::Light;

    // What the hub currently has; follows changes from outside (the app, MCP).
    let stored = {
        let id = id.clone();
        Memo::new(move |_| {
            house
                .entities
                .with(|m| m.get(&id).map(|e| e.ui.clone()))
                .unwrap_or_default()
        })
    };
    let draft: Draft = RwSignal::new(entity.ui);
    let patch = Memo::new(move |_| stored.with(|s| draft.with(|d| dirty_patch(s, d))));
    let dirty = Memo::new(move |_| patch.with(|p| !p.is_empty()));
    let error = RwSignal::new(None::<String>);
    let busy = RwSignal::new(false);

    // Take over an outside change only while the form has no unsaved edits.
    Effect::new(move |previous: Option<UiOverride>| {
        let now = stored.get();
        if let Some(previous) = previous
            && now != previous
            && draft.with_untracked(|d| dirty_patch(&previous, d).is_empty())
        {
            draft.set(now.clone());
        }
        now
    });

    let save = {
        let id = id.clone();
        move |_| {
            let patch = patch.get_untracked();
            if patch.is_empty() || busy.get_untracked() {
                return;
            }
            busy.set(true);
            error.set(None);
            let id = id.clone();
            spawn_local(async move {
                match set_override(&id, patch).await {
                    Ok(view) => {
                        draft.set(view.ui.clone());
                        house.entities.update(|m| {
                            m.insert(view.id.clone(), view);
                        });
                        toasts.push("Saved");
                    }
                    Err(e) => error.set(Some(e.error)),
                }
                busy.set(false);
            });
        }
    };

    let clear = {
        let id = id.clone();
        move |_| {
            if busy.get_untracked() {
                return;
            }
            busy.set(true);
            error.set(None);
            let id = id.clone();
            spawn_local(async move {
                match clear_override(&id).await {
                    Ok(()) => {
                        draft.set(UiOverride::default());
                        house.entities.update(|m| {
                            if let Some(e) = m.get_mut(&id) {
                                e.ui = UiOverride::default();
                            }
                        });
                        toasts.push("Override cleared");
                    }
                    Err(e) => error.set(Some(e.error)),
                }
                busy.set(false);
            });
        }
    };

    view! {
        <form class="override" on:submit=|ev| ev.prevent_default()>
            <Group title="Display">
                <TextField
                    label="Name"
                    draft=draft
                    get=|d| d.name.clone()
                    set=|d, v| d.name = v
                />
                <AreaField draft=draft />
                <FlagField label="Hidden" draft=draft get=|d| d.hidden set=|d, v| d.hidden = v />
                <FlagField
                    label="Favorite"
                    draft=draft
                    get=|d| d.favorite
                    set=|d, v| d.favorite = v
                />
                <IntField
                    label="Order"
                    draft=draft
                    get=|d| d.order
                    set=|d, v| d.order = v
                />
                <TextField
                    label="Card style"
                    draft=draft
                    get=|d| d.card_style.clone()
                    set=|d, v| d.card_style = v
                />
                <FlagField
                    label="Pinned on Home"
                    draft=draft
                    get=|d| d.home_pinned
                    set=|d, v| d.home_pinned = v
                />
                <FlagField
                    label="Ungrouped"
                    draft=draft
                    get=|d| d.ungrouped
                    set=|d, v| d.ungrouped = v
                />
            </Group>
            {has_light.then(|| view! {
            <Group title="Light">
                {is_switch
                    .then(|| {
                        view! {
                            <FlagField
                                label="Show as a light"
                                draft=draft
                                get=|d| d.as_light
                                set=|d, v| d.as_light = v
                            />
                        }
                    })}
                <ChoiceField
                    label="Light-on preset"
                    draft=draft
                    options=LIGHT_ON_PRESETS
                    get=|d| d.light_on_preset.clone()
                    set=|d, v| d.light_on_preset = v
                />
                <BrightnessField draft=draft />
            </Group>
            })}
            <Group title="Energy">
                <ChoiceField
                    label="Energy role"
                    draft=draft
                    options=ENERGY_ROLES
                    get=|d| d.energy_role.clone()
                    set=|d, v| d.energy_role = v
                />
                <FlagField
                    label="Invert"
                    draft=draft
                    get=|d| d.energy_invert
                    set=|d, v| d.energy_invert = v
                />
                <TextField
                    label="Icon"
                    draft=draft
                    get=|d| d.energy_icon.clone()
                    set=|d, v| d.energy_icon = v
                />
                <TextField
                    label="Meter"
                    draft=draft
                    get=|d| d.energy_meter.clone()
                    set=|d, v| d.energy_meter = v
                />
            </Group>
            <Group title="Security">
                <FlagField
                    label="Hidden from security"
                    draft=draft
                    get=|d| d.security_hidden
                    set=|d, v| d.security_hidden = v
                />
                <FlagField
                    label="Security action"
                    draft=draft
                    get=|d| d.security_action
                    set=|d, v| d.security_action = v
                />
                <FlagField
                    label="Ask before acting"
                    draft=draft
                    get=|d| d.security_ask
                    set=|d, v| d.security_ask = v
                />
            </Group>
            <Group title="Climate">
                <TextField
                    label="Presets"
                    draft=draft
                    get=|d| d.climate_presets.clone()
                    set=|d, v| d.climate_presets = v
                />
            </Group>
            <p class="override__error" role="alert" hidden=move || error.with(Option::is_none)>
                {move || error.get().unwrap_or_default()}
            </p>
            <div class="override__actions">
                <button
                    type="button"
                    class="btn override__save"
                    disabled=move || !dirty.get() || busy.get()
                    on:click=save
                >
                    "Save"
                </button>
                <button
                    type="button"
                    class="btn"
                    disabled=move || busy.get()
                    on:click=clear
                >
                    "Clear override"
                </button>
            </div>
        </form>
    }
}

#[cfg(test)]
mod tests {
    use super::dirty_patch;
    use oikos_core::model::UiOverride;
    use serde_json::json;

    #[test]
    fn unchanged_fields_are_absent() {
        let before = UiOverride::default();
        let after = UiOverride {
            favorite: true,
            ..UiOverride::default()
        };
        let patch = dirty_patch(&before, &after);
        assert_eq!(
            patch,
            json!({"favorite": true}).as_object().cloned().unwrap()
        );
    }

    #[test]
    fn cleared_text_sends_null() {
        let before = UiOverride {
            name: Some("foo".into()),
            ..UiOverride::default()
        };
        let patch = dirty_patch(&before, &UiOverride::default());
        assert_eq!(patch, json!({"name": null}).as_object().cloned().unwrap());
    }

    #[test]
    fn brightness_and_order_are_numbers() {
        let after = UiOverride {
            light_on_brightness: Some(200),
            order: Some(7),
            ..UiOverride::default()
        };
        let patch = dirty_patch(&UiOverride::default(), &after);
        assert_eq!(
            patch,
            json!({"lightOnBrightness": 200, "order": 7})
                .as_object()
                .cloned()
                .unwrap()
        );
    }

    #[test]
    fn every_known_field_is_compared() {
        let after = UiOverride {
            name: Some("n".into()),
            area: Some("a".into()),
            hidden: true,
            order: Some(1),
            card_style: Some("c".into()),
            favorite: true,
            energy_role: Some("solar".into()),
            energy_invert: true,
            security_hidden: true,
            energy_icon: Some("i".into()),
            energy_meter: Some("m".into()),
            home_pinned: true,
            security_action: true,
            security_ask: false,
            ungrouped: true,
            as_light: true,
            light_on_preset: Some("warm".into()),
            light_on_brightness: Some(9),
            climate_presets: Some("p".into()),
            ..UiOverride::default()
        };
        let patch = dirty_patch(&UiOverride::default(), &after);
        let mut keys: Vec<&str> = patch.keys().map(String::as_str).collect();
        let mut known = UiOverride::FIELDS.to_vec();
        keys.sort_unstable();
        known.sort_unstable();
        assert_eq!(keys, known);
        assert!(dirty_patch(&after, &after).is_empty());
    }
}
