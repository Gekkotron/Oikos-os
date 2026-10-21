//! The shell: top bar (name, connection pill, theme toggle, navigation) around the router.

use leptos::prelude::*;
use leptos_router::components::{A, Route, Router, Routes};
use leptos_router::path;
use oikos_core::model::{Kind, StateValue};

use crate::commands::Pending;
use crate::components::confirm::Confirming;
use crate::components::now::install_now;
use crate::components::status_pill::StatusPill;
use crate::components::toast::{ToastQueue, Toasts};
use crate::house::House;
use crate::icons::{BOLT, SHIELD};
use crate::nav::{TrackReturnPath, install_nav};
use crate::screens::alert_detail::AlertDetailScreen;
use crate::screens::alerts::AlertsScreen;
use crate::screens::devices::DevicesScreen;
use crate::screens::energy::{EnergyScreen, install_history_refresh};
use crate::screens::entity_sheet::EntitySheetScreen;
use crate::screens::flow_canvas::{FlowCanvasScreen, NewFlowScreen};
use crate::screens::flows::FlowsScreen;
use crate::screens::group_sheet::GroupSheetScreen;
use crate::screens::history_detail::HistoryDetailScreen;
use crate::screens::home::HomeScreen;
use crate::screens::security::SecurityScreen;
use crate::screens::settings::SettingsScreen;
use crate::theme::{Theme, install_theme};

/// True while any alarm panel is other than disarmed: armed, arming, pending or triggered. The
/// Security tab glyph takes the warning colour then, as the app's tab icon does.
pub(crate) fn any_alarm_armed<'a>(
    mut entities: impl Iterator<Item = &'a oikos_api::EntityView>,
) -> bool {
    entities.any(|e| {
        e.kind == Kind::AlarmPanel
            && matches!(&e.state.value, StateValue::Alarm(s) if !matches!(s.as_str(), "disarmed" | "disarming" | "unknown" | ""))
    })
}

/// The next theme in the Auto, Night, Warm cycle.
fn next_theme(theme: Theme) -> Theme {
    match theme {
        Theme::Auto => Theme::Night,
        Theme::Night => Theme::Warm,
        Theme::Warm => Theme::Auto,
    }
}

#[component]
pub fn App() -> impl IntoView {
    let house = House::new();
    provide_context(house);
    let toasts = Toasts::new();
    provide_context(toasts);
    provide_context(Pending::new());
    provide_context(Confirming::new());
    provide_context(install_now());
    install_nav();
    install_history_refresh(house);
    // A command the hub gave up on: tell the user, then clear it so the next one fires again.
    Effect::new(move |_| {
        if let Some((id, reason)) = house.command_failed.get() {
            let name = house
                .entities
                .with_untracked(|m| m.get(&id).map(|e| e.name.clone()));
            toasts.push(match name {
                Some(name) => format!("{name}: {reason}"),
                None => reason,
            });
            house.command_failed.set(None);
        }
    });
    let theme = install_theme();
    provide_context(theme);
    let alarm_armed = Memo::new(move |_| house.entities.with(|m| any_alarm_armed(m.values())));
    let solar_active = Memo::new(move |_| {
        house
            .entities
            .with(|m| crate::energy::any_solar_active(m.values()))
    });
    #[cfg(target_arch = "wasm32")]
    crate::ws::connect("/ws", house);
    // Easter egg: double-clicking the logo rings the doorbell and gives the house a little shake.
    let ringing = RwSignal::new(false);
    let ring = move |_| {
        crate::chime::ring();
        ringing.set(true);
        gloo_timers::callback::Timeout::new(1200, move || {
            ringing.try_set(false);
        })
        .forget();
    };

    view! {
        <Router>
            <TrackReturnPath />
            <header class="topbar">
                <span class="topbar__brand">
                    <img
                        class="topbar__logo"
                        class:topbar__logo--ringing=move || ringing.get()
                        src="/assets/icon.svg"
                        alt=""
                        width="28"
                        height="28"
                        draggable="false"
                        on:dblclick=ring
                    />
                    <span class="topbar__name">"Oikos"</span>
                </span>
                <StatusPill />
                <button
                    type="button"
                    class="theme-toggle"
                    title="Switch theme"
                    on:click=move |_| theme.update(|t| *t = next_theme(*t))
                >
                    {move || theme.get().as_str()}
                </button>
                <nav class="topbar__nav">
                    <A href="/" exact=true>"Home"</A>
                    <A href="/energy">
                        <span
                            class="nav__glyph"
                            class:nav__glyph--solar=move || solar_active.get()
                            inner_html=BOLT
                        ></span>
                        "Energy"
                    </A>
                    <A href="/security">
                        <span
                            class="nav__glyph"
                            class:nav__glyph--armed=move || alarm_armed.get()
                            inner_html=SHIELD
                        ></span>
                        "Security"
                    </A>
                    <A href="/alerts">"Alerts"</A>
                    <A href="/flows">"Flows"</A>
                    <A href="/devices">"Devices"</A>
                    <A href="/settings">"Settings"</A>
                </nav>
            </header>
            <main class="screen">
                <Routes fallback=HomeScreen>
                    <Route path=path!("/") view=HomeScreen />
                    <Route path=path!("/entity/:id") view=EntitySheetScreen />
                    <Route path=path!("/history/:id") view=HistoryDetailScreen />
                    <Route path=path!("/group/:area/:kind") view=GroupSheetScreen />
                    <Route path=path!("/energy") view=EnergyScreen />
                    <Route path=path!("/security") view=SecurityScreen />
                    <Route path=path!("/alerts") view=AlertsScreen />
                    <Route path=path!("/alerts/:id") view=AlertDetailScreen />
                    <Route path=path!("/flows") view=FlowsScreen />
                    <Route path=path!("/flows/new") view=NewFlowScreen />
                    <Route path=path!("/flows/:id") view=FlowCanvasScreen />
                    <Route path=path!("/devices") view=DevicesScreen />
                    <Route path=path!("/settings") view=SettingsScreen />
                </Routes>
            </main>
            <ToastQueue />
        </Router>
    }
}

#[cfg(test)]
mod tests {
    use super::any_alarm_armed;
    use oikos_api::EntityView;
    use oikos_core::model::{
        Capabilities, EntityCategory, EntityState, Kind, StateValue, UiOverride,
    };

    fn panel(state: &str) -> EntityView {
        EntityView {
            id: "alarm".into(),
            device_id: None,
            name: "Alarm".into(),
            device_name: None,
            label: None,
            kind: Kind::AlarmPanel,
            protocol_kind: Kind::AlarmPanel,
            device_class: None,
            unit: None,
            icon: None,
            category: EntityCategory::Primary,
            enabled_by_default: true,
            area: "hall".into(),
            part_of: None,
            capabilities: Capabilities::default(),
            ui: UiOverride::default(),
            state: EntityState {
                value: StateValue::Alarm(state.into()),
                ..Default::default()
            },
        }
    }

    #[test]
    fn armed_glyph_follows_the_panel_state() {
        assert!(!any_alarm_armed([panel("disarmed")].iter()));
        assert!(!any_alarm_armed([panel("disarming")].iter()));
        assert!(any_alarm_armed([panel("armed_away")].iter()));
        assert!(any_alarm_armed([panel("arming")].iter()));
        assert!(any_alarm_armed([panel("pending")].iter()));
        assert!(any_alarm_armed([panel("triggered")].iter()));
        assert!(!any_alarm_armed(std::iter::empty()));
    }
}
