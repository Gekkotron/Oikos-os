//! The weather page the header's weather card links to: now, the next five days, the source.

use chrono::{DateTime, NaiveDate, TimeZone, Utc};
use leptos::prelude::*;
use oikos_api::{WeatherDay, WeatherView};

use crate::components::now::{Now, unix_now};
use crate::home_header::mood::{Detail, RAIN_MIN_MM, sun_today};
use crate::home_header::view::degrees;
use crate::home_header::weather_fmt::{compass_index, condition_icon, uv_level};
use crate::house::House;
use crate::i18n::{self, Lang, WeatherText as T, format, weather_text};
use crate::icons;
use crate::nav::BackLink;
use crate::screens::flow_canvas::sun::house_place;

const DASH: &str = "–";

/// A number with `decimals` places and a unit after a no-break space, or a dash when the value is
/// unknown.
fn measure(lang: Lang, v: Option<f64>, decimals: usize, unit: Option<&str>) -> String {
    v.map_or_else(
        || DASH.to_owned(),
        |v| format::with_unit(i18n::home_number(lang, v + 0.0, decimals), unit),
    )
}

fn tile(label: &'static str, value: AnyView, sub: String) -> impl IntoView {
    view! {
        <div class="weather-tile">
            <div class="weather-tile__label">{label}</div>
            <div class="weather-tile__value">{value}</div>
            <div class="weather-tile__sub">{sub}</div>
        </div>
    }
}

fn day_card(lang: Lang, d: &WeatherDay) -> impl IntoView {
    let title = NaiveDate::parse_from_str(&d.date, "%Y-%m-%d")
        .map(|date| i18n::short_weekday(lang, date))
        .unwrap_or_else(|_| d.date.clone());
    let icon = d.condition.map(|c| condition_icon(c, true));
    let max = d.max.map_or_else(|| DASH.to_owned(), |t| degrees(lang, t));
    let min = d.min.map_or_else(|| DASH.to_owned(), |t| degrees(lang, t));
    let chance = d
        .precipitation_probability
        .map(|p| format::with_unit(i18n::home_number(lang, p, 0), Some("%")));
    view! {
        <div class="weather-day">
            <div class="weather-day__name">{title}</div>
            <span class="weather-day__icon" inner_html=icon.unwrap_or_default()></span>
            <div class="weather-day__range">
                <b>{max}</b>
                " "
                <span>{min}</span>
            </div>
            <div class="weather-day__rain">
                <span class="weather-day__drop" inner_html=icons::DROP></span>
                {chance}
            </div>
        </div>
    }
}

/// The page body for one weather document; `midday` is any instant of the house-local day.
fn content(
    w: &WeatherView,
    lang: Lang,
    midday: DateTime<Utc>,
    house: House,
) -> impl IntoView + use<'_> {
    let cur = &w.current;
    let zone = house.flows_config.with(|c| house_place(c.as_ref()).1);
    let (sunrise, sunset, _, fallback) = house.flows_config.with(|c| sun_today(c.as_ref(), midday));
    let fetched = DateTime::<Utc>::from_timestamp(w.fetched_at as i64, 0)
        .map(|t| t.with_timezone(&zone).time());
    let updated = fetched.map(|at| i18n::updated_at(lang, at));

    let icon = cur
        .condition
        .map(|c| condition_icon(c, cur.daylight.unwrap_or(true)))
        .unwrap_or_default();
    let temp = cur.temperature.map_or_else(
        || DASH.to_owned(),
        |t| format!("{}°", i18n::home_number(lang, t + 0.0, 1)),
    );
    let label = cur.condition.map(|c| i18n::condition_label(lang, c));
    let feels = cur
        .apparent_temperature
        .map(|t| i18n::feels_like(lang, &degrees(lang, t)));
    let headline = [label.map(str::to_owned), feels]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" · ");
    let range = match (w.today.min, w.today.max) {
        (Some(lo), Some(hi)) => Some(format!(
            "{} {} / {}",
            weather_text(lang, T::Today),
            degrees(lang, lo),
            degrees(lang, hi)
        )),
        _ => None,
    };
    let tonight = w
        .tonight_min
        .map(|t| format!("{} {}", weather_text(lang, T::TonightMin), degrees(lang, t)));
    let rain_soon = match w.rain_next_hour {
        Some(mm) if mm > RAIN_MIN_MM => i18n::rain_next_hour(lang, &i18n::home_number(lang, mm, 1)),
        Some(_) => weather_text(lang, T::NoRainSoon).to_owned(),
        None => String::new(),
    };

    let wind_value: AnyView = match cur.wind_speed {
        Some(speed) => {
            let arrow = cur.wind_direction.map(|deg| {
                view! {
                    <span
                        class="weather-wind"
                        style=format!("transform: rotate({deg}deg)")
                        inner_html=icons::ARROW_UP
                    ></span>
                }
            });
            view! { {arrow} {measure(lang, Some(speed), 0, Some("km/h"))} }.into_any()
        }
        None => DASH.into_any(),
    };
    let compass = cur
        .wind_direction
        .map(|d| i18n::compass_word(lang, compass_index(d)));
    let gusts = cur.wind_gusts.map(|g| i18n::home_number(lang, g, 0));
    let chance = w
        .today
        .precipitation_probability
        .map(|p| i18n::home_number(lang, p, 0));
    let uv_sub = cur
        .uv_index
        .map(|uv| i18n::uv_word(lang, uv_level(uv)).to_owned())
        .unwrap_or_default();
    let (sun_value, sun_sub) = match (fallback, sunset) {
        (false, Some(set)) => (
            i18n::home_time(lang, set),
            sunrise
                .map(|r| i18n::sunrise_sub(lang, r))
                .unwrap_or_default(),
        ),
        (true, _) => (
            DASH.to_owned(),
            i18n::detail_text(lang, &Detail::SunsetFallback),
        ),
        _ => (DASH.to_owned(), String::new()),
    };
    let place = [Some(w.place.name.clone()), w.place.admin.clone()]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(", ");
    let days = w.days.iter().map(|d| day_card(lang, d)).collect_view();
    let stale = w
        .stale
        .then(|| fetched.map(|at| i18n::data_from(lang, at)))
        .flatten();

    view! {
        <div class="weather-page__top">
            <span class="weather-page__icon" inner_html=icon></span>
            <div>
                <div class="weather-page__temp">{temp}</div>
                <div class="weather-page__headline">{headline}</div>
            </div>
            <div class="weather-page__side">
                {range.map(|r| view! { <div>{r}</div> })}
                {tonight.map(|r| view! { <div>{r}</div> })}
                <div>{rain_soon}</div>
            </div>
        </div>

        <h3 class="weather-page__label">{weather_text(lang, T::Now)}</h3>
        <div class="weather-tiles">
            {tile(weather_text(lang, T::Wind), wind_value, i18n::wind_sub(lang, compass, gusts.as_deref()))}
            {tile(weather_text(lang, T::Humidity), measure(lang, cur.humidity, 0, Some("%")).into_any(), String::new())}
            {tile(weather_text(lang, T::Pressure), measure(lang, cur.pressure, 0, Some("hPa")).into_any(), String::new())}
            {tile(weather_text(lang, T::Clouds), measure(lang, cur.cloud_cover, 0, Some("%")).into_any(), String::new())}
            {tile(weather_text(lang, T::Rain), measure(lang, w.today.precipitation, 1, Some("mm")).into_any(), i18n::rain_sub(lang, chance.as_deref()))}
            {tile(weather_text(lang, T::MaxGusts), measure(lang, w.today.wind_gusts_max, 0, Some("km/h")).into_any(), weather_text(lang, T::Today).to_lowercase())}
            {tile(weather_text(lang, T::UvIndex), measure(lang, cur.uv_index, 0, None).into_any(), uv_sub)}
            {tile(weather_text(lang, T::Sun), sun_value.into_any(), sun_sub)}
        </div>

        <h3 class="weather-page__label">{weather_text(lang, T::NextDays)}</h3>
        <div class="weather-days">{days}</div>

        <footer class="weather-page__foot">
            <p>{place}{updated.map(|u| view! { " · " {u} })}</p>
            {stale.map(|s| view! { <p>{s}</p> })}
            <p>{weather_text(lang, T::Credit)}</p>
        </footer>
    }
}

/// The weather page; with no weather it says how to turn it on.
#[component]
pub fn WeatherScreen() -> impl IntoView {
    let house: House = expect_context();
    let lang = use_context::<RwSignal<Lang>>().unwrap_or_else(|| RwSignal::new(Lang::En));
    let now = use_context::<Now>().map_or_else(|| RwSignal::new(unix_now()), |n| n.0);
    // Re-renders on a new house-local day, not every minute.
    let midday = Memo::new(move |_| {
        let zone = house.flows_config.with(|c| house_place(c.as_ref()).1);
        let utc = DateTime::<Utc>::from_timestamp(now.get() as i64, 0).unwrap_or_default();
        let date = utc.with_timezone(&zone).date_naive();
        date.and_hms_opt(12, 0, 0)
            .and_then(|noon| zone.from_local_datetime(&noon).earliest())
            .map_or(utc, |t| t.with_timezone(&Utc))
    });
    view! {
        <section class="weather-page">
            <BackLink />
            {move || {
                let l = lang.get();
                match house.weather.get() {
                    Some(w) => content(&w, l, midday.get(), house).into_any(),
                    // Nothing until the first fetch has answered, then the hint.
                    None if !house.weather_loaded.get() => ().into_any(),
                    None => {
                        view! {
                            <div class="weather-page__empty">
                                <p>{weather_text(l, T::Unavailable)}</p>
                                <p class="weather-page__hint">{weather_text(l, T::EnableHint)}</p>
                            </div>
                        }
                            .into_any()
                    }
                }
            }}
        </section>
    }
}
