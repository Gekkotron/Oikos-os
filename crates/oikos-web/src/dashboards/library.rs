//! The widget library in the side panel while editing a dashboard: a search box over the
//! widgets that can be added, grouped by kind, each row showing its icon, title, room and
//! device. Clicking a row builds a `Widget` with the smallest allowed span for its kind and
//! calls `on_add`.

use std::collections::BTreeMap;

use leptos::ev;
use leptos::prelude::*;
use oikos_api::{AreaView, EntityView};
use oikos_core::dashboard::{
    DashboardHistoryRange, Span, Widget, WidgetConfig, WidgetKind, allowed_spans,
};
use oikos_core::model::Kind;

use super::energy_stat::ENERGY_METRICS;
use crate::flow_edit::fold_accents;
use crate::format::{entity_title, kind_search_words, room_name};
use crate::house::House;
use crate::i18n::{DashText, Lang, dash_text, widget_text};
use crate::icons::{BOLT, HOUSE, PLAY, entity_icon};

/// A timestamp is unique enough: widgets are added by hand, one click at a time.
#[cfg(target_arch = "wasm32")]
fn new_widget_id() -> String {
    let n = js_sys::Date::now() as u64;
    format!("w_{:x}", n)
}

/// Smallest allowed span keeps a fresh widget from crowding the grid; the user resizes later.
fn make_widget_with_id(id: String, kind: WidgetKind, config: WidgetConfig) -> Widget {
    let span = allowed_spans(kind).first().copied().unwrap_or(Span::X1x1);
    Widget {
        id,
        kind,
        span,
        config,
    }
}

#[cfg(target_arch = "wasm32")]
fn make_widget(kind: WidgetKind, config: WidgetConfig) -> Widget {
    make_widget_with_id(new_widget_id(), kind, config)
}

#[cfg(not(target_arch = "wasm32"))]
fn make_widget(kind: WidgetKind, config: WidgetConfig) -> Widget {
    // js_sys::Date panics off wasm, so native unit tests use a fixed id.
    make_widget_with_id("w_test".into(), kind, config)
}

/// The section of the library a row sits in, in display order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Section {
    Lights,
    Covers,
    Climates,
    Sensors,
    WaterHeaters,
    Rooms,
    FlowButtons,
    Energy,
    HistoryCharts,
}

impl Section {
    fn header(self, lang: Lang) -> &'static str {
        dash_text(
            lang,
            match self {
                Self::Lights => DashText::SectionLights,
                Self::Covers => DashText::SectionCovers,
                Self::Climates => DashText::SectionClimate,
                Self::Sensors => DashText::SectionSensors,
                Self::WaterHeaters => DashText::SectionWaterHeaters,
                Self::Rooms => DashText::SectionRoomGroups,
                Self::FlowButtons => DashText::SectionFlowButtons,
                Self::Energy => DashText::SectionEnergy,
                Self::HistoryCharts => DashText::SectionHistory,
            },
        )
    }

    /// The suffix of the section's colour modifier classes.
    fn suffix(self) -> &'static str {
        match self {
            Self::Lights => "lights",
            Self::Covers => "covers",
            Self::Climates => "climates",
            Self::Sensors => "sensors",
            Self::WaterHeaters => "water-heaters",
            Self::Rooms => "rooms",
            Self::FlowButtons => "flows",
            Self::Energy => "energy",
            Self::HistoryCharts => "history",
        }
    }
}

/// One widget the library can add.
#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub section: Section,
    pub title: String,
    /// The row's second line: an entity's room and device, or what the row does.
    pub sub: Option<String>,
    pub icon: &'static str,
    pub kind: WidgetKind,
    pub config: WidgetConfig,
}

/// `"{room} · {device}"`, skipping a missing part and a device that only repeats the title.
fn subline(entity: &EntityView, areas: &[AreaView], title: &str) -> Option<String> {
    let room = room_name(areas, &entity.area);
    let device = entity
        .device_name
        .clone()
        .filter(|d| !d.eq_ignore_ascii_case(title));
    let parts: Vec<String> = [room, device]
        .into_iter()
        .flatten()
        .filter(|s| !s.is_empty())
        .collect();
    (!parts.is_empty()).then(|| parts.join(" · "))
}

/// What `query` offers to put on a dashboard, ordered by section, then title. A row matches
/// when its title or second line contains the query, ignoring case and accents.
pub fn entries(
    lang: Lang,
    query: &str,
    entities: &BTreeMap<String, EntityView>,
    areas: &[AreaView],
    flows: &[(String, String)],
) -> Vec<Entry> {
    // Each row with the kind words that also find it ("cover", "volet"), for entity rows.
    let mut rows: Vec<(String, Entry, Option<[String; 2]>)> = Vec::new();
    for e in entities.values().filter(|e| e.is_primary()) {
        let (section, kind) = match e.kind {
            Kind::Light => (Section::Lights, WidgetKind::Light),
            Kind::Cover => (Section::Covers, WidgetKind::Cover),
            Kind::Climate => (Section::Climates, WidgetKind::Climate),
            Kind::Sensor => (Section::Sensors, WidgetKind::Sensor),
            Kind::WaterHeater => (Section::WaterHeaters, WidgetKind::WaterHeater),
            _ => continue,
        };
        let title = entity_title(lang, e);
        let sub = subline(e, areas, &title);
        let icon = entity_icon(e);
        let words = kind_search_words(e);
        let entity_id = e.id.clone();
        let config = WidgetConfig::Entity {
            entity_id: entity_id.clone(),
            title: None,
        };
        rows.push((
            entity_id.clone(),
            Entry {
                section,
                title: title.clone(),
                sub: sub.clone(),
                icon,
                kind,
                config,
            },
            Some(words.clone()),
        ));
        if e.kind == Kind::Sensor {
            rows.push((
                entity_id.clone(),
                Entry {
                    section: Section::HistoryCharts,
                    title,
                    sub,
                    icon,
                    kind: WidgetKind::HistoryChart,
                    config: WidgetConfig::History {
                        entity_id,
                        range: DashboardHistoryRange::H24,
                    },
                },
                Some(words),
            ));
        }
    }
    for area in areas.iter().filter(|a| room_name(areas, &a.id).is_some()) {
        rows.push((
            area.id.clone(),
            Entry {
                section: Section::Rooms,
                title: area.name.clone(),
                sub: Some(dash_text(lang, DashText::RoomGroupSub).into()),
                icon: HOUSE,
                kind: WidgetKind::RoomGroup,
                config: WidgetConfig::Area {
                    area_id: area.id.clone(),
                    title: None,
                },
            },
            None,
        ));
    }
    for (id, name) in flows {
        rows.push((
            id.clone(),
            Entry {
                section: Section::FlowButtons,
                title: name.clone(),
                sub: Some(dash_text(lang, DashText::FlowButtonSub).into()),
                icon: PLAY,
                kind: WidgetKind::FlowButton,
                config: WidgetConfig::Flow {
                    flow_id: id.clone(),
                    title: None,
                },
            },
            None,
        ));
    }
    for (id, label) in ENERGY_METRICS {
        rows.push((
            id.to_owned(),
            Entry {
                section: Section::Energy,
                title: widget_text(lang, label).to_owned(),
                sub: None,
                icon: BOLT,
                kind: WidgetKind::EnergyStat,
                config: WidgetConfig::Energy {
                    metric: id.to_owned(),
                    title: None,
                },
            },
            None,
        ));
    }

    let needle = fold_accents(query.trim());
    rows.retain(|(_, e, words)| {
        needle.is_empty()
            || fold_accents(&e.title).contains(&needle)
            || words
                .iter()
                .flatten()
                .any(|w| fold_accents(w).contains(&needle))
            || e.sub
                .as_deref()
                .is_some_and(|s| fold_accents(s).contains(&needle))
    });
    rows.sort_by_cached_key(|(id, e, _)| (e.section, e.title.to_lowercase(), id.clone()));
    rows.into_iter().map(|(_, e, _)| e).collect()
}

/// The library. Clicking a row, or Enter on the highlighted one while the search box has the
/// focus, calls `on_add`; the library stays as it is so several widgets can go in a row. Esc
/// in the search box clears it.
#[component]
pub fn WidgetLibrary(on_add: Callback<Widget>) -> impl IntoView {
    let house = expect_context::<House>();
    let lang = crate::i18n::use_lang();
    let query = RwSignal::new(String::new());
    let highlight = RwSignal::new(0_usize);

    let results = Memo::new(move |_| {
        let query = query.get();
        let flows: Vec<(String, String)> = house.flows.with(|flows| {
            flows
                .iter()
                .map(|f| (f.definition.id.clone(), f.definition.name.clone()))
                .collect()
        });
        house.entities.with(|entities| {
            house
                .areas
                .with(|areas| entries(lang(), &query, entities, areas, &flows))
        })
    });

    // A new query starts again from the top.
    Effect::watch(move || query.get(), move |_, _, _| highlight.set(0), false);

    let add = move |entry: Entry| on_add.run(make_widget(entry.kind, entry.config));
    let on_key = move |ev: ev::KeyboardEvent| {
        let last = results.with_untracked(Vec::len).saturating_sub(1);
        match ev.key().to_ascii_lowercase().as_str() {
            "arrowdown" => {
                ev.prevent_default();
                highlight.update(|i| *i = (*i + 1).min(last));
            }
            "arrowup" => {
                ev.prevent_default();
                highlight.update(|i| *i = i.saturating_sub(1));
            }
            "enter" => {
                ev.prevent_default();
                if let Some(entry) =
                    results.with_untracked(|r| r.get(highlight.get_untracked()).cloned())
                {
                    add(entry);
                }
            }
            "escape" => query.set(String::new()),
            _ => {}
        }
    };

    let list = move || {
        let mut last = None;
        let lang = lang();
        results
            .get()
            .into_iter()
            .enumerate()
            .map(|(i, entry)| {
                let suffix = entry.section.suffix();
                let head = (last != Some(entry.section)).then(|| {
                    let class = format!("flow-library__header dashboard-library__header--{suffix}");
                    view! { <li class=class role="presentation">{entry.section.header(lang)}</li> }
                });
                last = Some(entry.section);
                let class = format!(
                    "flow-library__item dashboard-library__item dashboard-library__item--{suffix}"
                );
                let title = entry.title.clone();
                let sub = entry.sub.clone();
                let icon = entry.icon;
                view! {
                    {head}
                    <li
                        class=class
                        class:flow-library__item--on=move || highlight.get() == i
                        role="option"
                        aria-selected=move || (highlight.get() == i).to_string()
                        on:pointermove=move |_| highlight.set(i)
                        on:click=move |_| add(entry.clone())
                    >
                        <span class="flow-library__icon" inner_html=icon></span>
                        <span class="flow-library__text">
                            <span class="flow-library__title">{title}</span>
                            {sub.map(|s| view! { <span class="flow-library__sub">{s}</span> })}
                        </span>
                    </li>
                }
            })
            .collect_view()
    };

    view! {
        <section class="flow-library dashboard-library" aria-label=move || dash_text(lang(), DashText::AddWidget)>
            <header class="flow-drawer__header">
                <h2 class="flow-drawer__title">{move || dash_text(lang(), DashText::AddWidget)}</h2>
            </header>
            <input
                class="field flow-library__search"
                type="search"
                placeholder=move || dash_text(lang(), DashText::SearchPlaceholder)
                aria-label=move || dash_text(lang(), DashText::SearchWidgets)
                prop:value=move || query.get()
                on:input=move |ev| query.set(event_target_value(&ev))
                on:keydown=on_key
            />
            <ul class="flow-library__list" role="listbox">
                {list}
                <Show when=move || results.with(Vec::is_empty)>
                    <li class="flow-library__empty">{move || dash_text(lang(), DashText::NothingMatches)}</li>
                </Show>
            </ul>
        </section>
    }
}

#[cfg(test)]
mod tests {
    use oikos_core::model::{Capabilities, EntityCategory, EntityState, UiOverride};

    use super::*;

    fn ent(id: &str, name: &str, kind: Kind) -> EntityView {
        EntityView {
            id: id.into(),
            device_id: None,
            name: name.into(),
            device_name: None,
            label: None,
            kind,
            component: kind.component().to_string(),
            protocol_kind: kind,
            device_class: None,
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

    fn house(list: Vec<EntityView>) -> BTreeMap<String, EntityView> {
        list.into_iter().map(|e| (e.id.clone(), e)).collect()
    }

    fn area(id: &str, name: &str) -> AreaView {
        AreaView {
            id: id.into(),
            name: name.into(),
            icon: None,
            order: None,
            entity_count: 0,
            primary_count: 0,
            group_names: Default::default(),
            group_favorites: Default::default(),
        }
    }

    fn run(query: &str, entities: &BTreeMap<String, EntityView>) -> Vec<Entry> {
        entries(Lang::En, query, entities, &[area("salon", "Salon")], &[])
    }

    fn of(entries: &[Entry], section: Section) -> Vec<&Entry> {
        entries.iter().filter(|e| e.section == section).collect()
    }

    #[test]
    fn hidden_and_diagnostic_entities_are_left_out() {
        let mut hidden = ent("l2", "Hidden", Kind::Light);
        hidden.ui.hidden = true;
        let mut diag = ent("l3", "Diag", Kind::Light);
        diag.category = EntityCategory::Diagnostic;
        let entities = house(vec![ent("l1", "Lamp", Kind::Light), hidden, diag]);
        let got = run("", &entities);
        assert_eq!(of(&got, Section::Lights).len(), 1);
    }

    #[test]
    fn sub_line_is_room_then_device() {
        let mut light = ent("l1", "Lamp", Kind::Light);
        light.device_name = Some("Hue bulb".into());
        let mut twin = ent("l2", "Light", Kind::Light);
        twin.device_name = Some("light".into());
        let entities = house(vec![light, twin]);
        let got = run("", &entities);
        let subs: Vec<_> = of(&got, Section::Lights)
            .iter()
            .map(|e| e.sub.as_deref())
            .collect();
        assert_eq!(subs, [Some("Salon · Hue bulb"), Some("Salon")]);
    }

    #[test]
    fn search_ignores_accents_and_matches_the_room() {
        let entities = house(vec![
            ent("l1", "Lampe évier", Kind::Light),
            ent("l2", "Other", Kind::Light),
        ]);
        let got = run("EVIER", &entities);
        assert_eq!(of(&got, Section::Lights).len(), 1);
        let got = run("  salon ", &entities);
        assert_eq!(of(&got, Section::Lights).len(), 2);
    }

    #[test]
    fn the_kind_word_finds_entities_in_either_language() {
        let entities = house(vec![
            ent("c1", "Shutter", Kind::Cover),
            ent("l1", "Lamp", Kind::Light),
        ]);
        for query in ["volet", "cover"] {
            let got = run(query, &entities);
            assert_eq!(of(&got, Section::Covers).len(), 1, "{query}");
            assert!(of(&got, Section::Lights).is_empty(), "{query}");
        }
    }

    #[test]
    fn sections_come_in_display_order() {
        let entities = house(vec![
            ent("s1", "Temp", Kind::Sensor),
            ent("c1", "Shutter", Kind::Cover),
            ent("l1", "Lamp", Kind::Light),
        ]);
        let got = entries(
            Lang::En,
            "",
            &entities,
            &[area("salon", "Salon")],
            &[("f1".into(), "Night".into())],
        );
        let mut sections: Vec<_> = got.iter().map(|e| e.section).collect();
        sections.dedup();
        assert_eq!(
            sections,
            [
                Section::Lights,
                Section::Covers,
                Section::Sensors,
                Section::Rooms,
                Section::FlowButtons,
                Section::Energy,
                Section::HistoryCharts,
            ]
        );
    }

    #[test]
    fn a_sensor_is_offered_as_a_sensor_and_as_a_chart() {
        let entities = house(vec![ent("s1", "Temp", Kind::Sensor)]);
        let got = run("", &entities);
        let sensor = of(&got, Section::Sensors);
        let chart = of(&got, Section::HistoryCharts);
        assert_eq!(sensor[0].kind, WidgetKind::Sensor);
        assert_eq!(
            sensor[0].config,
            WidgetConfig::Entity {
                entity_id: "s1".into(),
                title: None
            }
        );
        assert_eq!(chart[0].kind, WidgetKind::HistoryChart);
        assert_eq!(
            chart[0].config,
            WidgetConfig::History {
                entity_id: "s1".into(),
                range: DashboardHistoryRange::H24
            }
        );
    }

    #[test]
    fn empty_sections_have_no_rows() {
        let got = entries(Lang::En, "", &BTreeMap::new(), &[], &[]);
        assert!(got.iter().all(|e| e.section == Section::Energy));
        assert_eq!(got.len(), ENERGY_METRICS.len());
    }

    #[test]
    fn adding_a_light_builds_entity_config_with_smallest_allowed_span() {
        let w = make_widget(
            WidgetKind::Light,
            WidgetConfig::Entity {
                entity_id: "light.a".into(),
                title: None,
            },
        );
        assert_eq!(w.kind, WidgetKind::Light);
        assert_eq!(w.span, Span::X1x1);
        assert_eq!(
            w.config,
            WidgetConfig::Entity {
                entity_id: "light.a".into(),
                title: None
            }
        );
    }

    #[test]
    fn adding_a_history_chart_defaults_to_2x1_and_24h_range() {
        let w = make_widget(
            WidgetKind::HistoryChart,
            WidgetConfig::History {
                entity_id: "x".into(),
                range: DashboardHistoryRange::H24,
            },
        );
        assert_eq!(w.span, Span::X2x1);
        assert!(matches!(
            w.config,
            WidgetConfig::History {
                range: DashboardHistoryRange::H24,
                ..
            }
        ));
    }
}
