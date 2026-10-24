//! The solar card of the home header: today's kWh, the day curve and tomorrow, one link to the
//! solar page.

use chrono::{DateTime, Utc};
use leptos::prelude::*;
use leptos_router::components::A;

use super::curve::{day_curve, producing_span};
use super::measured::SolarMeasured;
use super::model::{day_bounds, forecast_so_far, local_date, periods_in, tomorrow};
use crate::components::now::{Now, unix_now};
use crate::house::House;
use crate::i18n::{self, Lang, SolarText};
use crate::icons;
use crate::screens::flow_canvas::sun::house_place;

const W: f64 = 200.0;
const H: f64 = 60.0;

/// Everything the card draws, recomputed when the forecast, the clock or the measures change.
#[derive(Clone, Debug, Default, PartialEq)]
struct CardData {
    /// Measured production and forecast total, kWh; the first is `None` while nothing is measured.
    measured: Option<f64>,
    so_far: f64,
    total: Option<f64>,
    remaining: Option<f64>,
    tomorrow: Option<(f64, f64, f64)>,
    stale_at: Option<chrono::NaiveTime>,
    forecast_area: String,
    forecast_line: String,
    measured_area: String,
    /// The measured area is the forecast's own estimates, not production.
    measured_is_forecast: bool,
    now_x: Option<f64>,
}

fn card_data(house: House, now: u64, measured: Option<SolarMeasured>) -> Option<CardData> {
    let zone = house.flows_config.with(|c| house_place(c.as_ref()).1);
    house.solar.with(|s| {
        let s = s.as_ref()?;
        let today = local_date(zone, now);
        let (from, to) = day_bounds(zone, today);
        let day = periods_in(&s.periods, from, to);
        let today_kwh = measured.and_then(|m| m.today_kwh.get());
        let real: Vec<(u64, f64)> = measured.map(|m| m.periods.get()).unwrap_or_default();
        let (points, is_forecast) = if real.is_empty() {
            let est = day
                .iter()
                .filter(|p| p.end <= now)
                .map(|p| (p.end, p.estimate))
                .collect();
            (est, true)
        } else {
            (real, false)
        };
        let so_far = forecast_so_far(&day, now);
        let total = s.today.estimate.or_else(|| {
            (!day.is_empty()).then(|| {
                day.iter()
                    .map(|p| p.estimate * f64::from(p.minutes) / 60.0)
                    .sum()
            })
        });
        let mut data = CardData {
            measured: today_kwh,
            so_far,
            total,
            remaining: s.now.remaining_today,
            tomorrow: tomorrow(s, today).map(|d| (d.estimate, d.low, d.high)),
            stale_at: s
                .stale
                .then(|| {
                    DateTime::<Utc>::from_timestamp(s.fetched_at as i64, 0)
                        .map(|t| t.with_timezone(&zone).time())
                })
                .flatten(),
            measured_is_forecast: is_forecast,
            ..CardData::default()
        };
        if let Some(span) = producing_span(&day) {
            let c = day_curve(&day, &points, span, now, W, H);
            data.forecast_area = c.forecast_area;
            data.forecast_line = c.forecast_line;
            data.measured_area = c.measured_area;
            data.now_x = c.now_x;
        }
        Some(data)
    })
}

/// The solar card: a link to the solar page.
#[component]
pub fn SolarCard() -> impl IntoView {
    let house: House = expect_context();
    let lang = use_context::<RwSignal<Lang>>().unwrap_or_else(|| RwSignal::new(Lang::En));
    let now = use_context::<Now>().map_or_else(|| RwSignal::new(unix_now()), |n| n.0);
    let measured = use_context::<SolarMeasured>();
    let data = Memo::new(move |_| card_data(house, now.get(), measured));

    let title = move || {
        let d = data.get()?;
        let l = lang.get();
        let total = d
            .total
            .map_or_else(|| "–".to_owned(), |t| i18n::solar_kwh(l, t));
        let unit = i18n::solar_text(l, SolarText::Kwh);
        Some(match d.measured {
            Some(m) => format!("{} / {total} {unit}", i18n::solar_kwh(l, m)),
            None => format!("≈ {} / {total} {unit}", i18n::solar_kwh(l, d.so_far)),
        })
    };
    let bottom = move || {
        let d = data.get()?;
        let l = lang.get();
        let line = match d.stale_at {
            Some(at) => i18n::data_from(l, at),
            None => i18n::solar_bottom(l, d.remaining, d.tomorrow),
        };
        (!line.is_empty()).then_some(line)
    };
    let field = move |pick: fn(&CardData) -> String| {
        move || data.get().map(|d| pick(&d)).unwrap_or_default()
    };
    let measured_class = move || {
        if data.get().is_some_and(|d| d.measured_is_forecast) {
            "solar-curve__measured solar-curve__measured--forecast"
        } else {
            "solar-curve__measured"
        }
    };
    let now_line = move || {
        data.get().and_then(|d| d.now_x).map(|x| {
            view! { <line class="solar-curve__now" x1=x x2=x y1="0" y2=H /> }
        })
    };

    view! {
        <A href="/solar" attr:class="solar-card" attr:aria-label=move || i18n::solar_text(lang.get(), SolarText::CardLabel)>
            <span class="solar-card__head">
                <span class="solar-card__icon" inner_html=icons::SUN></span>
                <span class="solar-card__title">{title}</span>
            </span>
            <svg class="solar-curve" viewBox="0 0 200 60" preserveAspectRatio="none" aria-hidden="true">
                <path class="solar-curve__forecast" d=field(|d| d.forecast_area.clone()) />
                <path class="solar-curve__line" d=field(|d| d.forecast_line.clone()) />
                <path class=measured_class d=field(|d| d.measured_area.clone()) />
                {now_line}
            </svg>
            {move || bottom().map(|b| view! { <span class="solar-card__line">{b}</span> })}
        </A>
    }
}
