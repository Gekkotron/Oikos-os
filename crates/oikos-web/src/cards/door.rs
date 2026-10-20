//! The door opener card: one "Open" button that turns the relay on, asking first by default.

use leptos::prelude::*;
use oikos_api::EntityView;
use oikos_core::model::Command;

use super::Sender;
use super::light::read_on;
use crate::components::card_frame::{CardFrame, open_entity};
use crate::components::confirm::ConfirmBar;

/// What a tap on "Open" does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DoorTap {
    /// Show the question first.
    Ask,
    Send,
}

pub fn door_tap(ask: bool) -> DoorTap {
    if ask { DoorTap::Ask } else { DoorTap::Send }
}

pub(crate) fn turn_on() -> Command {
    Command::TurnOn {
        brightness: None,
        color: None,
        effect: None,
    }
}

#[component]
pub fn DoorCard(entity: EntityView) -> impl IntoView {
    let sender = Sender::from_context();
    let asking = RwSignal::new(false);
    let on = read_on(&entity.state) == Some(true);
    let question = format!("Open {}?", entity.name);
    let send = {
        let id = entity.id.clone();
        Callback::new(move |()| {
            asking.set(false);
            sender.send(&id, turn_on(), None);
        })
    };
    let tap = door_tap(entity.ui.security_ask);
    let on_open = move |_| match tap {
        DoorTap::Ask => asking.set(true),
        DoorTap::Send => send.run(()),
    };
    let pending = sender.pending(&entity.id);
    let on_click = open_entity(&entity.id);
    view! {
        <CardFrame entity=entity on_click=on_click pending=pending>
            <Show
                when=move || asking.get()
                fallback=move || {
                    view! {
                        <div class="card__controls">
                            <button type="button" class="btn" on:click=on_open>
                                "Open"
                            </button>
                        </div>
                        {on.then(|| view! { <span class="card__note">"On"</span> })}
                    }
                }
            >
                <ConfirmBar
                    question=question.clone()
                    yes="Open"
                    on_yes=send
                    on_cancel=Callback::new(move |()| asking.set(false))
                />
            </Show>
        </CardFrame>
    }
}

#[cfg(test)]
mod tests {
    use super::{DoorTap, door_tap};

    #[test]
    fn door_asks_only_with_security_ask() {
        assert_eq!(door_tap(true), DoorTap::Ask);
        assert_eq!(door_tap(false), DoorTap::Send);
    }
}
