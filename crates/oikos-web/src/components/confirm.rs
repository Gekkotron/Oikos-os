//! The inline "are you sure?" that replaces a card's controls before a door opens or a siren
//! rings.

use leptos::prelude::*;

#[component]
pub fn ConfirmBar(
    question: String,
    yes: &'static str,
    on_yes: Callback<()>,
    on_cancel: Callback<()>,
) -> impl IntoView {
    let label = question.clone();
    view! {
        <div class="confirm" role="group" aria-label=label>
            <span class="confirm__question">{question}</span>
            <span class="confirm__buttons">
                <button type="button" class="btn btn--compact" on:click=move |_| on_yes.run(())>
                    {yes}
                </button>
                <button type="button" class="btn btn--compact" on:click=move |_| on_cancel.run(())>
                    "Cancel"
                </button>
            </span>
        </div>
    }
}
