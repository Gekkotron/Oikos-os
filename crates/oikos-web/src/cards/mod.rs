//! Entity cards, one component per kind.

pub mod light;
pub mod switch;

use leptos::prelude::*;
use oikos_core::model::{Command, EntityState};

use crate::commands::{Pending, fire_command};
use crate::components::toast::Toasts;
use crate::house::House;

/// The contexts a card needs to send commands, captured while the card is built so event
/// handlers can use them later.
#[derive(Clone, Copy)]
pub struct Sender {
    house: House,
    toasts: Toasts,
    pending: Pending,
}

impl Sender {
    pub fn from_context() -> Self {
        Self {
            house: expect_context(),
            toasts: expect_context(),
            pending: expect_context(),
        }
    }

    /// See [`fire_command`].
    pub fn send(&self, id: &str, cmd: Command, optimistic: Option<EntityState>) {
        fire_command(
            self.house,
            id.to_owned(),
            cmd,
            self.toasts,
            self.pending,
            optimistic,
        );
    }

    /// True while `id` has a command in flight.
    pub fn pending(&self, id: &str) -> Signal<bool> {
        self.pending.signal(id)
    }
}

/// The pill toggle with a sliding knob. The whole row is the tap target.
#[component]
pub fn PowerToggle(
    on: bool,
    #[prop(into)] label: String,
    on_toggle: Callback<()>,
) -> impl IntoView {
    view! {
        <button
            type="button"
            class="power"
            role="switch"
            aria-checked=on.to_string()
            on:click=move |_| on_toggle.run(())
        >
            <span class="power__label">{label}</span>
            <span class="switch" class:switch--on=on>
                <span class="switch__knob"></span>
            </span>
        </button>
    }
}
