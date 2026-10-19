//! The button card: one wide press button.

use leptos::prelude::*;
use oikos_api::EntityView;
use oikos_core::model::Command;

use super::Sender;
use crate::components::card_frame::{CardFrame, open_entity};

#[component]
pub fn ButtonCard(entity: EntityView) -> impl IntoView {
    let sender = Sender::from_context();
    let on_press = {
        let id = entity.id.clone();
        move |_| sender.send(&id, Command::Press, None)
    };
    let pending = sender.pending(&entity.id);
    let on_click = open_entity(&entity.id);
    view! {
        <CardFrame entity=entity on_click=on_click pending=pending>
            <div class="card__controls">
                <button type="button" class="btn" on:click=on_press>
                    "Press"
                </button>
            </div>
        </CardFrame>
    }
}
