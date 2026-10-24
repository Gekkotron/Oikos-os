//! The energy stat widget: one figure of the Energy page (a day, week or month of house
//! consumption, the grid or solar power now, the self-consumption) in a small card.

use std::cell::Cell;
use std::rc::Rc;

use leptos::prelude::*;
use leptos::task::spawn_local;
use oikos_api::EnergyKind;
use oikos_core::energy::{EnergyFlow, EnergyUi, NodeKind};

use crate::api::history::{fetch_daily, fetch_today};
use crate::energy::{
    HouseTodayParts, derived_house_today, energy_entities, energy_text, node_value_text, now_ms,
    plus_minus_ids,
};
use crate::house::House;
use crate::icons::BOLT;
use crate::screens::energy::by_role;

/// Metrics the hub's energy widget understands, with their labels; there is no entity behind
/// them to list. The library offers these and the widget reads them back.
pub const ENERGY_METRICS: [(&str, &str); 6] = [
    ("today_kwh", "Today (kWh)"),
    ("week_kwh", "This week (kWh)"),
    ("month_kwh", "This month (kWh)"),
    ("grid_power_now", "Grid power now"),
    ("pv_power_now", "Solar power now"),
    ("self_consumption_pct", "Self-consumption"),
];

/// What a metric id reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Metric {
    /// House consumption over the last `days` days; 1 is since midnight.
    Consumption {
        days: u16,
    },
    Power(NodeKind),
    SelfConsumption,
}

impl Metric {
    fn parse(id: &str) -> Option<Self> {
        Some(match id {
            "today_kwh" => Self::Consumption { days: 1 },
            "week_kwh" => Self::Consumption { days: 7 },
            "month_kwh" => Self::Consumption { days: 30 },
            "grid_power_now" => Self::Power(NodeKind::Grid),
            "pv_power_now" => Self::Power(NodeKind::Solar),
            "self_consumption_pct" => Self::SelfConsumption,
            _ => return None,
        })
    }
}

/// The label of the metric `id`, `None` for an id the hub does not know.
pub fn metric_label(id: &str) -> Option<&'static str> {
    ENERGY_METRICS
        .iter()
        .find(|(m, _)| *m == id)
        .map(|(_, label)| *label)
}

/// The live reading of a power or self-consumption metric, "—" while unknown.
fn live_text(metric: Metric, ui: &EnergyUi) -> String {
    match metric {
        Metric::Power(kind) => ui
            .diagram
            .iter()
            .find(|n| n.kind == kind)
            .map_or_else(|| "—".to_owned(), node_value_text),
        Metric::SelfConsumption => ui
            .self_consumption
            .map_or_else(|| "—".to_owned(), |p| format!("{p} %")),
        Metric::Consumption { .. } => "—".to_owned(),
    }
}

/// kWh of one counter over `days` days, from its daily totals.
fn sum_days(days: &[(i64, f64)]) -> f64 {
    days.iter().map(|(_, kwh)| kwh).sum()
}

/// The house's consumption over `days` days: its `plus` counters minus its `minus` ones.
async fn consumption(plus: &[String], minus: &[String], days: u16) -> Option<f64> {
    let one = async |id: &String| -> Option<f64> {
        if days == 1 {
            fetch_today(id, EnergyKind::Counter).await.ok()?.kwh
        } else {
            let daily = fetch_daily(id, days, EnergyKind::Counter).await.ok()?;
            Some(sum_days(&daily.days))
        }
    };
    let mut parts = HouseTodayParts::default();
    for id in plus {
        parts.plus.push(one(id).await);
    }
    for id in minus {
        parts.minus.push(one(id).await);
    }
    derived_house_today(&parts)
}

#[component]
pub fn EnergyStatWidget(metric: String, title: Option<String>) -> impl IntoView {
    let house = expect_context::<House>();
    let Some(parsed) = Metric::parse(&metric) else {
        return view! { <super::widget::Unavailable label=metric /> }.into_any();
    };
    let label = title
        .filter(|t| !t.is_empty())
        .or_else(|| metric_label(&metric).map(str::to_owned))
        .unwrap_or_default();

    let ui = Memo::new(move |_| {
        let entities = house.entities.with(energy_entities);
        EnergyFlow::build(&entities, now_ms(), None)
    });
    let kwh = RwSignal::new(None::<f64>);
    if let Metric::Consumption { days } = parsed {
        let counters = Memo::new(move |_| {
            house.entities.with(|m| {
                let all = energy_entities(m);
                plus_minus_ids(&by_role(&all), &all)
            })
        });
        let latest = Rc::new(Cell::new(0_u32));
        Effect::new(move |_| {
            let Some((plus, minus)) = counters.get() else {
                kwh.set(None);
                return;
            };
            let ticket = latest.get() + 1;
            latest.set(ticket);
            let latest = latest.clone();
            spawn_local(async move {
                let value = consumption(&plus, &minus, days).await;
                if latest.get() == ticket {
                    kwh.set(value);
                }
            });
        });
    }
    let text = move || match parsed {
        Metric::Consumption { .. } => energy_text(kwh.get()),
        _ => ui.with(|u| live_text(parsed, u)),
    };

    view! {
        <article class="card widget-stat">
            <div class="widget-stat__head">
                <span class="widget-stat__icon" inner_html=BOLT></span>
                <span class="widget-stat__label">{label}</span>
            </div>
            <span class="card__value widget-stat__value">{text}</span>
        </article>
    }
    .into_any()
}

#[cfg(test)]
mod tests {
    use oikos_core::energy::{EnergyNode, Flow, NodeStatus};

    use super::*;

    fn node(kind: NodeKind, watts: Option<f64>) -> EnergyNode {
        EnergyNode {
            id: format!("{kind:?}"),
            kind,
            name: None,
            value: watts,
            unit: None,
            flow: Flow::Idle,
            status: NodeStatus::Live,
            count: 0,
            sensor_ids: Vec::new(),
            icon: None,
            total_kwh: None,
            total_status: NodeStatus::Live,
            has_meter: false,
            water_total: None,
            water_total_unit: None,
        }
    }

    #[test]
    fn every_library_metric_is_understood_and_labelled() {
        for (id, label) in ENERGY_METRICS {
            assert!(Metric::parse(id).is_some(), "{id}");
            assert_eq!(metric_label(id), Some(label));
        }
        assert_eq!(Metric::parse("nope"), None);
        assert_eq!(metric_label("nope"), None);
    }

    #[test]
    fn periods_map_to_days() {
        assert_eq!(
            Metric::parse("today_kwh"),
            Some(Metric::Consumption { days: 1 })
        );
        assert_eq!(
            Metric::parse("month_kwh"),
            Some(Metric::Consumption { days: 30 })
        );
    }

    #[test]
    fn live_text_reads_the_node_of_the_metric() {
        let ui = EnergyUi {
            diagram: vec![
                node(NodeKind::Solar, Some(1_500.0)),
                node(NodeKind::Grid, None),
            ],
            self_consumption: Some(62),
            ..EnergyUi::default()
        };
        assert_eq!(live_text(Metric::Power(NodeKind::Solar), &ui), "1.5 kW");
        assert_eq!(live_text(Metric::Power(NodeKind::Grid), &ui), "—");
        assert_eq!(live_text(Metric::Power(NodeKind::Battery), &ui), "—");
        assert_eq!(live_text(Metric::SelfConsumption, &ui), "62 %");
    }

    #[test]
    fn daily_totals_add_up() {
        assert_eq!(sum_days(&[(1, 2.5), (2, 4.0)]), 6.5);
        assert_eq!(sum_days(&[]), 0.0);
    }
}
