//! `＋ Add widget` picker — grouped by widget kind, each entry builds a `Widget` with the
//! smallest allowed span for its kind, then calls `on_add`.

use leptos::prelude::*;
use oikos_core::dashboard::{
    DashboardHistoryRange, Span, Widget, WidgetConfig, WidgetKind, allowed_spans,
};
use oikos_core::model::Kind;

use crate::house::House;

/// Metrics the hub's energy widget understands; there is no entity behind them to list.
const ENERGY_METRICS: [(&str, &str); 6] = [
    ("today_kwh", "Today (kWh)"),
    ("week_kwh", "This week (kWh)"),
    ("month_kwh", "This month (kWh)"),
    ("grid_power_now", "Grid power now"),
    ("pv_power_now", "Solar power now"),
    ("self_consumption_pct", "Self-consumption"),
];

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

#[component]
pub fn AddWidgetPicker(on_add: Callback<Widget>) -> impl IntoView {
    let house = expect_context::<House>();
    let open = RwSignal::new(false);
    let trigger_ref = NodeRef::<leptos::html::Button>::new();
    // Viewport-anchor and direction, picked each time the panel opens. The panel is
    // position: fixed with z-index 60 so it sits above the sticky .topbar's stacking
    // context; left/top come from the trigger's viewport rect, and `open_up` picks
    // whichever side has more room so the panel never spills off-screen.
    let panel_pos = RwSignal::new((0.0_f64, 0.0_f64, false));

    let add = move |w: Widget| {
        on_add.run(w);
        open.set(false);
    };

    let toggle = move || {
        if !open.get_untracked()
            && let Some(button) = trigger_ref.get_untracked()
        {
            let rect = button.get_bounding_client_rect();
            let viewport_h = web_sys::window()
                .and_then(|w| w.inner_height().ok())
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0);
            let space_above = rect.top();
            let space_below = viewport_h - rect.bottom();
            let open_up = space_above > space_below;
            // When opening upward, top anchors the panel's bottom (translateY(-100%))
            // six pixels above the trigger; downward, top is six pixels below it.
            let top = if open_up {
                rect.top() - 6.0
            } else {
                rect.bottom() + 6.0
            };
            panel_pos.set((rect.left(), top, open_up));
        }
        open.update(|o| *o = !*o);
    };

    let entities_of = move |kind: Kind| {
        Memo::new(move |_| {
            house.entities.with(|m| {
                m.values()
                    .filter(|e| e.kind == kind)
                    .map(|e| (e.id.clone(), e.name.clone()))
                    .collect::<Vec<_>>()
            })
        })
    };
    let entity_pick = move |kind: WidgetKind| {
        Callback::new(move |(id, _name): (String, String)| {
            add(make_widget(
                kind,
                WidgetConfig::Entity {
                    entity_id: id,
                    title: None,
                },
            ));
        })
    };

    let areas = Memo::new(move |_| {
        house.areas.with(|a| {
            a.iter()
                .map(|x| (x.id.clone(), x.name.clone()))
                .collect::<Vec<_>>()
        })
    });
    let flows = Memo::new(move |_| {
        house.flows.with(|f| {
            f.iter()
                .map(|x| (x.definition.id.clone(), x.definition.name.clone()))
                .collect::<Vec<_>>()
        })
    });
    let energy = Memo::new(move |_| {
        ENERGY_METRICS
            .iter()
            .map(|(id, label)| (id.to_string(), label.to_string()))
            .collect::<Vec<_>>()
    });

    view! {
        <div class="dashboard-add-picker">
            <button
                node_ref=trigger_ref
                type="button"
                class="dashboard-add-picker__trigger"
                on:click=move |_| toggle()
            >
                "＋ Add widget"
            </button>
            <Show when=move || open.get()>
                <div
                    class="dashboard-add-picker__panel"
                    style=move || {
                        let (left, top, open_up) = panel_pos.get();
                        let t = if open_up { "translateY(-100%)" } else { "none" };
                        format!("left:{left}px;top:{top}px;transform:{t}")
                    }
                    on:click=move |ev| ev.stop_propagation()
                >
                    <AddGroup
                        title="Lights"
                        entries=entities_of(Kind::Light)
                        on_pick=entity_pick(WidgetKind::Light)
                    />
                    <AddGroup
                        title="Covers"
                        entries=entities_of(Kind::Cover)
                        on_pick=entity_pick(WidgetKind::Cover)
                    />
                    <AddGroup
                        title="Climates"
                        entries=entities_of(Kind::Climate)
                        on_pick=entity_pick(WidgetKind::Climate)
                    />
                    <AddGroup
                        title="Sensors"
                        entries=entities_of(Kind::Sensor)
                        on_pick=entity_pick(WidgetKind::Sensor)
                    />
                    <AddGroup
                        title="Water heaters"
                        entries=entities_of(Kind::WaterHeater)
                        on_pick=entity_pick(WidgetKind::WaterHeater)
                    />
                    <AddGroup
                        title="Room groups"
                        entries=areas
                        on_pick=Callback::new(move |(id, _): (String, String)| {
                            add(make_widget(
                                WidgetKind::RoomGroup,
                                WidgetConfig::Area { area_id: id, title: None },
                            ));
                        })
                    />
                    <AddGroup
                        title="Flow buttons"
                        entries=flows
                        on_pick=Callback::new(move |(id, _): (String, String)| {
                            add(make_widget(
                                WidgetKind::FlowButton,
                                WidgetConfig::Flow { flow_id: id, title: None },
                            ));
                        })
                    />
                    <AddGroup
                        title="Energy"
                        entries=energy
                        on_pick=Callback::new(move |(id, _): (String, String)| {
                            add(make_widget(
                                WidgetKind::EnergyStat,
                                WidgetConfig::Energy { metric: id, title: None },
                            ));
                        })
                    />
                    <AddGroup
                        title="History charts"
                        entries=entities_of(Kind::Sensor)
                        on_pick=Callback::new(move |(id, _): (String, String)| {
                            add(make_widget(
                                WidgetKind::HistoryChart,
                                WidgetConfig::History {
                                    entity_id: id,
                                    range: DashboardHistoryRange::H24,
                                },
                            ));
                        })
                    />
                </div>
            </Show>
        </div>
    }
}

#[component]
fn AddGroup(
    title: &'static str,
    entries: Memo<Vec<(String, String)>>,
    on_pick: Callback<(String, String)>,
) -> impl IntoView {
    // An empty group would be a bare heading; hide it.
    view! {
        <Show when=move || entries.with(|e| !e.is_empty())>
            <section class="dashboard-add-picker__group">
                <h4>{title}</h4>
                <ul>
                    <For
                        each=move || entries.get()
                        key=|(id, _)| id.clone()
                        children=move |(id, name)| {
                            let label = name.clone();
                            view! {
                                <li>
                                    <button
                                        type="button"
                                        on:click=move |_| on_pick.run((id.clone(), name.clone()))
                                    >
                                        {label}
                                    </button>
                                </li>
                            }
                        }
                    />
                </ul>
            </section>
        </Show>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
