//! One flow as a card in the grid view.

use leptos::prelude::*;
use leptos_router::components::A;

use super::row::{icon_view, invalid_badge, is_restart, last_run, last_run_failed};
use crate::api;
use crate::components::flow_switch::FlowSwitch;
use crate::components::now::Now;
use crate::flow_list::ListRow;
use crate::format::format_timestamp;

/// A card of the grid view. `menu` is the slot at the bottom end for the ⋯ menu.
#[component]
pub fn FlowCard(row: ListRow, #[prop(optional)] menu: Option<AnyView>) -> impl IntoView {
    let now = expect_context::<Now>();
    let flow = row.view.clone();
    let id = flow.definition.id.clone();
    let href = format!("/flows/{}", api::encode_id(&id));
    let name = flow.definition.name.clone();
    let enabled = flow.definition.enabled;
    let invalid = flow.invalid.is_some();
    let started = flow.runs.first().map(|r| format_timestamp(r.started_at));
    let failed = last_run_failed(&flow);
    let restart = is_restart(&flow);
    let badge = invalid_badge(&flow);
    let icon = icon_view(&row);
    let name_for_switch = name.clone();
    let run_flow = flow.clone();
    view! {
        <li
            class="flow-tile"
            class:flow-tile--off=!enabled
            class:flow-tile--attention=row.attention
        >
            <div class="flow-tile__top">
                {icon}
                <span class="flow-tile__name">
                    <A href=href attr:class="flow-row__link">{name}</A>
                    {badge}
                </span>
                <span class="flow-tile__switch">
                    <FlowSwitch id=id name=name_for_switch enabled=enabled invalid=invalid />
                </span>
            </div>
            <p class="flow-tile__trigger">{row.trigger_text.clone()}</p>
            <div class="flow-tile__bottom">
                <span class="flow-row__run" class:flow-row__run--failed=failed title=started>
                    {move || last_run(&run_flow, now.0.get())}
                </span>
                {restart.then(|| view! { <span class="flow-tag">"restart"</span> })}
                <span class="flow-tile__menu">{menu}</span>
            </div>
        </li>
    }
}
