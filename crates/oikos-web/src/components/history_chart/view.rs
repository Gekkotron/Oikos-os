//! The `HistoryChart` Leptos component: four states (loading, empty, unavailable, error) and
//! the SVG renderers for the two flavors (step line and daily bars).

use std::cell::Cell;
use std::rc::Rc;

use leptos::prelude::*;
use oikos_api::{EnergyKind, HistoryDaily, HistoryRange, HistorySeries};

use super::format::{state_axis_labels, tick_labels};
use super::geom::{Bar, StepPath, YScale, daily_bars, step_path, y_scale};
use crate::api::history::{HistoryApiError, fetch_daily, fetch_series};

/// Which shape of chart the entity needs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Flavor {
    Series,
    Counter,
}

/// What the chart is currently drawing.
#[derive(Clone, Debug)]
enum State {
    Loading,
    Series(HistorySeries),
    Bars(HistoryDaily),
    NotRecorded,
    Unavailable,
    Error(String),
}

/// A late response's `request_id` must match the latest issued one, or the response is dropped.
pub fn is_stale(my_req: u32, latest: u32) -> bool {
    my_req != latest
}

/// Days to ask `/daily` for, given a non-24h range.
fn daily_days_for(range: HistoryRange) -> u16 {
    match range {
        HistoryRange::Week => 7,
        HistoryRange::Month => 30,
        HistoryRange::Year => 365,
        HistoryRange::Day => 1,
    }
}

#[component]
pub fn HistoryChart(
    #[prop(into)] entity_id: Signal<String>,
    #[prop(into)] range: Signal<HistoryRange>,
    #[prop(into)] flavor: Signal<Flavor>,
    #[prop(into)] kind: Signal<String>,
    #[prop(into)] unit: Signal<Option<String>>,
    #[prop(into)] power_pair: Signal<Option<String>>,
) -> impl IntoView {
    let state = RwSignal::new(State::Loading);
    let latest: Rc<Cell<u32>> = Rc::new(Cell::new(0));
    let retry_bump = RwSignal::new(0u32);

    // Fetch effect — refires on any input change (or Retry).
    {
        let latest = latest.clone();
        Effect::new(move |_| {
            let id = entity_id.get();
            let r = range.get();
            let f = flavor.get();
            let pp = power_pair.get();
            let _ = retry_bump.get();
            let my_req = latest.get() + 1;
            latest.set(my_req);
            state.set(State::Loading);

            let latest_check = latest.clone();
            leptos::task::spawn_local(async move {
                let outcome = fetch_for(&id, r, f, pp.as_deref()).await;
                if is_stale(my_req, latest_check.get()) {
                    return;
                }
                state.set(match outcome {
                    FetchResult::Series(Ok(s)) => State::Series(s),
                    FetchResult::Daily(Ok(d)) => State::Bars(d),
                    FetchResult::Series(Err(e)) | FetchResult::Daily(Err(e)) => map_error(e),
                });
            });
        });
    }

    // Each render reads `state` and composes the right view.
    let kind_rx = kind;
    let unit_rx = unit;
    let range_rx = range;
    view! {
        <div class="history-chart">
            {move || match state.get() {
                State::Loading => view! { <div class="history-chart__shimmer" aria-busy="true"></div> }.into_any(),
                State::NotRecorded => view! {
                    <p class="history-chart__empty">
                        "No data recorded for this entity yet. The hub starts recording from the moment the entity first appears."
                    </p>
                }.into_any(),
                State::Unavailable => view! {
                    <p class="history-chart__err">"History is unavailable right now. The database may be down."</p>
                }.into_any(),
                State::Error(msg) => {
                    let msg = msg.clone();
                    view! {
                        <div class="history-chart__err">
                            <p>"Could not load history: " {msg}</p>
                            <button type="button" class="chip" on:click=move |_| retry_bump.update(|v| *v += 1)>"Retry"</button>
                        </div>
                    }.into_any()
                }
                State::Series(s) => render_series(s, range_rx.get(), kind_rx.get(), unit_rx.get()).into_any(),
                State::Bars(d) => render_bars(d, range_rx.get()).into_any(),
            }}
        </div>
    }
}

enum FetchResult {
    Series(Result<HistorySeries, HistoryApiError>),
    Daily(Result<HistoryDaily, HistoryApiError>),
}

/// Which endpoint to call, with its arguments; pure routing decision for the current chart
/// state. Extracted from `fetch_for` so it is natively testable — the Review Focus items
/// (text-entity 404, counter without power pair on 24h) are branches here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FetchPlan {
    /// Call `/history/series?entity={0}&range={range}`.
    Series(String),
    /// Call `/history/daily?entity={0}&days={1}&kind=counter`.
    Daily(String, u16),
}

/// Decide which endpoint to fetch for the current `(flavor, range, power_pair)` triple.
pub fn plan_fetch(
    entity: &str,
    range: HistoryRange,
    flavor: Flavor,
    power_pair: Option<&str>,
) -> FetchPlan {
    match (flavor, range) {
        (Flavor::Series, _) => FetchPlan::Series(entity.to_owned()),
        (Flavor::Counter, HistoryRange::Day) => match power_pair {
            Some(pid) => FetchPlan::Series(pid.to_owned()),
            None => FetchPlan::Daily(entity.to_owned(), 1),
        },
        (Flavor::Counter, _) => FetchPlan::Daily(entity.to_owned(), daily_days_for(range)),
    }
}

async fn fetch_for(
    entity: &str,
    range: HistoryRange,
    flavor: Flavor,
    power_pair: Option<&str>,
) -> FetchResult {
    match plan_fetch(entity, range, flavor, power_pair) {
        FetchPlan::Series(id) => FetchResult::Series(fetch_series(&id, range).await),
        FetchPlan::Daily(id, days) => {
            FetchResult::Daily(fetch_daily(&id, days, EnergyKind::Counter).await)
        }
    }
}

fn map_error(err: HistoryApiError) -> State {
    match err {
        HistoryApiError::NotRecorded => State::NotRecorded,
        HistoryApiError::Unavailable => State::Unavailable,
        HistoryApiError::BadRequest(m) | HistoryApiError::Other(m) => State::Error(m),
    }
}

const VIEW_W: f64 = 400.0;
const VIEW_H: f64 = 220.0;

fn render_series(
    s: HistorySeries,
    range: HistoryRange,
    kind: String,
    unit: Option<String>,
) -> impl IntoView {
    let scale = y_scale(&s.points);
    // The x-axis is the whole range ending now, so short history sits at the right edge.
    let now = (crate::energy::now_ms() / 1000) as i64;
    let StepPath { d, band_d } = step_path(
        &s.points,
        scale,
        now - range.span_secs(),
        now,
        VIEW_W,
        VIEW_H,
    );
    let (y_lo, y_hi) = y_labels(&scale, &kind, unit.as_deref());
    let xticks = tick_labels(range, now);
    let (min_v, max_v) = series_min_max(&s.points);
    let unit_str = unit.unwrap_or_default();

    view! {
        <svg class="history-chart__svg" viewBox=format!("0 0 {VIEW_W} {VIEW_H}") preserveAspectRatio="xMidYMid meet" role="img">
            <line x1="0" y1=format!("{}", VIEW_H / 2.0) x2=format!("{}", VIEW_W) y2=format!("{}", VIEW_H / 2.0) class="history-chart__grid" />
            <path d=band_d class="history-chart__band" />
            <path d=d class="history-chart__line" fill="none" />
        </svg>
        <div class="history-chart__axis-y">
            <span>{y_hi}</span>
            <span>{y_lo}</span>
        </div>
        <div class="history-chart__axis-x">
            {xticks.into_iter().map(|(pos, l)| view! {
                <span style=format!("left: {}%", pos * 100.0)>{l}</span>
            }).collect::<Vec<_>>()}
        </div>
        <div class="history-chart__legend">
            <span>{legend_left_series(range)}</span>
            <span>{format!("min {} {} · max {} {}", round1(min_v), unit_str, round1(max_v), unit_str)}</span>
        </div>
    }
}

fn render_bars(d: HistoryDaily, range: HistoryRange) -> impl IntoView {
    let bars = daily_bars(&d.days, VIEW_W, VIEW_H);
    let total: f64 = d.days.iter().map(|d| d.1).sum();

    view! {
        <svg class="history-chart__svg" viewBox=format!("0 0 {VIEW_W} {VIEW_H}") preserveAspectRatio="xMidYMid meet" role="img">
            {bars.into_iter().map(|Bar { x, y, w, h, .. }| view! {
                <rect x=format!("{x}") y=format!("{y}") width=format!("{w}") height=format!("{h}") class="history-chart__bar" />
            }).collect::<Vec<_>>()}
        </svg>
        <div class="history-chart__legend">
            <span>{legend_left_counter(range)}</span>
            <span>{format!("{} total: {} kWh", range_name(range), round1(total))}</span>
        </div>
    }
}

fn series_min_max(points: &[(i64, f64, f64, f64)]) -> (f64, f64) {
    let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
    for p in points {
        if p.2 < lo {
            lo = p.2;
        }
        if p.3 > hi {
            hi = p.3;
        }
    }
    if !lo.is_finite() {
        lo = 0.0;
    }
    if !hi.is_finite() {
        hi = 0.0;
    }
    (lo, hi)
}

fn y_labels(scale: &YScale, kind: &str, _unit: Option<&str>) -> (String, String) {
    if let Some((lo, hi)) = state_axis_labels(None, kind) {
        return (lo, hi);
    }
    (round1(scale.min), round1(scale.max))
}

fn round1<T: Into<f64>>(v: T) -> String {
    let v = v.into();
    format!("{v:.1}")
}

fn range_name(r: HistoryRange) -> &'static str {
    match r {
        HistoryRange::Day => "Last 24 hours",
        HistoryRange::Week => "Last 7 days",
        HistoryRange::Month => "Last 30 days",
        HistoryRange::Year => "Last year",
    }
}

fn legend_left_series(r: HistoryRange) -> String {
    format!("{} · held-until-next-row", range_name(r))
}

fn legend_left_counter(r: HistoryRange) -> String {
    format!("{} · daily total", range_name(r))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stale_guard_rejects_old_request_ids() {
        assert!(is_stale(1, 2));
        assert!(is_stale(3, 5));
        assert!(!is_stale(5, 5));
        assert!(!is_stale(1, 1));
    }

    #[test]
    fn daily_days_for_matches_range_windows() {
        assert_eq!(daily_days_for(HistoryRange::Week), 7);
        assert_eq!(daily_days_for(HistoryRange::Month), 30);
        assert_eq!(daily_days_for(HistoryRange::Year), 365);
        assert_eq!(daily_days_for(HistoryRange::Day), 1);
    }

    #[test]
    fn plan_fetch_series_flavor_uses_entity_series_for_every_range() {
        for r in [
            HistoryRange::Day,
            HistoryRange::Week,
            HistoryRange::Month,
            HistoryRange::Year,
        ] {
            assert_eq!(
                plan_fetch("sensor.temp", r, Flavor::Series, None),
                FetchPlan::Series("sensor.temp".into())
            );
        }
    }

    #[test]
    fn plan_fetch_counter_24h_uses_paired_power_series_when_present() {
        assert_eq!(
            plan_fetch(
                "counter.e",
                HistoryRange::Day,
                Flavor::Counter,
                Some("sensor.power"),
            ),
            FetchPlan::Series("sensor.power".into())
        );
    }

    #[test]
    fn plan_fetch_counter_24h_falls_back_to_single_day_bar_without_power() {
        // Review Focus #2.
        assert_eq!(
            plan_fetch("counter.e", HistoryRange::Day, Flavor::Counter, None),
            FetchPlan::Daily("counter.e".into(), 1)
        );
    }

    #[test]
    fn plan_fetch_counter_long_ranges_use_daily_with_right_days() {
        assert_eq!(
            plan_fetch("counter.e", HistoryRange::Week, Flavor::Counter, None),
            FetchPlan::Daily("counter.e".into(), 7)
        );
        assert_eq!(
            plan_fetch("counter.e", HistoryRange::Month, Flavor::Counter, None),
            FetchPlan::Daily("counter.e".into(), 30)
        );
        assert_eq!(
            plan_fetch("counter.e", HistoryRange::Year, Flavor::Counter, None),
            FetchPlan::Daily("counter.e".into(), 365)
        );
    }

    #[test]
    fn map_error_routes_each_variant_to_its_state() {
        // Review Focus #1: a 404 on a text/alarm/vacuum entity renders the empty copy.
        assert!(matches!(
            map_error(HistoryApiError::NotRecorded),
            State::NotRecorded
        ));
        assert!(matches!(
            map_error(HistoryApiError::Unavailable),
            State::Unavailable
        ));
        assert!(matches!(
            map_error(HistoryApiError::BadRequest("bad".into())),
            State::Error(_)
        ));
        assert!(matches!(
            map_error(HistoryApiError::Other("boom".into())),
            State::Error(_)
        ));
    }

    #[test]
    fn series_min_max_handles_empty() {
        let (lo, hi) = series_min_max(&[]);
        assert_eq!((lo, hi), (0.0, 0.0));
    }
}
