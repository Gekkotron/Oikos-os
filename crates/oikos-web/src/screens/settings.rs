//! Status of the hub and its links, and the theme switch.

use leptos::prelude::*;
use oikos_api::ConnectionStatus;

use crate::house::House;
use crate::screens::entity_sheet::{availability_class, availability_word};
use crate::theme::Theme;

const DASH: &str = "—";

#[component]
fn Panel(title: &'static str, children: Children) -> impl IntoView {
    view! {
        <section class="sheet__panel">
            <h2 class="sheet__heading">{title}</h2>
            {children()}
        </section>
    }
}

#[component]
fn Row(label: &'static str, #[prop(into)] value: Signal<String>) -> impl IntoView {
    view! {
        <div class="sheet__row">
            <dt>{label}</dt>
            <dd>{value}</dd>
        </div>
    }
}

fn yes_no(value: bool) -> String {
    if value { "Yes" } else { "No" }.to_owned()
}

/// This page's origin, where the hub lives.
fn origin() -> String {
    #[cfg(target_arch = "wasm32")]
    {
        web_sys::window()
            .and_then(|w| w.location().origin().ok())
            .unwrap_or_else(|| DASH.to_owned())
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        "unavailable outside the browser".to_owned()
    }
}

#[component]
fn ThemeSwitch() -> impl IntoView {
    let theme = expect_context::<RwSignal<Theme>>();
    let segment = move |value: Theme, label: &'static str| {
        view! {
            <button
                type="button"
                class="chip segmented__option"
                class:chip--selected=move || theme.get() == value
                aria-pressed=move || (theme.get() == value).to_string()
                on:click=move |_| theme.set(value)
            >
                {label}
            </button>
        }
    };
    view! {
        <div class="segmented" role="group" aria-label="Theme">
            {segment(Theme::Auto, "Auto")} {segment(Theme::Night, "Night")}
            {segment(Theme::Warm, "Warm")}
        </div>
    }
}

#[component]
pub fn SettingsScreen() -> impl IntoView {
    let house = expect_context::<House>();
    let bridge = house.bridge;
    let connection = move || match house.connection.get() {
        ConnectionStatus::Connecting => "Connecting…".to_owned(),
        ConnectionStatus::Online => "Online".to_owned(),
        ConnectionStatus::Offline { .. } => "Offline".to_owned(),
    };
    let reason = move || match house.connection.get() {
        ConnectionStatus::Offline { reason } => Some(reason),
        _ => None,
    };
    let primary = move || {
        house
            .areas
            .with(|areas| areas.iter().map(|a| a.primary_count).sum::<usize>())
    };

    view! {
        <div class="sheet">
            <h1 class="settings__title">"Settings"</h1>
            <Panel title="Broker connection">
                <dl class="sheet__rows">
                    <Row label="Status" value=connection />
                    {move || {
                        reason()
                            .map(|r| {
                                view! { <Row label="Reason" value=r /> }
                            })
                    }}
                </dl>
            </Panel>
            <Panel title="Zigbee2MQTT bridge">
                <dl class="sheet__rows">
                    <Row
                        label="Version"
                        value=move || bridge.with(|b| b.version.clone().unwrap_or_else(|| DASH.to_owned()))
                    />
                    <Row
                        label="Coordinator"
                        value=move || {
                            bridge.with(|b| b.coordinator.clone().unwrap_or_else(|| DASH.to_owned()))
                        }
                    />
                    <Row label="Permit join" value=move || yes_no(bridge.with(|b| b.permit_join)) />
                    <div class="sheet__row">
                        <dt>"State"</dt>
                        <dd>
                            <span class="settings__state">
                                <span class=move || availability_class(bridge.with(|b| b.state))></span>
                                {move || availability_word(bridge.with(|b| b.state))}
                            </span>
                        </dd>
                    </div>
                </dl>
            </Panel>
            <Panel title="Counts">
                <dl class="sheet__rows">
                    <Row label="Devices" value=move || house.devices.with(Vec::len).to_string() />
                    <Row label="Entities" value=move || house.entities.with(|m| m.len()).to_string() />
                    <Row label="Primary entities" value=move || primary().to_string() />
                    <Row label="Areas" value=move || house.areas.with(Vec::len).to_string() />
                    <Row label="Ignored topics" value=move || house.ignored.with(Vec::len).to_string() />
                </dl>
            </Panel>
            <Panel title="Theme">
                <ThemeSwitch />
            </Panel>
            <Panel title="About">
                <dl class="sheet__rows">
                    <Row label="Product" value="Oikos" />
                    <Row label="Version" value=concat!("v", env!("CARGO_PKG_VERSION")) />
                    <Row label="Hub: this page's origin" value=origin() />
                </dl>
            </Panel>
        </div>
    }
}
