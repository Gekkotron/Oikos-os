//! The Alerts page: channel chips on top, the alerts grouped by day below.

use leptos::prelude::*;
use oikos_api::AlertChannel;

use crate::alerts::{DayLabel, filtered, group_by_day};
use crate::components::alert_row::AlertRow;
use crate::components::now::Now;
use crate::house::House;
use crate::icons::channel_icon;
use crate::nav::BackLink;

/// The heading of a day group: "Today", "Yesterday" or the date in the browser's locale.
pub fn day_title(label: &DayLabel) -> String {
    match label {
        DayLabel::Today => "Today".into(),
        DayLabel::Yesterday => "Yesterday".into(),
        DayLabel::On { year, month, day } => locale_day(*year, *month, *day),
    }
}

#[cfg(target_arch = "wasm32")]
fn locale_day(year: i32, month: u8, day: u8) -> String {
    use js_sys::{Date, Object, Reflect};
    use wasm_bindgen::JsValue;

    let options = Object::new();
    for (key, value) in [("month", "short"), ("day", "numeric"), ("year", "numeric")] {
        let _ = Reflect::set(&options, &JsValue::from_str(key), &JsValue::from_str(value));
    }
    let date = Date::new_with_year_month_day(year as u32, i32::from(month) - 1, i32::from(day));
    date.to_locale_date_string("default", &options).into()
}

#[cfg(not(target_arch = "wasm32"))]
fn locale_day(year: i32, month: u8, day: u8) -> String {
    format!("{day}/{month}/{year}")
}

/// The browser's `Date.getTimezoneOffset()` in minutes; 0 off the browser.
#[cfg(target_arch = "wasm32")]
fn tz_offset_min() -> i32 {
    js_sys::Date::new_0().get_timezone_offset() as i32
}

#[cfg(not(target_arch = "wasm32"))]
fn tz_offset_min() -> i32 {
    0
}

#[component]
pub fn AlertsScreen() -> impl IntoView {
    let house = expect_context::<House>();
    let now = expect_context::<Now>();
    let selected = RwSignal::new(None::<AlertChannel>);
    let shown = Memo::new(move |_| house.alerts.with(|alerts| filtered(alerts, selected.get())));
    let groups =
        Memo::new(move |_| shown.with(|alerts| group_by_day(alerts, now.0.get(), tz_offset_min())));

    let all_chip = view! {
        <button
            type="button"
            class="chip"
            class:chip--selected=move || selected.get().is_none()
            on:click=move |_| selected.set(None)
        >
            "All"
        </button>
    };
    let channel_chips = AlertChannel::ALL
        .into_iter()
        .map(|channel| {
            view! {
                <button
                    type="button"
                    class="chip"
                    class:chip--selected=move || selected.get() == Some(channel)
                    on:click=move |_| selected.set(Some(channel))
                >
                    <span class="chip__glyph" inner_html=channel_icon(channel)></span>
                    {channel.name()}
                </button>
            }
        })
        .collect_view();

    view! {
        <div class="alerts">
            <header class="sheet__header">
                <BackLink />
                <div class="sheet__titles">
                    <h1 class="sheet__title">"Alerts"</h1>
                </div>
            </header>
            <nav class="chips" aria-label="Channels">{all_chip} {channel_chips}</nav>
            <Show
                when=move || shown.with(|a| !a.is_empty())
                fallback=move || {
                    view! {
                        <p class="alerts__empty">
                            {move || {
                                if selected.get().is_none() {
                                    "No alerts yet"
                                } else {
                                    "No alerts in this channel"
                                }
                            }}
                        </p>
                    }
                }
            >
                <For
                    each=move || groups.get()
                    key=|(label, list)| {
                        (
                            label.clone(),
                            list.iter()
                                .map(|a| (a.id.clone(), a.title.clone(), a.body.clone(), a.image_url.clone(), a.resolved, a.sent_at))
                                .collect::<Vec<_>>(),
                        )
                    }
                    children=|(label, list)| {
                        view! {
                            <h2 class="alerts__day">{day_title(&label)}</h2>
                            <div class="alerts__list">
                                {list
                                    .into_iter()
                                    .map(|alert| view! { <AlertRow alert=alert /> })
                                    .collect_view()}
                            </div>
                        }
                    }
                />
            </Show>
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::day_title;
    use crate::alerts::DayLabel;

    #[test]
    fn day_title_words_and_numeric_fallback() {
        assert_eq!(day_title(&DayLabel::Today), "Today");
        assert_eq!(day_title(&DayLabel::Yesterday), "Yesterday");
        assert_eq!(
            day_title(&DayLabel::On {
                year: 2026,
                month: 10,
                day: 5
            }),
            "5/10/2026"
        );
    }
}
