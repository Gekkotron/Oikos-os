//! Status of the hub and its links, how to connect an AI assistant over MCP, and the theme switch.

use leptos::prelude::*;
#[cfg(target_arch = "wasm32")]
use leptos::task::spawn_local;
use leptos_router::components::A;
use oikos_api::{ConnectionStatus, DeviceView, UpdateView};

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

fn claude_code_snippet(origin: &str) -> String {
    format!("claude mcp add --transport http oikos {origin}/mcp")
}

fn codex_snippet(origin: &str) -> String {
    format!("[mcp_servers.oikos]\nurl = \"{origin}/mcp\"")
}

fn claude_desktop_snippet(origin: &str) -> String {
    // Written by hand so the keys keep the README's order (`command` before `args`).
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

    let origin = origin();
    let endpoint = format!("{origin}/mcp");
    let claude_code = claude_code_snippet(&origin);
    let codex = codex_snippet(&origin);
    let claude_desktop = claude_desktop_snippet(&origin);

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
            <Panel title="Updates available">
                {move || {
                    let pending: Vec<(DeviceView, UpdateView)> = house
                        .devices
                        .with(|ds| {
                            ds.iter()
                                .filter_map(|d| {
                                    let u = d.update.clone().filter(|u| u.available)?;
                                    Some((d.clone(), u))
                                })
                                .collect()
                        });
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
            <Panel title="MCP">
                <dl class="sheet__rows">
                    <Row label="Endpoint" value=endpoint />
                    <Row label="Transport" value="Streamable HTTP, no auth (LAN only)" />
                </dl>
                <Snippet title="Claude Code" text=claude_code />
                <Snippet title="Codex CLI (~/.codex/config.toml)" text=codex />
                <Snippet title="Claude Desktop (stdio proxy)" text=claude_desktop />
            </Panel>
            <Panel title="Diagnostics">
                <p class="settings__hint">
                    "Zigbee2MQTT devices, firmware updates and ignored discovery topics."
                </p>
                <A href="/devices" attr:class="chip">
                    "Open Z2M devices"
                </A>
            </Panel>
            <Panel title="Theme">
                <ThemeSwitch />
            </Panel>
            <Panel title="About">
                <dl class="sheet__rows">
                    <Row label="Product" value="Oikos-os" />
                    <Row label="Version" value=concat!("v", env!("CARGO_PKG_VERSION")) />
                    <Row label="Hub: this page's origin" value=origin />
                </dl>
            </Panel>
        </div>
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
}
