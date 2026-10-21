//! The settings form of the device sheet: only the controls that apply to the entity (see
//! [`crate::settings_rules`]), saved as a partial patch of the hub's `UiOverride`.

use leptos::prelude::*;
use leptos::task::spawn_local;
use oikos_api::{EntityView, OverridePatch};
use oikos_core::energy::{ConsumerIcon, EnergyMeters};
use oikos_core::model::{ENERGY_ROLES, Kind, LIGHT_ON_PRESETS, SwitchType, UiOverride};
use serde_json::json;

use crate::api::{clear_override, set_override};
use crate::components::toast::Toasts;
use crate::energy::energy_entities;
use crate::house::House;
use crate::icons::consumer_icon;
use crate::settings_rules::{Shown, clear_field, order_valid, other_saved, set_type, shown};

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
        show_as => "showAs",
        light_on_preset => "lightOnPreset",
        light_on_brightness => "lightOnBrightness",
        climate_presets => "climatePresets",
        main_control => "mainControl",
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
    #[prop(optional)] hint: Option<&'static str>,
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
            {hint.map(|h| view! { <span class="override__hint">{h}</span> })}
        </label>
    }
}

/// The position (`order`): free text so a typo can be shown and blocks Save instead of being
/// dropped. `valid` is false while the text is not empty or a whole number from 0 to 9999.
#[component]
fn PositionField(draft: Draft, valid: RwSignal<bool>) -> impl IntoView {
    let text =
        RwSignal::new(draft.with_untracked(|d| d.order.map(|n| n.to_string()).unwrap_or_default()));
    // Follow an outside change (the draft taking over a newer stored override).
    Effect::new(move |_| {
        let order = draft.with(|d| d.order);
        let shown = text.with_untracked(|t| t.trim().parse::<i32>().ok());
        if shown != order && valid.get_untracked() {
            text.set(order.map(|n| n.to_string()).unwrap_or_default());
        }
    });
    view! {
        <label class="override__field">
            <span class="override__label">"Position"</span>
            <input
                class="field"
                type="text"
                inputmode="numeric"
                prop:value=move || text.get()
                on:input=move |ev| {
                    let raw = event_target_value(&ev);
                    let ok = order_valid(&raw);
                    valid.set(ok);
                    if ok {
                        let order = raw.trim().parse::<i32>().ok();
                        draft.update(|d| d.order = order);
                    }
                    text.set(raw);
                }
            />
            <span class="override__hint" class:override__hint--error=move || !valid.get()>
                {move || {
                    if valid.get() {
                        "Lower numbers come first. Leave empty for automatic."
                    } else {
                        "Enter a whole number from 0 to 9999."
                    }
                }}
            </span>
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

/// One chip of a [`ChipField`]: its text, the value it stands for (`None`: the field's default)
/// and an optional glyph shown before the text.
#[derive(Clone, Copy)]
struct ChipOption {
    label: &'static str,
    value: Option<&'static str>,
    glyph: Option<&'static str>,
}

const fn chip(label: &'static str, value: Option<&'static str>) -> ChipOption {
    ChipOption {
        label,
        value,
        glyph: None,
    }
}

/// A row of single-choice chips. A stored value outside `options` (written by a newer app or by
/// hand) shows as one more selected chip with its raw text, so it is never silently replaced.
#[component]
fn ChipField(
    label: &'static str,
    draft: Draft,
    options: Vec<ChipOption>,
    get: fn(&UiOverride) -> Text,
    on_pick: fn(&mut UiOverride, Option<&'static str>),
) -> impl IntoView {
    let known: Vec<Option<&'static str>> = options.iter().map(|o| o.value).collect();
    let unknown = move || {
        draft
            .with(get)
            .filter(|v| !known.contains(&Some(v.as_str())))
    };
    view! {
        <div class="override__field" role="group" aria-label=label>
            <span class="override__label">{label}</span>
            <div class="override__chips">
                {options
                    .into_iter()
                    .map(|o| {
                        let selected = move || draft.with(get).as_deref() == o.value;
                        view! {
                            <button
                                type="button"
                                class="chip"
                                class:chip--selected=selected
                                aria-pressed=move || selected().to_string()
                                on:click=move |_| draft.update(|d| on_pick(d, o.value))
                            >
                                {o.glyph.map(|g| view! { <span class="chip__glyph" inner_html=g></span> })}
                                {o.label}
                            </button>
                        }
                    })
                    .collect_view()}
                {move || {
                    unknown()
                        .map(|v| {
                            view! {
                                <button type="button" class="chip chip--selected" aria-pressed="true">
                                    {v}
                                </button>
                            }
                        })
                }}
            </div>
        </div>
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
            <label class="override__label" for="override-area">"Room"</label>
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
                    aria-label="Room name"
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

/// The energy counter shown on a power sensor's card: "Automatic" or one of the house's
/// counters. A stored id that no longer names a counter stays selected under its raw id.
#[component]
fn CounterField(draft: Draft) -> impl IntoView {
    let house = expect_context::<House>();
    let counters = Memo::new(move |_| {
        let all = house.entities.with(energy_entities);
        let mut list: Vec<(String, String)> = EnergyMeters::counters(&all)
            .into_iter()
            .map(|e| (e.id.clone(), e.name.clone()))
            .collect();
        list.sort_by_cached_key(|(id, name)| (name.to_lowercase(), id.clone()));
        list
    });
    let current = move || draft.with(|d| d.energy_meter.clone());
    let unknown =
        move || current().filter(|id| counters.with(|c| !c.iter().any(|(known, _)| known == id)));
    view! {
        <label class="override__field">
            <span class="override__label">"Energy counter"</span>
            <select
                class="select"
                on:change=move |ev| {
                    let value = non_blank(event_target_value(&ev));
                    draft.update(|d| d.energy_meter = value);
                }
            >
                <option value="" prop:selected=move || current().is_none()>"Automatic"</option>
                <For
                    each=move || counters.get()
                    key=|(id, name)| (id.clone(), name.clone())
                    children=move |(id, name)| {
                        let value = id.clone();
                        view! {
                            <option
                                value=value
                                prop:selected=move || current().as_deref() == Some(id.as_str())
                            >
                                {name}
                            </option>
                        }
                    }
                />
                {move || unknown().map(|v| view! { <option value=v.clone() selected>{v.clone()}</option> })}
            </select>
        </label>
    }
}

/// The light-on brightness slider (1..=255, the hub rejects 0) with a reset to "Default".
#[component]
fn BrightnessField(draft: Draft) -> impl IntoView {
    view! {
        <div class="override__field">
            <span class="override__label">"Brightness"</span>
            <div class="override__slider">
                <input
                    class="slider"
                    type="range"
                    min="1"
                    max="255"
                    aria-label="Brightness when turned on"
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

/// A titled block of the form, left out entirely while it has nothing to show.
#[component]
fn Section(
    title: &'static str,
    #[prop(into)] when: Signal<bool>,
    children: ChildrenFn,
) -> impl IntoView {
    view! {
        <Show when=move || when.get()>
            <fieldset class="override__group">
                <legend class="override__legend">{title}</legend>
                {children()}
            </fieldset>
        </Show>
    }
}

/// Values set for controls the form does not show, each with a button that clears it.
#[component]
fn OtherSaved(draft: Draft, shown: Memo<Shown>) -> impl IntoView {
    let entries = Memo::new(move |_| draft.with(|d| shown.with(|s| other_saved(d, s))));
    view! {
        <Show when=move || entries.with(|e| !e.is_empty())>
            <div class="override__field">
                <span class="override__label">"Other saved settings"</span>
                <ul class="override__saved">
                    <For
                        each=move || entries.get()
                        key=|(key, value)| (*key, value.clone())
                        children=move |(key, value)| {
                            view! {
                                <li class="override__saved-row">
                                    <span>{format!("{key}: {value}")}</span>
                                    <button
                                        type="button"
                                        class="btn btn--compact"
                                        aria-label=format!("Clear {key}")
                                        on:click=move |_| draft.update(|d| clear_field(d, key))
                                    >
                                        "×"
                                    </button>
                                </li>
                            }
                        }
                    />
                </ul>
            </div>
        </Show>
    }
}

fn energy_role_label(role: &str) -> &'static str {
    match role {
        "solar" => "Solar",
        "grid" => "Grid",
        "grid_export" => "Grid export",
        "battery" => "Battery",
        "house" => "House",
        "consumer" => "Consumer",
        "water" => "Water",
        _ => "Unknown",
    }
}

fn light_on_label(preset: &str) -> &'static str {
    match preset {
        "white" => "White",
        "warm" => "Warm",
        "red" => "Red",
        "green" => "Green",
        "blue" => "Blue",
        _ => "Unknown",
    }
}

/// The consumer icons, in the app's order, with their labels.
const CONSUMER_ICONS: [(&str, &str); 7] = [
    ("water_heater", "Water heater"),
    ("heat_pump", "Heat pump"),
    ("oven", "Oven"),
    ("fridge", "Fridge"),
    ("washer", "Washer"),
    ("dryer", "Dryer"),
    ("dishwasher", "Dishwasher"),
];

fn type_options() -> Vec<ChipOption> {
    SwitchType::ALL
        .into_iter()
        .map(|t| chip(t.label(), t.key()))
        .collect()
}

fn light_on_options() -> Vec<ChipOption> {
    std::iter::once(chip("Last state", None))
        .chain(
            LIGHT_ON_PRESETS
                .iter()
                .map(|p| chip(light_on_label(p), Some(*p))),
        )
        .collect()
}

fn energy_role_options() -> Vec<ChipOption> {
    std::iter::once(chip("None", None))
        .chain(
            ENERGY_ROLES
                .iter()
                .map(|r| chip(energy_role_label(r), Some(*r))),
        )
        .collect()
}

fn icon_options() -> Vec<ChipOption> {
    std::iter::once(ChipOption {
        label: "Plug",
        value: None,
        glyph: Some(consumer_icon(None)),
    })
    .chain(CONSUMER_ICONS.iter().map(|(key, label)| ChipOption {
        label,
        value: Some(*key),
        glyph: Some(consumer_icon(ConsumerIcon::parse(key))),
    }))
    .collect()
}

#[component]
pub fn OverrideEditor(entity: EntityView) -> impl IntoView {
    let house = expect_context::<House>();
    let toasts = expect_context::<Toasts>();
    let id = entity.id.clone();

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
    let draft: Draft = RwSignal::new(entity.ui.clone());
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

    let rules_entity = entity.clone();
    let shown = Memo::new(move |_| draft.with(|d| shown(&rules_entity, d)));
    let order_ok = RwSignal::new(true);
    let group_label = if entity.protocol_kind == Kind::Cover {
        "Group with the other shutters in this room"
    } else {
        "Group with the other lights in this room"
    };
    let on = move |pick: fn(&Shown) -> bool| Signal::derive(move || shown.with(pick));

    view! {
        <form class="override" on:submit=|ev| ev.prevent_default()>
            <fieldset class="override__group">
                <legend class="override__legend">"General"</legend>
                <TextField
                    label="Name"
                    draft=draft
                    get=|d| d.name.clone()
                    set=|d, v| d.name = v
                    hint="Leave empty to use the device's own name."
                />
                <AreaField draft=draft />
                <FlagField label="Favorite" draft=draft get=|d| d.favorite set=|d, v| d.favorite = v />
                <PositionField draft=draft valid=order_ok />
                <Show
                    when=move || shown.with(|s| s.show_on_home)
                    fallback=move || {
                        view! {
                            <FlagField
                                label="Hide from Home"
                                draft=draft
                                get=|d| d.hidden
                                set=|d, v| d.hidden = v
                            />
                        }
                    }
                >
                    <FlagField
                        label="Show on Home"
                        draft=draft
                        get=|d| d.home_pinned && !d.hidden
                        set=|d, v| {
                            d.home_pinned = v;
                            if v {
                                d.hidden = false;
                            }
                        }
                    />
                </Show>
            </fieldset>
            <Section title="Type" when=on(|s| s.show_as)>
                <ChipField
                    label="Show as"
                    draft=draft
                    options=type_options()
                    get=|d| d.show_as.clone()
                    on_pick=|d, v| {
                        let t = SwitchType::ALL
                            .into_iter()
                            .find(|t| t.key() == v)
                            .unwrap_or(SwitchType::Switch);
                        set_type(d, t);
                    }
                />
            </Section>
            <Section title="Light" when=on(|s| s.group_with_room || s.light_on)>
                <Show when=move || shown.with(|s| s.group_with_room)>
                    <FlagField
                        label=group_label
                        draft=draft
                        get=|d| !d.ungrouped
                        set=|d, v| d.ungrouped = !v
                    />
                </Show>
                <Show when=move || shown.with(|s| s.light_on)>
                    <ChipField
                        label="Turn on in"
                        draft=draft
                        options=light_on_options()
                        get=|d| d.light_on_preset.clone()
                        on_pick=|d, v| {
                            d.light_on_preset = v.map(str::to_owned);
                            if v.is_none() {
                                d.light_on_brightness = None;
                            }
                        }
                    />
                </Show>
                <Show when=move || shown.with(|s| s.light_on_brightness)>
                    <BrightnessField draft=draft />
                </Show>
            </Section>
            <Section title="Security" when=on(|s| s.security_shown || s.security_action)>
                <Show when=move || shown.with(|s| s.security_shown)>
                    <FlagField
                        label="Show on Security"
                        draft=draft
                        get=|d| !d.security_hidden
                        set=|d, v| d.security_hidden = !v
                    />
                </Show>
                <Show when=move || shown.with(|s| s.security_action)>
                    <FlagField
                        label="Show on Security"
                        draft=draft
                        get=|d| d.security_action
                        set=|d, v| d.security_action = v
                    />
                </Show>
                <Show when=move || shown.with(|s| s.security_ask)>
                    <FlagField
                        label="Ask before acting"
                        draft=draft
                        get=|d| d.security_ask
                        set=|d, v| d.security_ask = v
                    />
                </Show>
            </Section>
            <Section title="Display and energy" when=on(|s| s.card_style || s.energy_role)>
                <Show when=move || shown.with(|s| s.card_style)>
                    <ChipField
                        label="Card style"
                        draft=draft
                        options=vec![chip("Standard", None), chip("Header temperature", Some("hero_weather"))]
                        get=|d| d.card_style.clone()
                        on_pick=|d, v| d.card_style = v.map(str::to_owned)
                    />
                </Show>
                <Show when=move || shown.with(|s| s.energy_role)>
                    <ChipField
                        label="Energy role"
                        draft=draft
                        options=energy_role_options()
                        get=|d| d.energy_role.clone()
                        on_pick=|d, v| d.energy_role = v.map(str::to_owned)
                    />
                </Show>
                <Show when=move || shown.with(|s| s.unit_warning)>
                    <p class="override__hint override__hint--error">"Needs a power unit (W, kW)"</p>
                </Show>
                <Show when=move || shown.with(|s| s.energy_invert)>
                    <FlagField
                        label="Invert sign"
                        draft=draft
                        get=|d| d.energy_invert
                        set=|d, v| d.energy_invert = v
                    />
                </Show>
                <Show when=move || shown.with(|s| s.energy_icon)>
                    <ChipField
                        label="Icon"
                        draft=draft
                        options=icon_options()
                        get=|d| d.energy_icon.clone()
                        on_pick=|d, v| d.energy_icon = v.map(str::to_owned)
                    />
                </Show>
                <Show when=move || shown.with(|s| s.energy_meter)>
                    <CounterField draft=draft />
                </Show>
            </Section>
            <Section title="Climate" when=on(|s| s.climate_presets)>
                <TextField
                    label="Target presets (°C)"
                    draft=draft
                    get=|d| d.climate_presets.clone()
                    set=|d, v| d.climate_presets = v
                    hint="Comma-separated, e.g. 19, 21, 23. Up to 6 chips."
                />
            </Section>
            <OtherSaved draft=draft shown=shown />
            <p class="override__error" role="alert" hidden=move || error.with(Option::is_none)>
                {move || error.get().unwrap_or_default()}
            </p>
            <div class="override__actions">
                <button
                    type="button"
                    class="btn override__save"
                    disabled=move || !dirty.get() || busy.get() || !order_ok.get()
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
                    "Reset to defaults"
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
            show_as: Some("door".into()),
            light_on_preset: Some("warm".into()),
            light_on_brightness: Some(9),
            climate_presets: Some("p".into()),
            main_control: Some("locked".into()),
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
