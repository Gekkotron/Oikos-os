//! The siren card: a power toggle that reads "Ringing" in red while on and asks before ringing.

use leptos::prelude::*;
use oikos_api::EntityView;

use super::light::{power_command, read_on, with_on};
use super::{PowerToggle, Sender};
use crate::components::card_frame::{CardFrame, open_entity};
use crate::components::confirm::ConfirmBar;
use crate::format::display_value;

#[component]
pub fn SirenCard(entity: EntityView) -> impl IntoView {
    let sender = Sender::from_context();
    let asking = RwSignal::new(false);
    let on = read_on(&entity.state).unwrap_or(false);
    let label = if on {
        "Ringing".to_owned()
    } else {
        display_value(&entity.state, entity.unit.as_deref())
    };
    let question = format!("Ring {}?", entity.name);
    let send = {
        let id = entity.id.clone();
        let state = entity.state.clone();
        Callback::new(move |next: bool| {
            asking.set(false);
            sender.send(&id, power_command(next), with_on(&state, next));
        })
    };
    let ask = entity.ui.security_ask;
    let toggle = Callback::new(move |()| {
        if !on && ask {
            asking.set(true);
        } else {
            send.run(!on);
        }
    });
    let pending = sender.pending(&entity.id);
    let on_click = open_entity(&entity.id);
    view! {
        <CardFrame entity=entity on_click=on_click pending=pending alert=on>
            <Show
                when=move || asking.get()
                fallback=move || {
                    view! { <PowerToggle on=on label=label.clone() on_toggle=toggle /> }
                }
            >
                <ConfirmBar
                    question=question.clone()
                    yes="Ring"
                    on_yes=Callback::new(move |()| send.run(true))
                    on_cancel=Callback::new(move |()| asking.set(false))
                />
            </Show>
        </CardFrame>
    }
}
