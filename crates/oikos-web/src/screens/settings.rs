//! Settings: a short list of sub-pages (the home header settings, appearance, the Zigbee2MQTT bridge, the broker, how to
//! connect an AI assistant over MCP, about), each opened at `/settings/<slug>`.

use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::components::A;
use leptos_router::hooks::use_params_map;
use oikos_api::{ConnectionStatus, DeviceView, UpdateView};
use oikos_core::model::Availability;

use crate::house::House;
use crate::i18n::{Key, Lang, text, use_lang};
use crate::nav::BackLink;
use crate::screens::alarm_settings::{AlarmSettingsPage, settings_status};
use crate::screens::entity_sheet::{availability_class, availability_word};
use crate::screens::home_settings_panel::HomeSettingsPanel;
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

fn claude_code_snippet(origin: &str) -> String {
    format!("claude mcp add --transport http oikos {origin}/mcp")
}

fn codex_snippet(origin: &str) -> String {
    format!("[mcp_servers.oikos]\nurl = \"{origin}/mcp\"")
}

fn claude_desktop_snippet(origin: &str) -> String {
    // Written by hand so the keys keep the order of docs/users/assistants-mcp.md (`command` before `args`).
    format!(
        "{{\n  \"mcpServers\": {{\n    \"oikos\": {{\n      \"command\": \"<path>/oikos-os\",\n      \"args\": [\"mcp-stdio\", \"--server\", {}]\n    }}\n  }}\n}}",
        serde_json::Value::String(origin.to_owned())
    )
}

/// Writes `text` to the clipboard; false when the clipboard API is unavailable or refuses.
#[cfg(target_arch = "wasm32")]
async fn write_clipboard(text: &str) -> bool {
    let Some(window) = web_sys::window() else {
        return false;
    };
    let navigator = window.navigator();
    let available = js_sys::Reflect::get(&navigator, &"clipboard".into())
        .map(|c| !c.is_undefined() && !c.is_null())
        .unwrap_or(false);
    if !available {
        return false;
    }
    let promise = navigator.clipboard().write_text(text);
    wasm_bindgen_futures::JsFuture::from(promise).await.is_ok()
}

#[component]
fn CopyButton(text: String) -> impl IntoView {
    let label = RwSignal::new("Copy");
    let on_click = move |_| {
        #[cfg(target_arch = "wasm32")]
        {
            let text = text.clone();
            spawn_local(async move {
                if write_clipboard(&text).await {
                    label.set("Copied");
                    gloo_timers::future::TimeoutFuture::new(1500).await;
                    label.set("Copy");
                }
            });
        }
        #[cfg(not(target_arch = "wasm32"))]
        let _ = &text;
    };
    view! {
        <button type="button" class="chip sheet__copy" on:click=on_click>
            {move || label.get()}
        </button>
    }
}

#[component]
fn Snippet(title: &'static str, text: String) -> impl IntoView {
    view! {
        <h3 class="sheet__subheading">{title}</h3>
        <pre class="sheet__json">{text.clone()}</pre>
        <CopyButton text=text />
    }
}

#[component]
fn ThemeSwitch() -> impl IntoView {
    let theme = expect_context::<RwSignal<Theme>>();
    let segment = move |value: Theme| {
        view! {
            <button
                type="button"
                class="chip segmented__option"
                class:chip--selected=move || theme.get() == value
                aria-pressed=move || (theme.get() == value).to_string()
                on:click=move |_| theme.set(value)
            >
                {value.label()}
            </button>
        }
    };
    view! {
        <div class="segmented" role="group" aria-label="Theme">
            {segment(Theme::Auto)} {segment(Theme::Night)} {segment(Theme::Ivoire)}
        </div>
    }
}

/// A Settings sub-page, at `/settings/<slug>`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SettingsPage {
    Home,
    Alarm,
    Appearance,
    Zigbee2mqtt,
    Broker,
    Mcp,
    About,
}

impl SettingsPage {
    /// In the order the Settings list shows them.
    pub const ALL: [SettingsPage; 7] = [
        SettingsPage::Home,
        SettingsPage::Alarm,
        SettingsPage::Appearance,
        SettingsPage::Zigbee2mqtt,
        SettingsPage::Broker,
        SettingsPage::Mcp,
        SettingsPage::About,
    ];

    pub fn slug(self) -> &'static str {
        match self {
            SettingsPage::Home => "home",
            SettingsPage::Alarm => "alarm",
            SettingsPage::Appearance => "appearance",
            SettingsPage::Zigbee2mqtt => "zigbee2mqtt",
            SettingsPage::Broker => "broker",
            SettingsPage::Mcp => "mcp",
            SettingsPage::About => "about",
        }
    }

    pub fn from_slug(slug: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|p| p.slug() == slug)
    }

    /// The page's name, translated in the breadcrumb; the screen itself shows the English one.
    pub fn key(self) -> Key {
        match self {
            SettingsPage::Home => Key::HomeSection,
            SettingsPage::Alarm => Key::Alarm,
            SettingsPage::Appearance => Key::Appearance,
            SettingsPage::Zigbee2mqtt => Key::Zigbee2mqtt,
            SettingsPage::Broker => Key::Broker,
            SettingsPage::Mcp => Key::AiAssistants,
            SettingsPage::About => Key::About,
        }
    }

    fn title(self) -> &'static str {
        text(Lang::En, self.key())
    }

    fn hint(self) -> &'static str {
        match self {
            SettingsPage::Home => "Eco setpoint, house mode, indoor temperature, off-peak hours",
            SettingsPage::Alarm => "Sensors, sirens, remotes and presence",
            SettingsPage::Appearance => "Theme",
            SettingsPage::Zigbee2mqtt => "Bridge, firmware updates and devices",
            SettingsPage::Broker => "MQTT connection",
            SettingsPage::Mcp => "Connect Claude, Codex and other assistants",
            SettingsPage::About => "Version and hub address",
        }
    }

    fn href(self) -> String {
        format!("/settings/{}", self.slug())
    }
}

const VERSION: &str = concat!("v", env!("CARGO_PKG_VERSION"));

fn connection_word(status: &ConnectionStatus) -> String {
    match status {
        ConnectionStatus::Connecting => "Connecting…",
        ConnectionStatus::Online => "Online",
        ConnectionStatus::Offline { .. } => "Offline",
    }
    .to_owned()
}

/// The devices with a firmware update waiting, with that update.
fn pending_updates(devices: &[DeviceView]) -> Vec<(DeviceView, UpdateView)> {
    devices
        .iter()
        .filter_map(|d| {
            let u = d.update.clone().filter(|u| u.available)?;
            Some((d.clone(), u))
        })
        .collect()
}

/// The Zigbee2MQTT row's status: the waiting updates when there are any, else the bridge state.
fn zigbee_status(updates: usize, state: Availability) -> String {
    match updates {
        0 => availability_word(state).to_owned(),
        1 => "1 update".to_owned(),
        n => format!("{n} updates"),
    }
}

/// The Settings list: one row per sub-page, with its status on the right.
#[component]
pub fn SettingsScreen() -> impl IntoView {
    let house = expect_context::<House>();
    let theme = expect_context::<RwSignal<Theme>>();
    let lang = use_lang();
    let alarm = RwSignal::new(None::<oikos_alarm::view::AlarmView>);
    Effect::new(move |_| {
        house.alarm_rev.track();
        spawn_local(async move {
            if let Ok(v) = crate::api::alarm::get_alarm().await {
                alarm.try_set(Some(v));
            }
        });
    });
    let status = move |page: SettingsPage| match page {
        SettingsPage::Home => String::new(),
        SettingsPage::Alarm => alarm
            .get()
            .map(|v| settings_status(&v, lang()))
            .unwrap_or_default(),
        SettingsPage::Appearance => theme.get().label().to_owned(),
        SettingsPage::Zigbee2mqtt => zigbee_status(
            house.devices.with(|ds| pending_updates(ds).len()),
            house.bridge.with(|b| b.state),
        ),
        SettingsPage::Broker => house.connection.with(connection_word),
        SettingsPage::Mcp => String::new(),
        SettingsPage::About => VERSION.to_owned(),
    };

    view! {
        <div class="sheet">
            <h1 class="settings__title">"Settings"</h1>
            <ul class="settings__list">
                {SettingsPage::ALL
                    .into_iter()
                    .map(|page| {
                        view! {
                            <li>
                                <A href=page.href() attr:class="settings__item">
                                    <span class="settings__item-text">
                                        <span class="settings__item-title">{page.title()}</span>
                                        <span class="settings__item-hint">{page.hint()}</span>
                                    </span>
                                    <span class="settings__item-status">{move || status(page)}</span>
                                    <span class="settings__item-chevron" aria-hidden="true">
                                        "›"
                                    </span>
                                </A>
                            </li>
                        }
                    })
                    .collect_view()}
            </ul>
        </div>
    }
}

/// A Settings sub-page (`/settings/:page`), with a Back link to where it was opened from.
#[component]
pub fn SettingsPageScreen() -> impl IntoView {
    let params = use_params_map();
    let page = Memo::new(move |_| {
        params
            .read()
            .get("page")
            .as_deref()
            .and_then(SettingsPage::from_slug)
    });
    view! {
        <div class="sheet">
            <header class="sheet__header">
                <BackLink />
                <div class="sheet__titles">
                    <h1 class="sheet__title">
                        {move || page.get().map_or("Settings", SettingsPage::title)}
                    </h1>
                </div>
            </header>
            {move || match page.get() {
                Some(SettingsPage::Home) => view! { <HomeSettingsPanel /> }.into_any(),
                Some(SettingsPage::Alarm) => view! { <AlarmSettingsPage /> }.into_any(),
                Some(SettingsPage::Appearance) => view! { <AppearancePage /> }.into_any(),
                Some(SettingsPage::Zigbee2mqtt) => view! { <ZigbeePage /> }.into_any(),
                Some(SettingsPage::Broker) => view! { <BrokerPage /> }.into_any(),
                Some(SettingsPage::Mcp) => view! { <McpPage /> }.into_any(),
                Some(SettingsPage::About) => view! { <AboutPage /> }.into_any(),
                None => {
                    view! {
                        <p class="settings__empty">"There is no such settings page."</p>
                        <A href="/settings" attr:class="chip settings__all">
                            "All settings"
                        </A>
                    }
                        .into_any()
                }
            }}
        </div>
    }
}

#[component]
fn AppearancePage() -> impl IntoView {
    view! {
        <Panel title="Theme">
            <ThemeSwitch />
            <p class="settings__hint">"The theme button in the header switches it too."</p>
        </Panel>
    }
}

/// The Zigbee2MQTT sub-page: the bridge, the firmware updates waiting, the counts and the way
/// to the devices list.
#[component]
pub fn ZigbeePage() -> impl IntoView {
    let house = expect_context::<House>();
    let bridge = house.bridge;
    let primary = move || {
        house
            .areas
            .with(|areas| areas.iter().map(|a| a.primary_count).sum::<usize>())
    };
    view! {
        <Panel title="Bridge">
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
        <Panel title="Updates available">
            {move || {
                let pending = house.devices.with(|ds| pending_updates(ds));
                if pending.is_empty() {
                    view! { <p class="settings__empty">"All devices are up to date"</p> }
                        .into_any()
                } else {
                    view! {
                        <ul class="settings__updates">
                            {pending
                                .into_iter()
                                .map(|(d, u)| {
                                    view! {
                                        <li>
                                            <A href="/devices">{d.friendly_name.clone()}</A>
                                            <span>
                                                {format!(
                                                    "{} → {}",
                                                    u.installed.unwrap_or_default(),
                                                    u.latest.unwrap_or_default(),
                                                )}
                                            </span>
                                        </li>
                                    }
                                })
                                .collect_view()}
                        </ul>
                    }
                        .into_any()
                }
            }}
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
        <Panel title="Devices">
            <p class="settings__hint">
                "Zigbee2MQTT devices, firmware updates and ignored discovery topics."
            </p>
            <A href="/devices" attr:class="chip">
                "Open Z2M devices"
            </A>
        </Panel>
    }
}

#[component]
fn BrokerPage() -> impl IntoView {
    let house = expect_context::<House>();
    let reason = move || match house.connection.get() {
        ConnectionStatus::Offline { reason } => Some(reason),
        _ => None,
    };
    view! {
        <Panel title="Connection">
            <dl class="sheet__rows">
                <Row label="Status" value=move || house.connection.with(connection_word) />
                {move || {
                    reason()
                        .map(|r| {
                            view! { <Row label="Reason" value=r /> }
                        })
                }}
            </dl>
        </Panel>
    }
}

#[component]
fn McpPage() -> impl IntoView {
    let origin = origin();
    let endpoint = format!("{origin}/mcp");
    view! {
        <Panel title="Endpoint">
            <dl class="sheet__rows">
                <Row label="Endpoint" value=endpoint />
                <Row label="Transport" value="Streamable HTTP, no auth (LAN only)" />
            </dl>
        </Panel>
        <Panel title="Set up an assistant">
            <Snippet title="Claude Code" text=claude_code_snippet(&origin) />
            <Snippet title="Codex CLI (~/.codex/config.toml)" text=codex_snippet(&origin) />
            <Snippet title="Claude Desktop (stdio proxy)" text=claude_desktop_snippet(&origin) />
        </Panel>
    }
}

#[component]
fn AboutPage() -> impl IntoView {
    view! {
        <Panel title="Hub">
            <dl class="sheet__rows">
                <Row label="Product" value="Oikos-os" />
                <Row label="Version" value=VERSION />
                <Row label="Hub: this page's origin" value=origin() />
            </dl>
        </Panel>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ORIGIN: &str = "http://192.168.1.10:8081";

    #[test]
    fn claude_code_snippet_has_endpoint() {
        let s = claude_code_snippet(ORIGIN);
        assert_eq!(
            s,
            "claude mcp add --transport http oikos http://192.168.1.10:8081/mcp"
        );
    }

    #[test]
    fn codex_snippet_has_endpoint() {
        let s = codex_snippet(ORIGIN);
        assert!(s.starts_with("[mcp_servers.oikos]\n"));
        assert!(s.contains("url = \"http://192.168.1.10:8081/mcp\""));
    }

    #[test]
    fn claude_desktop_snippet_is_json_with_origin() {
        let s = claude_desktop_snippet(ORIGIN);
        assert!(s.lines().count() > 1);
        let v: serde_json::Value = serde_json::from_str(&s).unwrap();
        assert_eq!(v["mcpServers"]["oikos"]["args"][2], ORIGIN);
        assert_eq!(v["mcpServers"]["oikos"]["command"], "<path>/oikos-os");
    }

    #[test]
    fn every_page_round_trips_its_slug() {
        for page in SettingsPage::ALL {
            assert_eq!(SettingsPage::from_slug(page.slug()), Some(page));
            assert_eq!(page.href(), format!("/settings/{}", page.slug()));
        }
        assert_eq!(SettingsPage::from_slug("nope"), None);
        assert_eq!(SettingsPage::Zigbee2mqtt.title(), "Zigbee2MQTT");
    }

    #[test]
    fn zigbee_status_prefers_updates_over_the_bridge_state() {
        assert_eq!(zigbee_status(0, Availability::Online), "Online");
        assert_eq!(zigbee_status(0, Availability::Offline), "Offline");
        assert_eq!(zigbee_status(1, Availability::Online), "1 update");
        assert_eq!(zigbee_status(3, Availability::Offline), "3 updates");
    }
}
