//! The Energy screen: the live power flow of the house, a port of the app's Energy tab.
//!
//! The diagram is one inline SVG; the glyphs are `<foreignObject>` spans holding the icon
//! constants, so the same strings serve the cards and the diagram.

use std::collections::BTreeMap;

use leptos::prelude::*;
use leptos::task::spawn_local;
use oikos_api::{ConnectionStatus, EnergyKind};
use oikos_core::energy::{
    ARC_SLOTS, ConsumerOrder, EnergyEntity, EnergyFlow, EnergyHint, EnergyMeters, EnergyNode,
    EnergyRole, EnergyUi, Flow, NodeKind, NodeStatus,
};

use crate::api::history::fetch_today;
use crate::energy::{
    CLEARANCE, HouseTodayParts, NODE_R, VIEW_H, VIEW_W, aria_sentence, derived_house_today, edge,
    energy_entities, energy_text, meter_text, node_label, node_value_text, now_ms, plus_minus_ids,
    positions, power_text, should_fire_mount, speed, stroke,
};
use crate::house::House;
use crate::icons::{consumer_icon, role_icon};
use crate::screens::energy_day::GridDayCard;

/// Where a node's labels end, below its centre, plus a gap: where solar's edge starts.
const LABELS_BOTTOM: f64 = NODE_R + 29.0;
/// Longest label drawn before it is ellipsised.
const LABEL_CHARS: usize = 12;

fn kind_class(kind: NodeKind) -> &'static str {
    match kind {
        NodeKind::Solar => "solar",
        NodeKind::Grid => "grid",
        NodeKind::Battery => "battery",
        NodeKind::House => "house",
        NodeKind::Consumer | NodeKind::More => "consumer",
        NodeKind::Water => "water",
    }
}

fn short(label: String) -> String {
    if label.chars().count() > LABEL_CHARS {
        let cut: String = label.chars().take(LABEL_CHARS - 1).collect();
        format!("{}…", cut.trim_end())
    } else {
        label
    }
}

fn glyph(n: &EnergyNode) -> &'static str {
    match n.kind {
        NodeKind::Consumer => consumer_icon(n.icon),
        kind => role_icon(kind),
    }
}

/// What a card shows as its lifetime figure: the water meter, or the node's energy counter.
fn card_total(n: &EnergyNode) -> String {
    if n.kind == NodeKind::Water {
        return n
            .water_total
            .map(|v| format!("Meter {}", meter_text(v, n.water_total_unit.as_deref())))
            .unwrap_or_default();
    }
    if !n.has_meter {
        "No energy meter".to_string()
    } else if n.total_status == NodeStatus::Unavailable {
        "Unavailable".to_string()
    } else {
        energy_text(n.total_kwh)
    }
}

type Positions = Memo<BTreeMap<String, (f64, f64)>>;

#[component]
fn DiagramEdge(id: String, kind: NodeKind, ui: Memo<EnergyUi>, pos: Positions) -> impl IntoView {
    let node = Memo::new(move |_| ui.with(|u| u.diagram.iter().find(|n| n.id == id).cloned()));
    let path = move || {
        let n = node.get()?;
        pos.with(|p| {
            let at = *p.get(&n.id)?;
            let hub = *p.get("house")?;
            let below = (n.kind == NodeKind::Solar).then_some(at.1 + LABELS_BOTTOM);
            Some(edge(at, hub, CLEARANCE, below).path())
        })
    };
    let flow = move || node.get().map_or(Flow::Idle, |n| n.flow);
    view! {
        <path
            class=format!("energy-edge energy-edge--{}", kind_class(kind))
            class:energy-edge--idle=move || flow() == Flow::Idle
            class:energy-edge--out=move || flow() == Flow::Out
            d=move || path().unwrap_or_default()
            style=move || {
                node.get()
                    .map(|n| {
                        format!(
                            "stroke-width: {}px; animation-duration: {}s",
                            stroke(n.value, n.flow),
                            1.0 / speed(n.value),
                        )
                    })
                    .unwrap_or_default()
            }
        />
    }
}

#[component]
fn DiagramNode(
    id: String,
    ui: Memo<EnergyUi>,
    pos: Positions,
    open: Callback<String>,
    expanded: RwSignal<bool>,
) -> impl IntoView {
    let node = Memo::new(move |_| ui.with(|u| u.diagram.iter().find(|n| n.id == id).cloned()));
    let Some(first) = node.get_untracked() else {
        return ().into_any();
    };
    let kind = first.kind;
    let transform = move || {
        node.get()
            .and_then(|n| pos.with(|p| p.get(&n.id).copied()))
            .map(|(x, y)| format!("translate({x} {y})"))
            .unwrap_or_default()
    };
    let off = move || {
        node.get()
            .is_some_and(|n| n.status == NodeStatus::Unavailable)
    };
    let on_click = move |_| {
        let Some(n) = node.get_untracked() else {
            return;
        };
        if n.kind == NodeKind::More {
            expanded.update(|e| *e = !*e);
        } else if let [only] = n.sensor_ids.as_slice() {
            open.run(only.clone());
        }
    };
    let clickable = kind == NodeKind::More || !first.sensor_ids.is_empty();
    view! {
        <g
            class=format!("energy-node energy-node--{}", kind_class(kind))
            class:energy-node--off=off
            class:energy-node--click=clickable
            transform=transform
            on:click=on_click
        >
            <circle class="energy-node__disc" r=NODE_R />
            <foreignObject x=-10 y=-10 width=20 height=20>
                <span
                    class="energy-node__glyph"
                    inner_html=move || node.get().map(|n| glyph(&n)).unwrap_or_default()
                ></span>
            </foreignObject>
            // The app's order: the value under the disc, the name under the value.
            <text class="energy-node__label" y=NODE_R + 23.0 text-anchor="middle">
                {move || node.get().map(|n| short(node_label(&n))).unwrap_or_default()}
            </text>
            <text class="energy-node__value" y=NODE_R + 11.0 text-anchor="middle">
                {move || {
                    node.get()
                        .map(|n| {
                            if n.kind == NodeKind::More {
                                power_text(n.value)
                            } else {
                                node_value_text(&n)
                            }
                        })
                        .unwrap_or_default()
                }}
            </text>
        </g>
    }
    .into_any()
}

#[component]
fn EnergyCard(
    id: String,
    ui: Memo<EnergyUi>,
    open: Callback<String>,
    names: Memo<BTreeMap<String, String>>,
    today: Signal<Option<f64>>,
) -> impl IntoView {
    let node = Memo::new(move |_| ui.with(|u| u.cards.iter().find(|n| n.id == id).cloned()));
    let sensors_open = RwSignal::new(false);
    let on_head = move |_| {
        let Some(n) = node.get_untracked() else {
            return;
        };
        match n.sensor_ids.as_slice() {
            [] => {}
            [only] => open.run(only.clone()),
            _ => sensors_open.update(|o| *o = !*o),
        }
    };
    move || {
        let n = node.get()?;
        let off = if n.kind == NodeKind::Water {
            n.status == NodeStatus::Unavailable
        } else {
            n.total_status == NodeStatus::Unavailable || n.status == NodeStatus::Unavailable
        };
        let many = n.sensor_ids.len() > 1;
        let sensors = n.sensor_ids.clone();
        let total = card_total(&n);
        Some(view! {
            <article class="card energy-card" class:card--offline=off>
                <button
                    type="button"
                    class="card__header"
                    disabled=n.sensor_ids.is_empty()
                    aria-expanded=many.then(|| sensors_open.get().to_string())
                    on:click=on_head
                >
                    <span class="card__icon" inner_html=glyph(&n)></span>
                    <span class="card__name">{node_label(&n)}</span>
                </button>
                <div class="card__body">
                    <span class="card__value">{node_value_text(&n)}</span>
                    <span class="energy-card__total">{total}</span>
                    <span class="energy-tile-today">
                        {move || today.get().map(|v| format!("{v:.1} kWh today"))}
                    </span>
                </div>
                <Show when=move || many && sensors_open.get()>
                    <ul class="energy-card__sensors">
                        {sensors
                            .iter()
                            .map(|sid| {
                                let sid = sid.clone();
                                let label = names
                                    .with(|m| m.get(&sid).cloned())
                                    .unwrap_or_else(|| sid.clone());
                                view! {
                                    <li>
                                        <button
                                            type="button"
                                            class="btn btn--compact"
                                            on:click=move |_| open.run(sid.clone())
                                        >
                                            {label}
                                        </button>
                                    </li>
                                }
                            })
                            .collect_view()}
                    </ul>
                </Show>
            </article>
        })
    }
}

/// Shared, app-scoped kWh-today map: counter id → kWh since local midnight (`None` while
/// loading or on error). Installed once in `install_history_refresh` from `app.rs`; the Energy
/// screen reads it from context, so navigating between screens never re-arms the timer or
/// stacks another visibilitychange listener on top of the old one.
#[derive(Clone, Copy)]
pub struct KwhToday(pub RwSignal<BTreeMap<String, Option<f64>>>);

/// kWh today for one card, from the kwh_today map built by `install_today_refresh`.
fn today_for_card(
    card_id: &str,
    house: House,
    ui: Memo<EnergyUi>,
    kwh_today: RwSignal<BTreeMap<String, Option<f64>>>,
) -> Option<f64> {
    let node = ui.with(|u| u.cards.iter().find(|n| n.id == card_id).cloned())?;
    let all = house.entities.with(energy_entities);
    let map = kwh_today.get();
    if node.kind == NodeKind::House {
        let by_role = by_role(&all);
        let (plus_ids, minus_ids) = plus_minus_ids(&by_role, &all)?;
        let parts = HouseTodayParts {
            plus: plus_ids
                .iter()
                .map(|id| map.get(id).copied().flatten())
                .collect(),
            minus: minus_ids
                .iter()
                .map(|id| map.get(id).copied().flatten())
                .collect(),
        };
        return derived_house_today(&parts);
    }
    let sensors: Vec<&EnergyEntity> = node
        .sensor_ids
        .iter()
        .filter_map(|sid| all.iter().find(|e| e.id == *sid))
        .collect();
    let counters = EnergyMeters::node_meters(&sensors, &all);
    if counters.is_empty() {
        return None;
    }
    let mut sum = 0.0;
    for c in &counters {
        sum += map.get(&c.id).copied().flatten()?;
    }
    Some(sum)
}

fn by_role(all: &[EnergyEntity]) -> BTreeMap<EnergyRole, Vec<&EnergyEntity>> {
    let mut m: BTreeMap<EnergyRole, Vec<&EnergyEntity>> = BTreeMap::new();
    for e in all {
        if let Some(r) = e.role {
            m.entry(r).or_default().push(e);
        }
    }
    m
}

#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
const TODAY_INTERVAL_MS: u32 = 300_000;

/// Install the shared `KwhToday` map and keep it fresh: fetch once as soon as the entity map is
/// loaded, then every 5 min while the tab is visible, plus one extra refresh on
/// `visibilitychange` → visible. Call this once from `App` so there is exactly one timer and
/// one listener for the whole page lifetime.
pub fn install_history_refresh(house: House) {
    let kwh = KwhToday(RwSignal::new(BTreeMap::new()));
    provide_context(kwh);

    let fire: std::rc::Rc<dyn Fn()> = std::rc::Rc::new(move || {
        let ids: Vec<String> = house.entities.with(|m| {
            let all = energy_entities(m);
            EnergyMeters::counters(&all)
                .into_iter()
                .map(|e| e.id.clone())
                .collect()
        });
        for id in ids {
            let id_c = id.clone();
            spawn_local(async move {
                let value = fetch_today(&id_c, EnergyKind::Counter)
                    .await
                    .ok()
                    .and_then(|t| t.kwh);
                kwh.0.update(|m| {
                    m.insert(id_c, value);
                });
            });
        }
    });

    let fire_mount = fire.clone();
    Effect::new(move |prev: Option<bool>| {
        let has = house.entities.with(|m| !m.is_empty());
        if should_fire_mount(prev, has) {
            fire_mount();
        }
        has
    });

    #[cfg(target_arch = "wasm32")]
    {
        use wasm_bindgen::JsCast;
        fn is_hidden() -> bool {
            web_sys::window()
                .and_then(|w| w.document())
                .is_some_and(|d| d.hidden())
        }
        let fire_tick = fire.clone();
        let interval = gloo_timers::callback::Interval::new(TODAY_INTERVAL_MS, move || {
            if !is_hidden() {
                fire_tick();
            }
        });
        interval.forget();

        if let Some(doc) = web_sys::window().and_then(|w| w.document()) {
            let fire_vis = fire;
            let closure = wasm_bindgen::closure::Closure::wrap(Box::new(move || {
                if !is_hidden() {
                    fire_vis();
                }
            }) as Box<dyn Fn()>);
            let _ = doc.add_event_listener_with_callback(
                "visibilitychange",
                closure.as_ref().unchecked_ref(),
            );
            closure.forget();
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = fire;
    }
}

#[component]
pub fn EnergyScreen() -> impl IntoView {
    let house = expect_context::<House>();
    let navigate = leptos_router::hooks::use_navigate();
    let open = Callback::new(move |id: String| {
        navigate(
            &format!("/entity/{}", crate::api::encode_id(&id)),
            Default::default(),
        )
    });

    let order = StoredValue::new(None::<ConsumerOrder>);
    let ui = Memo::new(move |_| {
        let entities = house.entities.with(energy_entities);
        let built = order.with_value(|prev| EnergyFlow::build(&entities, now_ms(), prev.as_ref()));
        order.set_value(built.order.clone());
        built
    });
    let pos: Positions = Memo::new(move |_| ui.with(|u| positions(&u.diagram)));
    let names = Memo::new(move |_| {
        house.entities.with(|m| {
            m.values()
                .map(|e| (e.id.clone(), e.name.clone()))
                .collect::<BTreeMap<_, _>>()
        })
    });
    let expanded = RwSignal::new(false);
    let online = move || matches!(house.connection.get(), ConnectionStatus::Online);
    let loading = move || ui.with(|u| u.loading) && !online();
    let nothing = move || ui.with(|u| u.loading || u.empty);

    let subtitle = move || {
        ui.with(|u| match u.self_consumption {
            Some(p) => format!("Live · self-consumption {p} %"),
            None => "Live".to_string(),
        })
    };
    let hint = move || ui.with(|u| u.hint == Some(EnergyHint::CheckSign));
    let edges = move || {
        ui.with(|u| {
            u.diagram
                .iter()
                .filter(|n| !matches!(n.kind, NodeKind::House | NodeKind::Water))
                .map(|n| (n.id.clone(), n.kind))
                .collect::<Vec<_>>()
        })
    };
    let nodes = move || ui.with(|u| u.diagram.iter().map(|n| n.id.clone()).collect::<Vec<_>>());
    let cards = move || ui.with(|u| u.cards.iter().map(|n| n.id.clone()).collect::<Vec<_>>());
    let description = move || ui.with(aria_sentence);
    let folded = move || ui.with(|u| u.consumers.len() > ARC_SLOTS);

    // Shared KwhToday map, installed once by App::install_history_refresh.
    let kwh_today = expect_context::<KwhToday>().0;

    view! {
        <section class="energy">
            <h1 class="room__title">"Energy"</h1>
            <Show
                when=move || !loading()
                fallback=|| {
                    view! {
                        <div class="grid" aria-busy="true">
                            <div class="card card--skeleton"></div>
                            <div class="card card--skeleton"></div>
                        </div>
                    }
                }
            >
                <Show
                    when=move || !nothing()
                    fallback=|| {
                        view! {
                            <div class="energy__empty">
                                <p class="room__empty">
                                    "Long-press a power sensor on Home and choose an energy role."
                                </p>
                                <a class="btn btn--compact" href="/">
                                    "Go to Home"
                                </a>
                            </div>
                        }
                    }
                >
                    <p class="energy__subtitle">{subtitle}</p>
                    <Show when=hint>
                        <p class="energy__hint">"Check the grid or battery sign"</p>
                    </Show>
                    <div class="energy-diagram">
                        <svg
                            class="energy-diagram__svg"
                            viewBox=format!("0 0 {VIEW_W} {VIEW_H}")
                            role="img"
                            aria-label=description
                        >
                            <title>{description}</title>
                            <For
                                each=edges
                                key=|(id, _)| id.clone()
                                children=move |(id, kind)| {
                                    view! { <DiagramEdge id=id kind=kind ui=ui pos=pos /> }
                                }
                            />
                            <For
                                each=nodes
                                key=|id| id.clone()
                                children=move |id| {
                                    view! {
                                        <DiagramNode
                                            id=id
                                            ui=ui
                                            pos=pos
                                            open=open
                                            expanded=expanded
                                        />
                                    }
                                }
                            />
                        </svg>
                    </div>
                    <Show when=folded>
                        <button
                            type="button"
                            class="btn btn--compact energy__more-toggle"
                            aria-expanded=move || expanded.get().to_string()
                            on:click=move |_| expanded.update(|e| *e = !*e)
                        >
                            {move || {
                                format!("Show all {} consumers", ui.with(|u| u.consumers.len()))
                            }}
                        </button>
                    </Show>
                    <GridDayCard />
                    <div class="grid energy-cards">
                        <For
                            each=cards
                            key=|id| id.clone()
                            children=move |id| {
                                let card_id = id.clone();
                                let today = Signal::derive(move || {
                                    today_for_card(&card_id, house, ui, kwh_today)
                                });
                                view! {
                                    <EnergyCard id=id ui=ui open=open names=names today=today />
                                }
                            }
                        />
                    </div>
                    <Show when=move || folded() && expanded.get()>
                        <section class="energy-more" aria-label="All consumers">
                            <h2 class="energy-more__title">"All consumers"</h2>
                            <ul>
                                {move || {
                                    ui.with(|u| {
                                        u.consumers
                                            .iter()
                                            .map(|c| {
                                                let sid = c.sensor_ids.first().cloned();
                                                let name = node_label(c);
                                                let value = node_value_text(c);
                                                view! {
                                                    <li>
                                                        <button
                                                            type="button"
                                                            class="energy-more__row"
                                                            on:click=move |_| {
                                                                if let Some(s) = sid.clone() {
                                                                    open.run(s);
                                                                }
                                                            }
                                                        >
                                                            <span class="energy-more__name">{name}</span>
                                                            <span class="energy-more__value">{value}</span>
                                                        </button>
                                                    </li>
                                                }
                                            })
                                            .collect_view()
                                    })
                                }}
                            </ul>
                        </section>
                    </Show>
                </Show>
            </Show>
        </section>
    }
}
