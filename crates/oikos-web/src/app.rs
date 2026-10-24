//! The app root: house state, contexts and the routes, framed by the sidebar shell.

use leptos::prelude::*;
use leptos_router::components::{Route, Router, Routes};
use leptos_router::hooks::use_navigate;
use leptos_router::path;
use oikos_api::DashboardView;
use oikos_core::model::{Kind, StateValue};

use crate::ambiances::about::AmbiancesAboutScreen;
use crate::ambiances::editor_placeholder::AmbianceEditorPlaceholder;
use crate::ambiances::page::AmbiancesScreen;
use crate::ambiances::report::AmbianceReportScreen;
use crate::commands::Pending;
use crate::components::confirm::Confirming;
use crate::components::now::install_now;
use crate::components::toast::{ToastQueue, Toasts};
use crate::house::House;
use crate::i18n::install_lang;
use crate::nav::{TrackBackStack, install_nav};
use crate::screens::alert_detail::AlertDetailScreen;
use crate::screens::alerts::AlertsScreen;
use crate::screens::climate::ClimateScreen;
use crate::screens::dashboard::DashboardScreen;
use crate::screens::devices::DevicesScreen;
use crate::screens::energy::{EnergyScreen, install_history_refresh};
use crate::screens::entity_sheet::EntitySheetScreen;
use crate::screens::flow_canvas::{FlowCanvasScreen, NewFlowScreen};
use crate::screens::flows::FlowsScreen;
use crate::screens::group_sheet_v2::GroupSheetV2Screen;
use crate::screens::group_wizard::GroupWizardScreen;
use crate::screens::groups::GroupsScreen;
use crate::screens::history_detail::HistoryDetailScreen;
use crate::screens::room::RoomScreen;
use crate::screens::security::SecurityScreen;
use crate::screens::settings::{SettingsPageScreen, SettingsScreen};
use crate::screens::weather::WeatherScreen;
use crate::shell::Shell;
use crate::theme::install_theme;

/// Where landing goes: the first authored dashboard if there is one, else House; `None` while the
/// list is still empty (not fetched yet).
pub(crate) fn landing_dashboard(list: &[DashboardView]) -> Option<String> {
    if list.is_empty() {
        return None;
    }
    Some(
        list.iter()
            .find(|d| d.dashboard.id != oikos_api::HOUSE_ID)
            .map(|d| d.dashboard.id.clone())
            .unwrap_or_else(|| oikos_api::HOUSE_ID.to_owned()),
    )
}

/// Landing and unknown paths go to [`landing_dashboard`]. Waits for the dashboards fetch to
/// resolve before deciding, so an authored dashboard is honoured even when the user opens the app
/// directly on `/`. `replace` keeps Back from bouncing off the redirect.
#[component]
fn HouseRedirect() -> impl IntoView {
    let house = expect_context::<House>();
    let navigate = use_navigate();
    Effect::new(move |_| {
        if let Some(id) = house.dashboards.with(|list| landing_dashboard(list)) {
            navigate(
                &format!("/dashboards/{id}"),
                leptos_router::NavigateOptions {
                    replace: true,
                    ..Default::default()
                },
            );
        }
    });
}

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

#[component]
pub fn App() -> impl IntoView {
    let house = House::new();
    provide_context(house);
    let toasts = Toasts::new();
    provide_context(toasts);
    provide_context(Pending::new());
    provide_context(Confirming::new());
    provide_context(install_now());
    // Dashboards are not pushed over the socket, so the shell fetches them once at start-up.
    leptos::task::spawn_local(async move {
        match crate::api::fetch_dashboards().await {
            Ok(list) => house.dashboards.set(list),
            Err(err) => toasts.push(format!("Could not load dashboards: {}", err.error)),
        }
    });
    install_nav();
    crate::alerts::install_alerts_badge(house);
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
    // The Climate link shows only while the hub's engine answers (a 503 means it is disabled).
    let climate_on = RwSignal::new(false);
    Effect::new(move |_| {
        house.climate_rev.track();
        leptos::task::spawn_local(async move {
            // Only the engine being disabled hides the link; any other failure keeps it as it was.
            match crate::api::climate::get_climate().await {
                Ok(_) => {
                    climate_on.try_set(true);
                }
                Err(e) if e.error == crate::api::climate::CLIMATE_DISABLED => {
                    climate_on.try_set(false);
                }
                Err(_) => {}
            }
        });
    });
    let theme = install_theme();
    provide_context(theme);
    provide_context(install_lang());
    let alarm_armed = Memo::new(move |_| house.entities.with(|m| any_alarm_armed(m.values())));
    let solar_active = Memo::new(move |_| {
        house
            .entities
            .with(|m| crate::energy::any_solar_active(m.values()))
    });
    // Count of alerts received since the user's last visit to /alerts. The badge in the sidebar
    // uses this: hidden at zero, capped at "99+".
    let alerts_badge = Memo::new(move |_| {
        let count = house
            .alerts
            .with(|list| crate::alerts::unseen_count(list, house.last_alerts_seen.get()));
        crate::alerts::badge_text(count)
    });
    #[cfg(target_arch = "wasm32")]
    crate::ws::connect("/ws", house);
    #[cfg(target_arch = "wasm32")]
    {
        house.refresh_flows_config();
        house.refresh_weather();
        house.refresh_ambiances();
        house.refresh_home_settings();
    }
    view! {
        <Router>
            <TrackBackStack />
            <crate::alerts::TrackAlertsSeen />
            <Shell climate_on alarm_armed solar_active alerts_badge>
                <Routes fallback=HouseRedirect>
                    <Route path=path!("/") view=HouseRedirect />
                    <Route path=path!("/dashboards/:id") view=DashboardScreen />
                    <Route path=path!("/rooms/:id") view=RoomScreen />
                    <Route path=path!("/entity/:id") view=EntitySheetScreen />
                    <Route path=path!("/history/:id") view=HistoryDetailScreen />
                    <Route path=path!("/group/:id") view=GroupSheetV2Screen />
                    <Route path=path!("/energy") view=EnergyScreen />
                    <Route path=path!("/security") view=SecurityScreen />
                    <Route path=path!("/climate") view=ClimateScreen />
                    <Route path=path!("/weather") view=WeatherScreen />
                    <Route path=path!("/alerts") view=AlertsScreen />
                    <Route path=path!("/alerts/:id") view=AlertDetailScreen />
                    <Route path=path!("/ambiances") view=AmbiancesScreen />
                    <Route path=path!("/ambiances/about") view=AmbiancesAboutScreen />
                    <Route path=path!("/ambiances/new") view=AmbianceEditorPlaceholder />
                    <Route path=path!("/ambiances/:id") view=AmbianceEditorPlaceholder />
                    <Route path=path!("/ambiances/:id/report") view=AmbianceReportScreen />
                    <Route path=path!("/flows") view=FlowsScreen />
                    <Route path=path!("/flows/new") view=NewFlowScreen />
                    <Route path=path!("/flows/:id") view=FlowCanvasScreen />
                    <Route path=path!("/groups") view=GroupsScreen />
                    <Route path=path!("/groups/new") view=GroupWizardScreen />
                    <Route path=path!("/groups/:id/edit") view=GroupWizardScreen />
                    <Route path=path!("/devices") view=DevicesScreen />
                    <Route path=path!("/settings") view=SettingsScreen />
                    <Route path=path!("/settings/:page") view=SettingsPageScreen />
                </Routes>
            </Shell>
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
            component: "alarm_control_panel".to_string(),
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
