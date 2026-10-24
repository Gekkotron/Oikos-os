//! The history chart widget: the entity's history over a fixed range, in a card.

use leptos::prelude::*;
use oikos_api::{EntityView, HistoryRange};
use oikos_core::dashboard::DashboardHistoryRange;

use crate::components::history_chart::HistoryChart;
use crate::format::entity_title;
use crate::house::House;
use crate::screens::history_detail::{flavor_for_unit, power_pair_of};

/// The chart range closest to a widget's: the chart has no 1 h or 6 h view, so those show a day.
pub fn chart_range(range: DashboardHistoryRange) -> HistoryRange {
    match range {
        DashboardHistoryRange::H1 | DashboardHistoryRange::H6 | DashboardHistoryRange::H24 => {
            HistoryRange::Day
        }
        DashboardHistoryRange::D7 => HistoryRange::Week,
        DashboardHistoryRange::D30 => HistoryRange::Month,
    }
}

#[component]
pub fn HistoryWidget(entity: EntityView, range: DashboardHistoryRange) -> impl IntoView {
    let house = expect_context::<House>();
    let flavor = flavor_for_unit(entity.unit.as_deref());
    let id = entity.id.clone();
    let power_pair = Memo::new(move |_| house.entities.with(|m| power_pair_of(m, &id)));
    view! {
        <article class="card widget-history">
            <span class="widget-history__title">{entity_title(&entity)}</span>
            <HistoryChart
                entity_id=entity.id.clone()
                range=chart_range(range)
                flavor=flavor
                kind=format!("{:?}", entity.kind)
                unit=entity.unit.clone()
                power_pair=power_pair
            />
        </article>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn widget_ranges_map_to_the_nearest_chart_range() {
        assert_eq!(chart_range(DashboardHistoryRange::H1), HistoryRange::Day);
        assert_eq!(chart_range(DashboardHistoryRange::H6), HistoryRange::Day);
        assert_eq!(chart_range(DashboardHistoryRange::H24), HistoryRange::Day);
        assert_eq!(chart_range(DashboardHistoryRange::D7), HistoryRange::Week);
        assert_eq!(chart_range(DashboardHistoryRange::D30), HistoryRange::Month);
    }
}
