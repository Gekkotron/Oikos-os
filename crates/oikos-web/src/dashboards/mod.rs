pub mod config_sheet;
pub mod edit;
pub mod grid;
pub mod library;
pub mod widget;

use leptos::prelude::*;
use oikos_api::DashboardView;
use oikos_core::dashboard::Dashboard;

use crate::api;
use crate::house::House;

/// Saves an empty "New dashboard" after the last one and adds it to `house`; returns its id.
/// `None` when the save fails: the list stays as it was and the edit-mode task owns error display.
pub(crate) async fn create_dashboard(house: House) -> Option<String> {
    let order = house
        .dashboards
        .with_untracked(|list| list.iter().map(|d| d.dashboard.order).max().unwrap_or(0) + 1);
    let d = Dashboard {
        id: format!("d_{}", gen_id()),
        name: "New dashboard".to_owned(),
        order,
        widgets: Vec::new(),
        show_header: false,
    };
    let saved = api::put_dashboard(&d).await.ok()?;
    let id = saved.id.clone();
    house.dashboards.update(|list| {
        list.push(DashboardView {
            dashboard: saved,
            synthesized: false,
        });
    });
    Some(id)
}

/// A short URL-safe id from the clock; avoids a uuid dependency for a single-user client.
fn gen_id() -> String {
    format!("{:x}", js_sys::Date::now() as u64)
}
