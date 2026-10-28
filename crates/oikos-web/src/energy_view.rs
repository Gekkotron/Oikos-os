//! Energy view switch: the Classic diagram or the Flux (Sankey) one.

use leptos::prelude::*;

/// Storage key for the persisted choice.
#[cfg(target_arch = "wasm32")]
const STORAGE_KEY: &str = "oikos.energy_view";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum EnergyView {
    /// The original diagram.
    #[default]
    Classic,
    /// The live Sankey diagram.
    Flux,
}

impl EnergyView {
    pub fn as_str(self) -> &'static str {
        match self {
            EnergyView::Classic => "classic",
            EnergyView::Flux => "flux",
        }
    }
}

/// Reads a stored value; anything missing or unknown is `Classic`.
pub fn parse(value: Option<&str>) -> EnergyView {
    match value {
        Some("flux") => EnergyView::Flux,
        _ => EnergyView::Classic,
    }
}

/// `localStorage`, or `None` when it is missing, blocked or throws.
#[cfg(target_arch = "wasm32")]
fn storage() -> Option<web_sys::Storage> {
    web_sys::window().and_then(|w| w.local_storage().ok().flatten())
}

/// Loads the saved choice and keeps it in sync with the returned signal.
/// Every storage access is fallible and silent: a private window must never panic.
pub fn install_energy_view() -> RwSignal<EnergyView> {
    #[cfg(target_arch = "wasm32")]
    let initial = parse(
        storage()
            .and_then(|s| s.get_item(STORAGE_KEY).ok().flatten())
            .as_deref(),
    );
    #[cfg(not(target_arch = "wasm32"))]
    let initial = EnergyView::Classic;

    let view = RwSignal::new(initial);

    #[cfg(target_arch = "wasm32")]
    Effect::new(move |_| {
        if let Some(storage) = storage() {
            let _ = storage.set_item(STORAGE_KEY, view.get().as_str());
        }
    });

    view
}

#[cfg(test)]
mod tests {
    use super::{EnergyView, parse};

    #[test]
    fn parses_known_values() {
        assert_eq!(parse(Some("classic")), EnergyView::Classic);
        assert_eq!(parse(Some("flux")), EnergyView::Flux);
        assert_eq!(parse(None), EnergyView::Classic);
        assert_eq!(parse(Some("orbit")), EnergyView::Classic);
    }
    #[test]
    fn serialises_known_values() {
        assert_eq!(EnergyView::Classic.as_str(), "classic");
        assert_eq!(EnergyView::Flux.as_str(), "flux");
    }
}
