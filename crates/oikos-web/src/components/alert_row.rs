//! One alert as a row, and the Home screen's "Recent alerts" section built from them.

use leptos::prelude::*;
use leptos_router::components::A;
use oikos_api::AlertView;

use crate::alerts::{recent, relative_time};
use crate::components::now::Now;
use crate::house::House;
use crate::icons::channel_icon;

/// The age of `alert` at `now`, with " · Resolved" after it when the producer cancelled it.
fn time_line(alert: &AlertView, now: u64) -> String {
    let age = relative_time(alert.sent_at, now);
    if alert.resolved {
        format!("{age} · Resolved")
    } else {
        age
    }
}

#[component]
pub fn AlertRow(alert: AlertView) -> impl IntoView {
    let now = expect_context::<Now>();
    let href = format!("/alerts/{}", alert.id);
    // A picture that fails to load (gone, or refused) falls back to the channel glyph instead
    // of the browser's broken-image icon.
    let broken = RwSignal::new(false);
    let channel = alert.channel;
    let image_url = alert.image_url.clone();
    let thumb = move || match image_url.clone() {
        Some(src) if !broken.get() => view! {
            <img
                class="alert-row__thumb"
                src=src
                loading="lazy"
                alt=""
                on:error=move |_| broken.set(true)
            />
        }
        .into_any(),
        _ => view! {
            <span
                class="alert-row__thumb alert-row__thumb--glyph"
                inner_html=channel_icon(channel)
            ></span>
        }
        .into_any(),
    };
    let time = {
        let alert = alert.clone();
        move || time_line(&alert, now.0.get())
    };
    view! {
        <A href=href attr:class="alert-row">
            {thumb}
            <span class="alert-row__text">
                <span class="alert-row__title">{alert.title}</span>
                <span class="alert-row__body">{alert.body}</span>
                <span class="alert-row__time">{time}</span>
            </span>
        </A>
    }
}

/// The Home section: up to five recent alarm, security and camera alerts, newest first.
#[component]
pub fn RecentAlerts() -> impl IntoView {
    let house = expect_context::<House>();
    let alerts = Memo::new(move |_| house.alerts.with(|a| recent(a)));
    view! {
        <section class="alerts-recent">
            <header class="alerts-recent__header">
                <h2 class="alerts-recent__title">"Recent alerts"</h2>
                <A href="/alerts" attr:class="btn btn--compact">"See all"</A>
            </header>
            <Show
                when=move || alerts.with(|a| !a.is_empty())
                fallback=|| view! { <p class="alerts-recent__empty">"No alerts yet"</p> }
            >
                <div class="alerts-recent__list">
                    <For
                        each=move || alerts.get()
                        key=|a| (a.id.clone(), a.title.clone(), a.body.clone(), a.image_url.clone(), a.resolved, a.sent_at)
                        children=|alert| view! { <AlertRow alert=alert /> }
                    />
                </div>
            </Show>
        </section>
    }
}

#[cfg(test)]
mod tests {
    use oikos_api::{AlertChannel, AlertSeverity, AlertView};

    use super::time_line;

    fn alert(resolved: bool) -> AlertView {
        AlertView {
            id: "a".into(),
            channel: AlertChannel::Camera,
            severity: AlertSeverity::Info,
            title: "Front Door".into(),
            body: String::new(),
            image_url: None,
            video_url: None,
            entity_id: None,
            actions: vec![],
            source: None,
            silent: false,
            resolved,
            sent_at: 1_000,
        }
    }

    #[test]
    fn time_line_adds_resolved_after_the_time() {
        assert_eq!(time_line(&alert(false), 1_120), "2 min ago");
        assert_eq!(time_line(&alert(true), 1_120), "2 min ago · Resolved");
        assert_eq!(time_line(&alert(true), 1_010), "Just now · Resolved");
    }
}
