//! The connection pill in the top bar.

use leptos::prelude::*;
use oikos_api::ConnectionStatus;

use crate::house::House;

#[component]
pub fn StatusPill() -> impl IntoView {
    let house = expect_context::<House>();
    let connection = house.connection;
    let class = move || match connection.get() {
        ConnectionStatus::Connecting => "pill pill--connecting",
        ConnectionStatus::Online => "pill pill--online",
        ConnectionStatus::Offline { .. } => "pill pill--offline",
    };
    let text = move || match connection.get() {
        ConnectionStatus::Connecting => "Reconnecting…",
        ConnectionStatus::Online => "Online",
        ConnectionStatus::Offline { .. } => "Offline",
    };
    let title = move || match connection.get() {
        ConnectionStatus::Offline { reason } => reason,
        _ => String::new(),
    };
    view! {
        <span class=class title=title role="status">
            <span class="pill__dot"></span>
            {text}
        </span>
    }
}
