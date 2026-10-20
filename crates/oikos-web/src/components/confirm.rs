//! The inline "are you sure?" that replaces a card's controls before a door opens or a siren
//! rings.

use leptos::prelude::*;

/// The entity whose card is asking "are you sure?", if any. Kept above the cards: a card is
/// rebuilt on every state echo, and a question held in the card would vanish with it.
#[derive(Clone, Copy)]
pub struct Confirming(RwSignal<Option<String>>);

impl Confirming {
    pub fn new() -> Self {
        Self(RwSignal::new(None))
    }

    /// Opens the question on `id`'s card, closing any other.
    pub fn ask(&self, id: &str) {
        self.0.set(Some(id.to_owned()));
    }

    /// True while `id`'s card is asking (tracked).
    pub fn is_asking(&self, id: &str) -> bool {
        self.0.with(|c| c.as_deref() == Some(id))
    }

    pub fn clear(&self) {
        self.0.set(None);
    }
}

impl Default for Confirming {
    fn default() -> Self {
        Self::new()
    }
}

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

#[cfg(test)]
mod tests {
    use super::Confirming;

    #[test]
    fn question_outlives_a_card_rebuild() {
        let confirming = Confirming::new();
        confirming.ask("gate");
        // A card rebuilt by a state echo reads the same context: the question is still open.
        let rebuilt = confirming;
        assert!(rebuilt.is_asking("gate"));
        assert!(!rebuilt.is_asking("siren"));
        rebuilt.ask("siren");
        assert!(!rebuilt.is_asking("gate"), "one question at a time");
        rebuilt.clear();
        assert!(!rebuilt.is_asking("siren"));
    }
}
