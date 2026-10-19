//! Entity cards, one component per kind.

pub mod alarm;
pub mod binary_sensor;
pub mod button;
pub mod climate;
pub mod cover;
pub mod light;
pub mod number;
pub mod select;
pub mod sensor;
pub mod switch;
pub mod vacuum;
pub mod water_heater;

use leptos::prelude::*;
use oikos_core::model::{Command, EntityState};

use crate::commands::{Pending, fire_command};
use crate::components::toast::Toasts;
use crate::format::word;
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

/// A value readout between `−` and `+` step buttons. The buttons are disabled when there is
/// nothing to step from; `on_step` receives `-1.0` or `1.0`.
#[component]
pub fn Stepper(
    #[prop(into)] label: String,
    can_step: bool,
    on_step: Callback<f64>,
) -> impl IntoView {
    view! {
        <div class="stepper">
            <button
                type="button"
                class="btn btn--round"
                aria-label="Decrease"
                disabled=!can_step
                on:click=move |_| on_step.run(-1.0)
            >
                "−"
            </button>
            <span class="card__value stepper__value">{label}</span>
            <button
                type="button"
                class="btn btn--round"
                aria-label="Increase"
                disabled=!can_step
                on:click=move |_| on_step.run(1.0)
            >
                "+"
            </button>
        </div>
    }
}

/// A labelled `<select>` over `options`; `on_select` receives the picked option.
#[component]
pub fn ModeSelect(
    #[prop(into)] label: String,
    options: Vec<String>,
    selected: Option<String>,
    on_select: Callback<String>,
) -> impl IntoView {
    let items = options
        .into_iter()
        .map(|option| {
            let is_selected = selected.as_deref() == Some(option.as_str());
            let text = word(&option);
            view! { <option value=option selected=is_selected>{text}</option> }
        })
        .collect_view();
    view! {
        <select
            class="select"
            aria-label=label
            on:change=move |ev| on_select.run(event_target_value(&ev))
        >
            {items}
        </select>
    }
}
