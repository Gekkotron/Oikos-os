//! Theme switch: Night Glass, Warm Minimal, or follow the system.

use leptos::prelude::*;

/// Storage key for the persisted choice.
#[cfg(target_arch = "wasm32")]
const STORAGE_KEY: &str = "oikos.theme";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Theme {
    /// Night Glass (dark).
    Night,
    /// Warm Minimal (light).
    Warm,
    /// Follow `prefers-color-scheme`.
    Auto,
}

impl Theme {
    pub fn as_str(self) -> &'static str {
        match self {
            Theme::Night => "night",
            Theme::Warm => "warm",
            Theme::Auto => "auto",
        }
    }
}

/// Reads a stored value; anything missing or unknown is `Auto`.
pub fn parse(value: Option<&str>) -> Theme {
    match value {
        Some("night") => Theme::Night,
        Some("warm") => Theme::Warm,
        _ => Theme::Auto,
    }
}

/// The `data-theme` attribute for a theme, `None` to follow the system.
#[cfg(any(target_arch = "wasm32", test))]
fn attribute(theme: Theme) -> Option<&'static str> {
    match theme {
        Theme::Night => Some("dark"),
        Theme::Warm => Some("light"),
        Theme::Auto => None,
    }
}

/// `localStorage`, or `None` when it is missing, blocked or throws.
#[cfg(target_arch = "wasm32")]
fn storage() -> Option<web_sys::Storage> {
    web_sys::window().and_then(|w| w.local_storage().ok().flatten())
}

/// Loads the saved theme, applies it to `<html>` and keeps both in sync with the returned signal.
/// Every storage access is fallible and silent: a private window must never panic.
pub fn install_theme() -> RwSignal<Theme> {
    #[cfg(target_arch = "wasm32")]
    let initial = parse(
        storage()
            .and_then(|s| s.get_item(STORAGE_KEY).ok().flatten())
            .as_deref(),
    );
    #[cfg(not(target_arch = "wasm32"))]
    let initial = Theme::Auto;

    let theme = RwSignal::new(initial);

    #[cfg(target_arch = "wasm32")]
    Effect::new(move |_| {
        let current = theme.get();
        if let Some(root) = web_sys::window()
            .and_then(|w| w.document())
            .and_then(|d| d.document_element())
        {
            let _ = match attribute(current) {
                Some(value) => root.set_attribute("data-theme", value),
                None => root.remove_attribute("data-theme"),
            };
        }
        if let Some(storage) = storage() {
            let _ = storage.set_item(STORAGE_KEY, current.as_str());
        }
    });

    theme
}

#[cfg(test)]
mod tests {
    use super::Theme;
    use super::parse;
    #[test]
    fn parses_known_values() {
        assert_eq!(parse(Some("night")), Theme::Night);
        assert_eq!(parse(Some("warm")), Theme::Warm);
        assert_eq!(parse(Some("auto")), Theme::Auto);
        assert_eq!(parse(None), Theme::Auto);
        assert_eq!(parse(Some("gibberish")), Theme::Auto);
    }
    #[test]
    fn maps_to_the_data_theme_attribute() {
        assert_eq!(super::attribute(Theme::Night), Some("dark"));
        assert_eq!(super::attribute(Theme::Warm), Some("light"));
        assert_eq!(super::attribute(Theme::Auto), None);
    }
    #[test]
    fn serialises_known_values() {
        assert_eq!(Theme::Night.as_str(), "night");
        assert_eq!(Theme::Warm.as_str(), "warm");
        assert_eq!(Theme::Auto.as_str(), "auto");
    }
}
