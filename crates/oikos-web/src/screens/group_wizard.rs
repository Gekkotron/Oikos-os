//! The three-step group wizard: type, members, then name, room and the hide-members switch.
//! `/groups/new` creates a group; `/groups/:id/edit` reopens it with the type locked.

use std::collections::{BTreeMap, BTreeSet};

use leptos::prelude::*;
use leptos_router::hooks::{use_navigate, use_params_map};
use oikos_api::{AreaView, EntityView, GroupSpec};
use oikos_core::model::Kind;

use crate::api::{encode_id, list_groups, set_group, set_override};
use crate::cards::PowerToggle;
use crate::components::toast::Toasts;
use crate::flow_edit::fold_accents;
use crate::format::list_label;
use crate::house::House;
use crate::icons::{CHECK, MAGNIFIER};
use crate::nav::{BackLink, use_back};

/// The kinds a group can be made of: every controllable kind.
pub const GROUP_KINDS: &[Kind] = &[
    Kind::Light,
    Kind::Switch,
    Kind::Cover,
    Kind::Climate,
    Kind::Number,
    Kind::Select,
    Kind::Button,
    Kind::Vacuum,
    Kind::WaterHeater,
];

/// "water heater" for `Kind::WaterHeater`.
pub fn kind_word(kind: Kind) -> String {
    kind.component().replace('_', " ")
}

/// A group id from its name; see [`crate::ambiances::slug_id`] for the rules.
pub fn slug_group_id(name: &str, taken: &BTreeSet<String>) -> String {
    crate::ambiances::slug_id(name, taken, "group")
}

/// The entities of `kind` (their effective kind, so `showAs` counts) matching `query`, grouped by
/// room in the hub's room order, unknown rooms last. Entities that are not shown by default, or
/// are groups themselves, appear only when already `selected`.
pub fn members_by_room(
    entities: &BTreeMap<String, EntityView>,
    areas: &[AreaView],
    kind: Kind,
    query: &str,
    selected: &BTreeSet<String>,
) -> Vec<(String, Vec<(String, String)>)> {
    let needle = fold_accents(query.trim());
    let items = entities
        .values()
        .filter(|e| e.kind == kind)
        .filter_map(|e| {
            let shown = (e.is_primary() && !e.id.starts_with("group_")) || selected.contains(&e.id);
            let label = list_label(e);
            (shown && (needle.is_empty() || fold_accents(&label).contains(&needle)))
                .then(|| (e.id.clone(), label, e.area.clone()))
        });
    group_by_room(items, areas)
}

/// `(id, label, area id)` items grouped by room in the hub's room order, each room sorted by
/// label (accents folded), unknown rooms last and unassigned ones under "Unassigned".
pub fn group_by_room(
    items: impl IntoIterator<Item = (String, String, String)>,
    areas: &[AreaView],
) -> Vec<(String, Vec<(String, String)>)> {
    let mut by_area: BTreeMap<String, Vec<(String, String)>> = BTreeMap::new();
    for (id, label, area) in items {
        by_area.entry(area).or_default().push((id, label));
    }
    let mut rooms = Vec::new();
    for area in areas {
        if let Some(mut list) = by_area.remove(&area.id) {
            list.sort_by_key(|(_, label)| fold_accents(label));
            rooms.push((area.name.clone(), list));
        }
    }
    for (area, mut list) in by_area {
        list.sort_by_key(|(_, label)| fold_accents(label));
        let name = if area.is_empty() {
            "Unassigned".to_owned()
        } else {
            area
        };
        rooms.push((name, list));
    }
    rooms
}

#[component]
pub fn GroupWizardScreen() -> impl IntoView {
    let house = expect_context::<House>();
    let toasts = expect_context::<Toasts>();
    let navigate = use_navigate();
    let back = use_back();
    let params = use_params_map();
    let edit_id: Option<String> = params.read_untracked().get("id").filter(|s| !s.is_empty());
    let editing = edit_id.is_some();

    let step = RwSignal::new(if editing { 2u8 } else { 1u8 });
    let kind = RwSignal::new(None::<Kind>);
    let selected = RwSignal::new(BTreeSet::<String>::new());
    let filter = RwSignal::new(String::new());
    let name = RwSignal::new(String::new());
    let area = RwSignal::new(String::new());
    let hide_members = RwSignal::new(true);
    let loading = RwSignal::new(editing);
    let saving = RwSignal::new(false);

    if let Some(id) = edit_id.clone() {
        let navigate = navigate.clone();
        leptos::task::spawn_local(async move {
            match list_groups().await {
                Ok(groups) => match groups.into_iter().find(|g| g.id == id) {
                    Some(group) => {
                        kind.set(Some(group.kind));
                        selected.set(group.members.iter().map(|m| m.id.clone()).collect());
                        name.set(group.name);
                        area.set(group.entity.area);
                        hide_members.set(group.hide_members);
                        loading.set(false);
                    }
                    None => {
                        toasts.push("Group not found");
                        navigate(&back.get_untracked(), Default::default());
                    }
                },
                Err(e) => toasts.push(e.error),
            }
        });
    }

    let rooms = Memo::new(move |_| {
        let Some(kind) = kind.get() else {
            return Vec::new();
        };
        let query = filter.get();
        let selected = selected.get();
        house.entities.with(|entities| {
            house
                .areas
                .with(|areas| members_by_room(entities, areas, kind, &query, &selected))
        })
    });

    let can_next = move || match step.get() {
        1 => kind.get().is_some(),
        2 => !selected.with(BTreeSet::is_empty),
        _ => !name.with(|n| n.trim().is_empty()) && !saving.get(),
    };

    let submit = StoredValue::new_local({
        let edit_id = edit_id.clone();
        move || {
            let Some(kind) = kind.get_untracked() else {
                return;
            };
            let spec = GroupSpec {
                kind,
                name: name.get_untracked().trim().to_owned(),
                members: selected.get_untracked().into_iter().collect(),
                hide_members: hide_members.get_untracked(),
            };
            let room = area.get_untracked();
            let edit_id = edit_id.clone();
            let navigate = navigate.clone();
            saving.set(true);
            leptos::task::spawn_local(async move {
                let id = match edit_id {
                    Some(id) => id,
                    None => match list_groups().await {
                        Ok(groups) => {
                            let taken = groups.into_iter().map(|g| g.id).collect();
                            slug_group_id(&spec.name, &taken)
                        }
                        Err(e) => {
                            toasts.push(e.error);
                            saving.set(false);
                            return;
                        }
                    },
                };
                match set_group(&id, &spec).await {
                    Ok(group) => {
                        if !room.is_empty() && room != group.entity.area {
                            let mut patch = oikos_api::OverridePatch::new();
                            patch.insert("area".to_owned(), room.into());
                            if let Err(e) = set_override(&group.entity.id, patch).await {
                                toasts.push(e.error);
                            }
                        }
                        navigate(&format!("/group/{}", encode_id(&id)), Default::default());
                    }
                    Err(e) => toasts.push(e.error),
                }
                saving.set(false);
            });
        }
    });

    let area_options = move || {
        house
            .areas
            .get()
            .into_iter()
            .map(|a| view! { <option value=a.id.clone()>{a.name}</option> })
            .collect_view()
    };

    view! {
        <div class="devices">
            <BackLink />
            <h1 class="devices__title">
                {if editing { "Edit group" } else { "New group" }}
            </h1>
            <Show when=move || !loading.get() fallback=|| view! { <p>"Loading…"</p> }>
                <section class="sheet__panel wizard">
                    <div class="wizard__head">
                        <h2 class="sheet__heading">
                            {move || match step.get() {
                                1 => "Type",
                                2 => "Members",
                                _ => "Name and room",
                            }}
                        </h2>
                        <div class="wizard__step" aria-label="Progress">
                            <span class="wizard__step-count">
                                {move || format!("Step {} of 3", step.get())}
                            </span>
                            <span class="wizard__pips" aria-hidden="true">
                                <span class:lit=move || { step.get() >= 1 }></span>
                                <span class:lit=move || { step.get() >= 2 }></span>
                                <span class:lit=move || { step.get() >= 3 }></span>
                            </span>
                        </div>
                    </div>
                    {move || match step.get() {
                        1 => {
                            view! {
                                <div class="card__controls card__controls--grid">
                                    {GROUP_KINDS
                                        .iter()
                                        .map(|&k| {
                                            view! {
                                                <button
                                                    type="button"
                                                    class="card"
                                                    class:card--on=move || kind.get() == Some(k)
                                                    aria-pressed=move || (kind.get() == Some(k)).to_string()
                                                    disabled=editing
                                                    on:click=move |_| {
                                                        if kind.get_untracked() != Some(k) {
                                                            selected.set(BTreeSet::new());
                                                        }
                                                        kind.set(Some(k));
                                                    }
                                                >
                                                    <span class="card__name">{kind_word(k)}</span>
                                                </button>
                                            }
                                        })
                                        .collect_view()}
                                </div>
                            }
                                .into_any()
                        }
                        2 => {
                            view! {
                                <label class="devices__search">
                                    <span class="devices__search-icon" inner_html=MAGNIFIER></span>
                                    <input
                                        class="devices__search-input"
                                        type="search"
                                        placeholder="Filter members"
                                        aria-label="Filter members"
                                        prop:value=move || filter.get()
                                        on:input=move |ev| filter.set(event_target_value(&ev))
                                    />
                                </label>
                                {move || {
                                    rooms
                                        .get()
                                        .into_iter()
                                        .map(|(room, list)| {
                                            view! {
                                                <h3 class="sheet__subheading">{room}</h3>
                                                <div class="card__controls card__controls--grid wizard__members">
                                                    {list
                                                        .into_iter()
                                                        .map(|(id, label)| {
                                                            let on = {
                                                                let id = id.clone();
                                                                move || selected.with(|s| s.contains(&id))
                                                            };
                                                            let pressed = on.clone();
                                                            view! {
                                                                <button
                                                                    type="button"
                                                                    class="card member-card"
                                                                    class:card--on=on
                                                                    aria-pressed=move || pressed().to_string()
                                                                    on:click=move |_| {
                                                                        selected
                                                                            .update(|s| {
                                                                                if !s.remove(&id) {
                                                                                    s.insert(id.clone());
                                                                                }
                                                                            })
                                                                    }
                                                                >
                                                                    <span class="card__name">{label}</span>
                                                                    <span class="member-card__check" aria-hidden="true" inner_html=CHECK></span>
                                                                </button>
                                                            }
                                                        })
                                                        .collect_view()}
                                                </div>
                                            }
                                        })
                                        .collect_view()
                                }}
                            }
                                .into_any()
                        }
                        _ => {
                            view! {
                                <label class="field">
                                    <span class="field__caption">"Name"</span>
                                    <input
                                        class="field__input"
                                        type="text"
                                        aria-label="Group name"
                                        prop:value=move || name.get()
                                        on:input=move |ev| name.set(event_target_value(&ev))
                                    />
                                </label>
                                <label class="field">
                                    <span class="field__caption">"Room"</span>
                                    <select
                                        class="field__input field__select"
                                        prop:value=move || area.get()
                                        on:change=move |ev| area.set(event_target_value(&ev))
                                    >
                                        <option value="">"Unassigned"</option>
                                        {area_options()}
                                    </select>
                                </label>
                                <PowerToggle
                                    on=hide_members.get()
                                    label="Hide members on Home"
                                    on_toggle=Callback::new(move |()| hide_members.update(|v| *v = !*v))
                                />
                            }
                                .into_any()
                        }
                    }}
                    <div class="sheet__actions">
                        <button
                            type="button"
                            class="btn btn--with-icon"
                            disabled=move || step.get() == if editing { 2 } else { 1 }
                            on:click=move |_| step.update(|s| *s = s.saturating_sub(1).max(1))
                        >
                            "Back"
                        </button>
                        <button
                            type="button"
                            class="btn btn--with-icon btn--primary"
                            disabled=move || !can_next()
                            on:click=move |_| {
                                if step.get_untracked() < 3 {
                                    step.update(|s| *s += 1);
                                } else {
                                    submit.with_value(|f| f());
                                }
                            }
                        >
                            {move || {
                                if step.get() < 3 {
                                    "Next"
                                } else if editing {
                                    "Save group"
                                } else {
                                    "Create group"
                                }
                            }}
                        </button>
                    </div>
                </section>
            </Show>
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oikos_core::model::{Capabilities, EntityCategory, EntityState, UiOverride};

    fn taken(ids: &[&str]) -> BTreeSet<String> {
        ids.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn slug_folds_accents_and_replaces_separators() {
        let none = BTreeSet::new();
        assert_eq!(
            slug_group_id("Lumières Salon-Est.1", &none),
            "lumieres_salon_est_1"
        );
        assert_eq!(slug_group_id("A  b!?", &none), "a__b");
        assert_eq!(slug_group_id("!!!", &none), "group");
    }

    #[test]
    fn slug_truncates_to_48() {
        let long = "x".repeat(80);
        assert_eq!(slug_group_id(&long, &BTreeSet::new()).len(), 48);
    }

    #[test]
    fn slug_suffixes_on_collision_within_the_limit() {
        assert_eq!(slug_group_id("Salon", &taken(&["salon"])), "salon_2");
        assert_eq!(
            slug_group_id("Salon", &taken(&["salon", "salon_2"])),
            "salon_3"
        );
        let long = "x".repeat(80);
        let full = "x".repeat(48);
        let id = slug_group_id(&long, &taken(&[full.as_str()]));
        assert_eq!(id.len(), 48);
        assert!(id.ends_with("_2"));
    }

    #[test]
    fn group_kinds_are_the_controllable_ones() {
        for k in [
            Kind::Sensor,
            Kind::BinarySensor,
            Kind::AlarmPanel,
            Kind::Generic,
        ] {
            assert!(!GROUP_KINDS.contains(&k));
        }
        assert_eq!(GROUP_KINDS.len(), Kind::ALL.len() - 4);
        assert_eq!(kind_word(Kind::WaterHeater), "water heater");
    }

    fn entity(id: &str, kind: Kind, protocol: Kind, area: &str) -> EntityView {
        EntityView {
            id: id.into(),
            device_id: None,
            name: id.into(),
            device_name: None,
            label: None,
            kind,
            component: kind.component().to_string(),
            protocol_kind: protocol,
            device_class: None,
            unit: None,
            icon: None,
            category: EntityCategory::Primary,
            enabled_by_default: true,
            area: area.into(),
            part_of: None,
            capabilities: Capabilities::default(),
            ui: UiOverride::default(),
            state: EntityState::default(),
        }
    }

    fn area(id: &str, name: &str) -> AreaView {
        AreaView {
            id: id.into(),
            name: name.into(),
            icon: None,
            order: None,
            entity_count: 0,
            primary_count: 0,
            group_names: BTreeMap::new(),
            group_favorites: BTreeSet::new(),
        }
    }

    #[test]
    fn members_filter_by_effective_kind_and_group_by_room() {
        let mut entities = BTreeMap::new();
        for e in [
            entity("lamp", Kind::Light, Kind::Light, "salon"),
            entity("plug", Kind::Light, Kind::Switch, "cuisine"),
            entity("fan", Kind::Switch, Kind::Switch, "salon"),
            entity("lost", Kind::Light, Kind::Light, "ghost"),
        ] {
            entities.insert(e.id.clone(), e);
        }
        let areas = [area("salon", "Salon"), area("cuisine", "Cuisine")];
        let rooms = members_by_room(&entities, &areas, Kind::Light, "", &BTreeSet::new());
        let names: Vec<&str> = rooms.iter().map(|(r, _)| r.as_str()).collect();
        assert_eq!(names, ["Salon", "Cuisine", "ghost"]);
        assert_eq!(rooms[1].1[0].0, "plug");
        let filtered = members_by_room(&entities, &areas, Kind::Light, "LAM", &BTreeSet::new());
        assert_eq!(filtered.len(), 1);
    }
}
