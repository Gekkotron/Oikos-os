//! FR/EN strings for the shell: language choice, storage and the text table.

use chrono::{Datelike, NaiveDate, NaiveTime, Timelike, Weekday};
use leptos::prelude::*;

use oikos_api::WeatherCondition;

use crate::home_header::mood::{Activity, Detail, Greeting, TempBand};
use crate::home_header::summary::HeatingChip;
use crate::home_header::weather_fmt::UvLevel;

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

/// The long date, weekday first: "Vendredi 23 octobre" / "Friday 23 October".
pub fn home_date(lang: Lang, date: NaiveDate) -> String {
    let (weekdays, months) = match lang {
        Lang::Fr => (
            [
                "Lundi", "Mardi", "Mercredi", "Jeudi", "Vendredi", "Samedi", "Dimanche",
            ],
            [
                "janvier",
                "février",
                "mars",
                "avril",
                "mai",
                "juin",
                "juillet",
                "août",
                "septembre",
                "octobre",
                "novembre",
                "décembre",
            ],
        ),
        Lang::En => (
            [
                "Monday",
                "Tuesday",
                "Wednesday",
                "Thursday",
                "Friday",
                "Saturday",
                "Sunday",
            ],
            [
                "January",
                "February",
                "March",
                "April",
                "May",
                "June",
                "July",
                "August",
                "September",
                "October",
                "November",
                "December",
            ],
        ),
    };
    let weekday = match date.weekday() {
        Weekday::Mon => 0,
        Weekday::Tue => 1,
        Weekday::Wed => 2,
        Weekday::Thu => 3,
        Weekday::Fri => 4,
        Weekday::Sat => 5,
        Weekday::Sun => 6,
    };
    format!(
        "{} {} {}",
        weekdays[weekday],
        date.day(),
        months[date.month0() as usize]
    )
}

/// A clock time: "18 h 42" / "6:42 pm".
pub fn home_time(lang: Lang, t: NaiveTime) -> String {
    match lang {
        Lang::Fr => format!("{} h {:02}", t.hour(), t.minute()),
        Lang::En => {
            let (_, hour) = t.hour12();
            let suffix = if t.hour() < 12 { "am" } else { "pm" };
            format!("{hour}:{:02} {suffix}", t.minute())
        }
    }
}

/// A number with `decimals` places and the language's decimal separator.
pub fn home_number(lang: Lang, v: f64, decimals: usize) -> String {
    let s = format!("{v:.decimals$}");
    match lang {
        Lang::Fr => s.replace('.', ","),
        Lang::En => s,
    }
}

/// The greeting for a band of the day.
pub fn greeting_text(lang: Lang, g: Greeting) -> &'static str {
    match (lang, g) {
        (Lang::Fr, Greeting::Morning) => "Bonjour",
        (Lang::Fr, Greeting::Afternoon) => "Bon après-midi",
        (Lang::Fr, Greeting::Evening) => "Bonsoir",
        (Lang::En, Greeting::Morning) => "Good morning",
        (Lang::En, Greeting::Afternoon) => "Good afternoon",
        (Lang::En, Greeting::Evening) => "Good evening",
    }
}

/// The mood sentence, ending with a full stop; the temperature half is left out without a band.
pub fn mood_sentence(lang: Lang, a: &Activity, band: Option<TempBand>) -> String {
    let fr = lang == Lang::Fr;
    let activity = match a {
        Activity::Attention if fr => "Quelque chose demande ton attention".to_owned(),
        Activity::Attention => "Something needs your attention".to_owned(),
        Activity::Open(o) => {
            let label = if fr { "Ouverture" } else { "Open" };
            let space = if fr { " : " } else { ": " };
            match &o.room {
                Some(room) => format!("{label}{space}{} ({room})", o.title),
                None => format!("{label}{space}{}", o.title),
            }
        }
        Activity::OpenMany(n) if fr => format!("{n} ouvertures"),
        Activity::OpenMany(n) => format!("{n} doors or windows open"),
        Activity::Asleep if fr => "La maison dort".to_owned(),
        Activity::Asleep => "The house is asleep".to_owned(),
        Activity::Quiet if fr => "La maison est calme".to_owned(),
        Activity::Quiet => "The house is quiet".to_owned(),
        Activity::Awake if fr => "La maison est éveillée".to_owned(),
        Activity::Awake => "The house is awake".to_owned(),
    };
    let temperature = match (lang, band) {
        (_, None) => "",
        (Lang::Fr, Some(TempBand::Cold)) => ", il fait froid",
        (Lang::Fr, Some(TempBand::Comfortable)) => ", il fait bon",
        (Lang::Fr, Some(TempBand::Warm)) => ", il fait chaud",
        (Lang::En, Some(TempBand::Cold)) => ", it's cold",
        (Lang::En, Some(TempBand::Comfortable)) => ", it's comfortable",
        (Lang::En, Some(TempBand::Warm)) => ", it's warm",
    };
    format!("{activity}{temperature}.")
}

/// One fragment of the detail line.
pub fn detail_text(lang: Lang, d: &Detail) -> String {
    match (lang, d) {
        (Lang::Fr, Detail::RainSoon) => "Pluie attendue dans l'heure".to_owned(),
        (Lang::En, Detail::RainSoon) => "Rain expected within the hour".to_owned(),
        (Lang::Fr, Detail::FrostTonight) => "Gel cette nuit".to_owned(),
        (Lang::En, Detail::FrostTonight) => "Frost tonight".to_owned(),
        (Lang::Fr, Detail::Sunset(at)) => format!("Coucher du soleil à {}", home_time(lang, *at)),
        (Lang::En, Detail::Sunset(at)) => format!("Sunset at {}", home_time(lang, *at)),
        (Lang::Fr, Detail::SunsetFallback) => {
            "Coucher du soleil : position de la maison non réglée".to_owned()
        }
        (Lang::En, Detail::SunsetFallback) => "Sunset: house location not set".to_owned(),
    }
}

/// The lights chip: "3 lights on" / "3 lumières allumées"; `short` drops the "on" part.
pub fn lights_chip(lang: Lang, n: usize, short: bool) -> String {
    match (lang, n == 1, short) {
        (Lang::Fr, true, true) => format!("{n} lumière"),
        (Lang::Fr, false, true) => format!("{n} lumières"),
        (Lang::Fr, true, false) => format!("{n} lumière allumée"),
        (Lang::Fr, false, false) => format!("{n} lumières allumées"),
        (Lang::En, true, true) => format!("{n} light"),
        (Lang::En, false, true) => format!("{n} lights"),
        (Lang::En, true, false) => format!("{n} light on"),
        (Lang::En, false, false) => format!("{n} lights on"),
    }
}

/// The indoor chip: "19.6 °C inside" / "19,6 °C à l'intérieur"; `short` is the temperature alone.
pub fn indoor_chip(lang: Lang, t: f64, short: bool) -> String {
    let value = format!("{} °C", home_number(lang, t, 1));
    match (lang, short) {
        (_, true) => value,
        (Lang::Fr, false) => format!("{value} à l'intérieur"),
        (Lang::En, false) => format!("{value} inside"),
    }
}

/// The heating chip; `short` is one word.
pub fn heating_chip_text(lang: Lang, h: HeatingChip, short: bool) -> &'static str {
    match (lang, h, short) {
        (Lang::Fr, HeatingChip::Off, false) => "Chauffage éteint",
        (Lang::Fr, HeatingChip::Off, true) => "Éteint",
        (Lang::Fr, HeatingChip::Eco, false) => "Chauffage en éco",
        (Lang::Fr, HeatingChip::Eco, true) => "Éco",
        (Lang::Fr, HeatingChip::Cooling, false) => "Clim en marche",
        (Lang::Fr, HeatingChip::Cooling, true) => "Clim",
        (Lang::En, HeatingChip::Off, false) => "Heating off",
        (Lang::En, HeatingChip::Off, true) => "Off",
        (Lang::En, HeatingChip::Eco, false) => "Heating in eco",
        (Lang::En, HeatingChip::Eco, true) => "Eco",
        (Lang::En, HeatingChip::Cooling, false) => "Air conditioning on",
        (Lang::En, HeatingChip::Cooling, true) => "A/C",
    }
}

/// The accessible name of the house mode menu.
pub fn house_mode_label(lang: Lang) -> &'static str {
    match lang {
        Lang::Fr => "Mode de la maison",
        Lang::En => "House mode",
    }
}

/// The weather card's accessible name.
pub fn weather_link_label(lang: Lang) -> &'static str {
    match lang {
        Lang::Fr => "Voir la météo",
        Lang::En => "Open the weather",
    }
}

/// "feels like 5°" after the condition, on the wide card.
pub fn feels_like(lang: Lang, deg: &str) -> String {
    match lang {
        Lang::Fr => format!("ressenti {deg}"),
        Lang::En => format!("feels like {deg}"),
    }
}

/// Today's low and high on the wide card: "Today 9° / 14°".
pub fn today_range(lang: Lang, range: &str) -> String {
    match lang {
        Lang::Fr => format!("Aujourd'hui {range}"),
        Lang::En => format!("Today {range}"),
    }
}

/// The sunset line of the card; `short` is "Sunset 6:42 pm" / "Coucher 18 h 42".
pub fn card_sunset(lang: Lang, at: NaiveTime, short: bool) -> String {
    let time = home_time(lang, at);
    match (lang, short) {
        (Lang::Fr, false) => format!("Coucher du soleil {time}"),
        (Lang::Fr, true) => format!("Coucher {time}"),
        (Lang::En, _) => format!("Sunset {time}"),
    }
}

/// The muted line of a card whose data is late.
pub fn data_from(lang: Lang, at: NaiveTime) -> String {
    match lang {
        Lang::Fr => format!("Données de {}", home_time(lang, at)),
        Lang::En => format!("Data from {}", home_time(lang, at)),
    }
}

/// The label of a weather condition.
pub fn condition_label(lang: Lang, c: WeatherCondition) -> &'static str {
    use WeatherCondition::*;
    match (lang, c) {
        (Lang::Fr, Clear) => "Dégagé",
        (Lang::Fr, PartlyCloudy) => "Éclaircies",
        (Lang::Fr, Cloudy) => "Nuageux",
        (Lang::Fr, Fog) => "Brouillard",
        (Lang::Fr, Drizzle) => "Bruine",
        (Lang::Fr, Rain) => "Pluie",
        (Lang::Fr, FreezingRain) => "Pluie verglaçante",
        (Lang::Fr, Snow) => "Neige",
        (Lang::Fr, Showers) => "Averses",
        (Lang::Fr, Thunderstorm) => "Orage",
        (Lang::En, Clear) => "Clear",
        (Lang::En, PartlyCloudy) => "Partly cloudy",
        (Lang::En, Cloudy) => "Cloudy",
        (Lang::En, Fog) => "Fog",
        (Lang::En, Drizzle) => "Drizzle",
        (Lang::En, Rain) => "Rain",
        (Lang::En, FreezingRain) => "Freezing rain",
        (Lang::En, Snow) => "Snow",
        (Lang::En, Showers) => "Showers",
        (Lang::En, Thunderstorm) => "Thunderstorm",
    }
}

/// The compass point for an index from `compass_index` (0 = N, clockwise).
pub fn compass_word(lang: Lang, index: usize) -> &'static str {
    const EN: [&str; 8] = ["N", "NE", "E", "SE", "S", "SW", "W", "NW"];
    const FR: [&str; 8] = ["N", "NE", "E", "SE", "S", "SO", "O", "NO"];
    let table = if lang == Lang::Fr { FR } else { EN };
    table[index % 8]
}

/// The word for a UV band.
pub fn uv_word(lang: Lang, level: UvLevel) -> &'static str {
    match (lang, level) {
        (Lang::Fr, UvLevel::Low) => "Faible",
        (Lang::Fr, UvLevel::Moderate) => "Modéré",
        (Lang::Fr, UvLevel::High) => "Élevé",
        (Lang::Fr, UvLevel::VeryHigh) => "Très élevé",
        (Lang::Fr, UvLevel::Extreme) => "Extrême",
        (Lang::En, UvLevel::Low) => "Low",
        (Lang::En, UvLevel::Moderate) => "Moderate",
        (Lang::En, UvLevel::High) => "High",
        (Lang::En, UvLevel::VeryHigh) => "Very high",
        (Lang::En, UvLevel::Extreme) => "Extreme",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::home_header::summary::Opening;
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
    fn window(room: Option<&str>) -> Activity {
        Activity::Open(Opening {
            title: "Fenêtre".into(),
            room: room.map(Into::into),
        })
    }
    #[test]
    fn mood_sentence_fr_en() {
        use TempBand::*;
        let fr = |a: &Activity, b| mood_sentence(Lang::Fr, a, Some(b));
        assert_eq!(
            fr(&Activity::Quiet, Comfortable),
            "La maison est calme, il fait bon."
        );
        assert_eq!(
            fr(&Activity::Asleep, Cold),
            "La maison dort, il fait froid."
        );
        assert_eq!(
            fr(&window(Some("Cuisine")), Cold),
            "Ouverture : Fenêtre (Cuisine), il fait froid."
        );
        assert_eq!(
            fr(&window(None), Cold),
            "Ouverture : Fenêtre, il fait froid."
        );
        assert_eq!(
            fr(&Activity::OpenMany(2), Warm),
            "2 ouvertures, il fait chaud."
        );
        assert_eq!(
            fr(&Activity::Attention, Comfortable),
            "Quelque chose demande ton attention, il fait bon."
        );
        assert_eq!(
            fr(&Activity::Awake, Warm),
            "La maison est éveillée, il fait chaud."
        );
        let en = |a: &Activity, b| mood_sentence(Lang::En, a, Some(b));
        assert_eq!(
            en(&Activity::Quiet, Comfortable),
            "The house is quiet, it's comfortable."
        );
        assert_eq!(
            en(&Activity::Asleep, Cold),
            "The house is asleep, it's cold."
        );
        assert_eq!(en(&Activity::Awake, Warm), "The house is awake, it's warm.");
        assert_eq!(
            en(&Activity::OpenMany(2), Cold),
            "2 doors or windows open, it's cold."
        );
        assert_eq!(
            en(&window(Some("Kitchen")), Cold),
            "Open: Fenêtre (Kitchen), it's cold."
        );
        assert_eq!(
            en(&Activity::Attention, Cold),
            "Something needs your attention, it's cold."
        );
    }
    #[test]
    fn sentence_without_temperature() {
        assert_eq!(
            mood_sentence(Lang::Fr, &Activity::Quiet, None),
            "La maison est calme."
        );
        assert_eq!(
            mood_sentence(Lang::En, &Activity::Quiet, None),
            "The house is quiet."
        );
    }
    #[test]
    fn home_date_fr_en() {
        let d = NaiveDate::from_ymd_opt(2026, 10, 23).unwrap();
        assert_eq!(home_date(Lang::Fr, d), "Vendredi 23 octobre");
        assert_eq!(home_date(Lang::En, d), "Friday 23 October");
    }
    #[test]
    fn home_time_fr_en() {
        let t = |h, m| NaiveTime::from_hms_opt(h, m, 0).unwrap();
        assert_eq!(home_time(Lang::Fr, t(18, 42)), "18 h 42");
        assert_eq!(home_time(Lang::En, t(18, 42)), "6:42 pm");
        assert_eq!(home_time(Lang::Fr, t(9, 5)), "9 h 05");
        assert_eq!(home_time(Lang::En, t(0, 7)), "12:07 am");
        assert_eq!(home_time(Lang::En, t(12, 0)), "12:00 pm");
    }
    #[test]
    fn home_number_follows_language() {
        assert_eq!(home_number(Lang::Fr, 19.64, 1), "19,6");
        assert_eq!(home_number(Lang::En, 19.64, 1), "19.6");
        assert_eq!(home_number(Lang::Fr, 5.0, 0), "5");
    }
    #[test]
    fn greetings_fr_en() {
        assert_eq!(greeting_text(Lang::Fr, Greeting::Morning), "Bonjour");
        assert_eq!(
            greeting_text(Lang::Fr, Greeting::Afternoon),
            "Bon après-midi"
        );
        assert_eq!(greeting_text(Lang::Fr, Greeting::Evening), "Bonsoir");
        assert_eq!(greeting_text(Lang::En, Greeting::Morning), "Good morning");
        assert_eq!(
            greeting_text(Lang::En, Greeting::Afternoon),
            "Good afternoon"
        );
        assert_eq!(greeting_text(Lang::En, Greeting::Evening), "Good evening");
    }
    #[test]
    fn detail_text_fr() {
        let sunset = NaiveTime::from_hms_opt(18, 42, 0).unwrap();
        assert_eq!(
            detail_text(Lang::Fr, &Detail::RainSoon),
            "Pluie attendue dans l'heure"
        );
        assert_eq!(
            detail_text(Lang::Fr, &Detail::FrostTonight),
            "Gel cette nuit"
        );
        assert_eq!(
            detail_text(Lang::Fr, &Detail::Sunset(sunset)),
            "Coucher du soleil à 18 h 42"
        );
        assert_eq!(
            detail_text(Lang::Fr, &Detail::SunsetFallback),
            "Coucher du soleil : position de la maison non réglée"
        );
    }
    #[test]
    fn detail_text_en() {
        let sunset = NaiveTime::from_hms_opt(18, 42, 0).unwrap();
        assert_eq!(
            detail_text(Lang::En, &Detail::RainSoon),
            "Rain expected within the hour"
        );
        assert_eq!(
            detail_text(Lang::En, &Detail::FrostTonight),
            "Frost tonight"
        );
        assert_eq!(
            detail_text(Lang::En, &Detail::Sunset(sunset)),
            "Sunset at 6:42 pm"
        );
        assert_eq!(
            detail_text(Lang::En, &Detail::SunsetFallback),
            "Sunset: house location not set"
        );
    }
}
