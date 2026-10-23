//! The detail of one alert: media, text, actions, a link to the device and the source line.

use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::components::A;
use leptos_router::hooks::use_params_map;
use oikos_api::{AlertAction, AlertView};

use crate::alerts::match_entity;
use crate::api::send_alert_action;
use crate::components::toast::Toasts;
use crate::house::House;
use crate::icons::channel_icon;
use crate::nav::BackLink;

/// The most action buttons shown.
const MAX_ACTIONS: usize = 3;

/// The full date and time of `sent_at` in the browser's locale.
#[cfg(target_arch = "wasm32")]
pub fn full_date(sent_at: u64) -> String {
    use js_sys::{Date, Object, Reflect};
    use wasm_bindgen::JsValue;

    let options = Object::new();
    for (key, value) in [("dateStyle", "medium"), ("timeStyle", "short")] {
        let _ = Reflect::set(&options, &JsValue::from_str(key), &JsValue::from_str(value));
    }
    Date::new(&JsValue::from_f64(sent_at as f64 * 1000.0))
        .to_locale_string("default", &options)
        .into()
}

/// The full date and time of `sent_at` as ISO 8601 UTC off the browser.
#[cfg(not(target_arch = "wasm32"))]
pub fn full_date(sent_at: u64) -> String {
    crate::format::format_timestamp(sent_at)
}

/// The toast shown when an action could not be sent.
fn failure_text(label: &str) -> String {
    format!("Couldn't send {label}")
}

/// The source line under the alert.
fn source_line(source: &str, sent_at: u64) -> String {
    format!("From {source} · {}", full_date(sent_at))
}

/// The actions that get a button.
fn shown_actions(actions: &[AlertAction]) -> Vec<AlertAction> {
    actions.iter().take(MAX_ACTIONS).cloned().collect()
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Media {
    Image,
    Playing,
    Failed,
}

#[component]
fn MediaBlock(image_url: Option<String>, video_url: Option<String>) -> impl IntoView {
    let media = RwSignal::new(Media::Image);
    let has_video = video_url.is_some();
    view! {
        <div class="alert-media">
            {move || match media.get() {
                Media::Failed => {
                    view! { <p class="alert-media__message">"Media no longer available"</p> }
                        .into_any()
                }
                Media::Playing => {
                    view! {
                        <video
                            controls
                            autoplay
                            playsinline
                            src=video_url.clone()
                            on:error=move |_| media.set(Media::Failed)
                        ></video>
                    }
                        .into_any()
                }
                Media::Image => {
                    view! {
                        {image_url
                            .clone()
                            .map(|src| {
                                view! {
                                    <img src=src alt="" on:error=move |_| media.set(Media::Failed) />
                                }
                            })}
                        {has_video
                            .then(|| {
                                view! {
                                    <button
                                        type="button"
                                        class="btn btn--round alert-media__play"
                                        aria-label="Play clip"
                                        title="Play"
                                        on:click=move |_| media.set(Media::Playing)
                                    >
                                        "▶"
                                    </button>
                                }
                            })}
                    }
                        .into_any()
                }
            }}
        </div>
    }
}

#[component]
fn ActionButton(alert_id: String, action: AlertAction) -> impl IntoView {
    let toasts = expect_context::<Toasts>();
    let label = action.label.clone();
    let on_click = move |_| {
        let id = alert_id.clone();
        let action_id = action.id.clone();
        let label = action.label.clone();
        spawn_local(async move {
            match send_alert_action(&id, &action_id).await {
                Ok(()) => toasts.push("Sent"),
                Err(_) => toasts.push(failure_text(&label)),
            }
        });
    };
    view! {
        <button type="button" class="btn" on:click=on_click>
            {label}
        </button>
    }
}

fn back_header(title: Option<String>) -> impl IntoView {
    view! {
        <header class="sheet__header">
            <BackLink />
            {title.map(|t| view! { <div class="sheet__titles"><h1 class="sheet__title">{t}</h1></div> })}
        </header>
    }
}

#[component]
fn DetailBody(alert: AlertView) -> impl IntoView {
    let house = expect_context::<House>();
    let device = house
        .entities
        .with_untracked(|entities| match_entity(alert.entity_id.as_deref(), entities));
    let has_media = alert.image_url.is_some() || alert.video_url.is_some();
    let date = full_date(alert.sent_at);
    let source = alert
        .source
        .as_deref()
        .map(|s| source_line(s, alert.sent_at));
    let actions = shown_actions(&alert.actions);
    let id = alert.id.clone();
    view! {
        {back_header(None)}
        {has_media
            .then(|| {
                view! { <MediaBlock image_url=alert.image_url.clone() video_url=alert.video_url.clone() /> }
            })}
        <h1 class="sheet__title alert-detail__title">{alert.title.clone()}</h1>
        <p class="alert-detail__body">{alert.body.clone()}</p>
        <div class="alert-detail__badges">
            <span class="badge alert-detail__channel">
                <span class="alert-detail__glyph" inner_html=channel_icon(alert.channel)></span>
                {alert.channel.name()}
            </span>
            {alert.resolved.then(|| view! { <span class="badge">"Resolved"</span> })}
        </div>
        <p class="sheet__updated">{date}</p>
        <div class="alert-detail__actions">
            {actions
                .into_iter()
                .map(|action| view! { <ActionButton alert_id=id.clone() action=action /> })
                .collect_view()}
            {device
                .map(|unique_id| {
                    view! {
                        <A href=format!("/entity/{}", crate::api::encode_id(&unique_id)) attr:class="btn alert-detail__device">
                            "Open device"
                        </A>
                    }
                })}
        </div>
        {source.map(|line| view! { <p class="alert-detail__source">{line}</p> })}
    }
}

#[component]
pub fn AlertDetailScreen() -> impl IntoView {
    let house = expect_context::<House>();
    let params = use_params_map();
    let id = Memo::new(move |_| params.read().get("id").unwrap_or_default());
    let alert = Memo::new(move |_| {
        let wanted = id.get();
        house
            .alerts
            .with(|a| a.iter().find(|a| a.id == wanted).cloned())
    });
    // Before the snapshot arrives the list is empty for a moment; a deep link or a reload must
    // not flash "no longer available" then (same guard as the entity sheet).
    let loaded = Memo::new(move |_| {
        house.alerts.with(|a| !a.is_empty())
            || matches!(house.connection.get(), oikos_api::ConnectionStatus::Online)
    });
    view! {
        <div class="sheet alert-detail">
            {move || match alert.get() {
                Some(alert) => view! { <DetailBody alert=alert /> }.into_any(),
                None if loaded.get() => {
                    view! {
                        {back_header(None)}
                        <p class="alerts__empty">"This alert is no longer available"</p>
                    }
                        .into_any()
                }
                None => {
                    view! {
                        {back_header(None)}
                        <div class="alert-media alert-media--skeleton" aria-busy="true"></div>
                    }
                        .into_any()
                }
            }}
        </div>
    }
}

#[cfg(test)]
mod tests {
    use oikos_api::AlertAction;

    use super::{failure_text, full_date, shown_actions, source_line};

    #[test]
    fn full_date_falls_back_to_iso() {
        assert_eq!(full_date(1_792_402_860), "2026-10-19T09:41:00Z");
    }

    #[test]
    fn source_line_joins_source_and_date() {
        assert_eq!(
            source_line("frigate", 1_792_402_860),
            "From frigate · 2026-10-19T09:41:00Z"
        );
    }

    #[test]
    fn failure_text_names_the_action() {
        assert_eq!(failure_text("Disarm"), "Couldn't send Disarm");
    }

    #[test]
    fn at_most_three_actions_are_shown() {
        let actions: Vec<AlertAction> = (0..5)
            .map(|i| AlertAction {
                id: format!("a{i}"),
                label: format!("A{i}"),
            })
            .collect();
        let shown = shown_actions(&actions);
        assert_eq!(shown.len(), 3);
        assert_eq!(shown[2].id, "a2");
    }
}
