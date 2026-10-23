//! FR/EN strings for the shell: language choice, storage and the text table.

use leptos::prelude::*;

/// Storage key for the persisted choice.
#[cfg(target_arch = "wasm32")]
const STORAGE_KEY: &str = "oikos.lang";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lang {
    En,
    Fr,
}

impl Lang {
    pub fn as_str(self) -> &'static str {
        match self {
            Lang::En => "en",
            Lang::Fr => "fr",
        }
    }

    /// The other language, for the toggle.
    pub fn next(self) -> Lang {
        match self {
            Lang::En => Lang::Fr,
            Lang::Fr => Lang::En,
        }
    }
}

/// Reads a stored value; `None` when missing or unknown.
pub fn parse(value: Option<&str>) -> Option<Lang> {
    match value {
        Some("en") => Some(Lang::En),
        Some("fr") => Some(Lang::Fr),
        _ => None,
    }
}

/// French for any `fr*` browser tag (case-insensitive), English otherwise.
pub fn from_browser(tag: &str) -> Lang {
    if tag.to_ascii_lowercase().starts_with("fr") {
        Lang::Fr
    } else {
        Lang::En
    }
}

/// `localStorage`, or `None` when it is missing, blocked or throws.
#[cfg(target_arch = "wasm32")]
fn storage() -> Option<web_sys::Storage> {
    web_sys::window().and_then(|w| w.local_storage().ok().flatten())
}

/// Loads the saved language (else the browser's), keeps `<html lang>` and storage in sync with the
/// returned signal. Every storage access is fallible and silent: a private window must never panic.
pub fn install_lang() -> RwSignal<Lang> {
    #[cfg(target_arch = "wasm32")]
    let initial = parse(
        storage()
            .and_then(|s| s.get_item(STORAGE_KEY).ok().flatten())
            .as_deref(),
    )
    .unwrap_or_else(|| {
        from_browser(
            &web_sys::window()
                .and_then(|w| w.navigator().language())
                .unwrap_or_default(),
        )
    });
    #[cfg(not(target_arch = "wasm32"))]
    let initial = Lang::En;

    let lang = RwSignal::new(initial);

    #[cfg(target_arch = "wasm32")]
    Effect::new(move |_| {
        let current = lang.get();
        if let Some(root) = web_sys::window()
            .and_then(|w| w.document())
            .and_then(|d| d.document_element())
        {
            let _ = root.set_attribute("lang", current.as_str());
        }
        if let Some(storage) = storage() {
            let _ = storage.set_item(STORAGE_KEY, current.as_str());
        }
    });

    lang
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    MyHome,
    Overview,
    House,
    Automations,
    Groups,
    MyRooms,
    HomeSection,
    Energy,
    Security,
    Climate,
    Beta,
    Alerts,
    Settings,
    Devices,
    NewDashboard,
    NewFlow,
    NewGroup,
    EditGroup,
    History,
    Alert,
    Online,
    Reconnecting,
    Offline,
    OpenMenu,
    CloseMenu,
    ChangeTheme,
    /// Names the language you switch TO.
    ChangeLanguage,
    Navigation,
    Breadcrumb,
    RoomGone,
}

impl Key {
    pub const ALL: &'static [Key] = &[
        Key::MyHome,
        Key::Overview,
        Key::House,
        Key::Automations,
        Key::Groups,
        Key::MyRooms,
        Key::HomeSection,
        Key::Energy,
        Key::Security,
        Key::Climate,
        Key::Beta,
        Key::Alerts,
        Key::Settings,
        Key::Devices,
        Key::NewDashboard,
        Key::NewFlow,
        Key::NewGroup,
        Key::EditGroup,
        Key::History,
        Key::Alert,
        Key::Online,
        Key::Reconnecting,
        Key::Offline,
        Key::OpenMenu,
        Key::CloseMenu,
        Key::ChangeTheme,
        Key::ChangeLanguage,
        Key::Navigation,
        Key::Breadcrumb,
        Key::RoomGone,
    ];
}

/// The text for a key in a language.
pub fn text(lang: Lang, key: Key) -> &'static str {
    let (en, fr) = match key {
        Key::MyHome => ("My home", "Ma maison"),
        Key::Overview => ("Overview", "Vue d'ensemble"),
        Key::House => ("House", "Maison"),
        Key::Automations => ("Automations", "Automatisations"),
        Key::Groups => ("Groups", "Groupes"),
        Key::MyRooms => ("My rooms", "Mes pièces"),
        Key::HomeSection => ("Home", "Maison"),
        Key::Energy => ("Energy", "Énergie"),
        Key::Security => ("Security", "Sécurité"),
        Key::Climate => ("Climate", "Climat"),
        Key::Beta => ("Beta", "Bêta"),
        Key::Alerts => ("Alerts", "Alertes"),
        Key::Settings => ("Settings", "Réglages"),
        Key::Devices => ("Devices", "Appareils"),
        Key::NewDashboard => ("New dashboard", "Nouveau tableau"),
        Key::NewFlow => ("New flow", "Nouvelle automatisation"),
        Key::NewGroup => ("New group", "Nouveau groupe"),
        Key::EditGroup => ("Edit group", "Modifier le groupe"),
        Key::History => ("History", "Historique"),
        Key::Alert => ("Alert", "Alerte"),
        Key::Online => ("Connected", "La maison est connectée"),
        Key::Reconnecting => ("Reconnecting…", "Reconnexion…"),
        Key::Offline => ("Offline", "Hors ligne"),
        Key::OpenMenu => ("Open menu", "Ouvrir le menu"),
        Key::CloseMenu => ("Close menu", "Fermer le menu"),
        Key::ChangeTheme => ("Change theme", "Changer de thème"),
        Key::ChangeLanguage => ("Français", "English"),
        Key::Navigation => ("Main", "Navigation principale"),
        Key::Breadcrumb => ("Breadcrumb", "Fil d'Ariane"),
        Key::RoomGone => ("This room is gone.", "Cette pièce n'existe plus."),
    };
    match lang {
        Lang::En => en,
        Lang::Fr => fr,
    }
}

/// The accessible label of the Alerts badge: "3 unseen alerts" / "3 alertes non lues", singular
/// for a badge of "1" ("99+" stays plural).
pub fn unseen_alerts(lang: Lang, badge: &str) -> String {
    match (lang, badge == "1") {
        (Lang::En, true) => format!("{badge} unseen alert"),
        (Lang::En, false) => format!("{badge} unseen alerts"),
        (Lang::Fr, true) => format!("{badge} alerte non lue"),
        (Lang::Fr, false) => format!("{badge} alertes non lues"),
    }
}

/// A translator bound to the language signal in context (reactive), or English when there is none.
pub fn use_text() -> impl Fn(Key) -> &'static str + Copy + 'static {
    let lang = use_context::<RwSignal<Lang>>();
    move |key| text(lang.map_or(Lang::En, |l| l.get()), key)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn every_key_has_both_languages() {
        for &k in Key::ALL {
            assert!(
                !text(Lang::En, k).is_empty() && !text(Lang::Fr, k).is_empty(),
                "{k:?}"
            );
        }
    }
    #[test]
    fn browser_language_picks_french_only_for_fr() {
        assert_eq!(from_browser("fr-FR"), Lang::Fr);
        assert_eq!(from_browser("FR"), Lang::Fr);
        assert_eq!(from_browser("en-US"), Lang::En);
        assert_eq!(from_browser("de-DE"), Lang::En);
        assert_eq!(from_browser(""), Lang::En);
    }
    #[test]
    fn stored_values_parse() {
        assert_eq!(parse(Some("fr")), Some(Lang::Fr));
        assert_eq!(parse(Some("en")), Some(Lang::En));
        assert_eq!(parse(Some("xx")), None);
        assert_eq!(parse(None), None);
    }
    #[test]
    fn shell_copy() {
        assert_eq!(text(Lang::Fr, Key::Overview), "Vue d'ensemble");
        assert_eq!(text(Lang::Fr, Key::Online), "La maison est connectée");
        assert_eq!(unseen_alerts(Lang::Fr, "3"), "3 alertes non lues");
        assert_eq!(unseen_alerts(Lang::En, "3"), "3 unseen alerts");
        assert_eq!(unseen_alerts(Lang::En, "1"), "1 unseen alert");
        assert_eq!(unseen_alerts(Lang::Fr, "1"), "1 alerte non lue");
        assert_eq!(unseen_alerts(Lang::En, "99+"), "99+ unseen alerts");
        assert_eq!(unseen_alerts(Lang::Fr, "99+"), "99+ alertes non lues");
    }
    #[test]
    fn toggles() {
        assert_eq!(Lang::En.next(), Lang::Fr);
        assert_eq!(Lang::Fr.next(), Lang::En);
        assert_eq!(Lang::Fr.as_str(), "fr");
    }
    /// Adding a `Key` variant without listing it here fails to compile.
    fn listed(k: Key) -> bool {
        match k {
            Key::MyHome
            | Key::Overview
            | Key::House
            | Key::Automations
            | Key::Groups
            | Key::MyRooms
            | Key::HomeSection
            | Key::Energy
            | Key::Security
            | Key::Climate
            | Key::Beta
            | Key::Alerts
            | Key::Settings
            | Key::Devices
            | Key::NewDashboard
            | Key::NewFlow
            | Key::NewGroup
            | Key::EditGroup
            | Key::History
            | Key::Alert
            | Key::Online
            | Key::Reconnecting
            | Key::Offline
            | Key::OpenMenu
            | Key::CloseMenu
            | Key::ChangeTheme
            | Key::ChangeLanguage
            | Key::Navigation
            | Key::Breadcrumb
            | Key::RoomGone => Key::ALL.contains(&k),
        }
    }
    #[test]
    fn all_is_complete() {
        for &k in Key::ALL {
            assert!(listed(k));
        }
        assert_eq!(Key::ALL.len(), 30);
    }
}
