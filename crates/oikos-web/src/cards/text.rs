//! The words the cards show, in the chosen language: state lines, numbers with the right
//! decimal separator and the like. Pure, so they are tested off the browser.

use crate::i18n::{Key, Lang, text};

/// Shown for a value nobody knows.
const DASH: &str = "—";

/// A number with at most one decimal, none when it rounds to a whole number, with the language's
/// decimal separator: `15,5` in French, `15.5` in English.
pub fn decimal(lang: Lang, v: f64) -> String {
    let tenths = (v * 10.0).round() / 10.0;
    let out = if tenths.fract() == 0.0 {
        format!("{tenths:.0}")
    } else {
        format!("{tenths:.1}")
    };
    match lang {
        Lang::Fr => out.replace('.', ","),
        Lang::En => out,
    }
}

/// A light's 0–255 brightness as a percentage; never 0 while the light gives any light.
pub fn brightness_percent(level: u8) -> u8 {
    let percent = (f64::from(level) * 100.0 / 255.0).round() as u8;
    if level > 0 { percent.max(1) } else { 0 }
}

/// "On · 14 %" / "Allumé · 14 %", "Off" / "Éteint", "—" while unknown.
pub fn light_state_line(lang: Lang, on: Option<bool>, brightness: Option<u8>) -> String {
    match (on, brightness) {
        (Some(true), Some(level)) => format!(
            "{} · {} %",
            text(lang, Key::LightOn),
            brightness_percent(level)
        ),
        (Some(true), None) => text(lang, Key::LightOn).to_owned(),
        (Some(false), _) => text(lang, Key::LightOff).to_owned(),
        (None, _) => DASH.to_owned(),
    }
}

/// "On" / "Activé", "Off" / "Désactivé", "—" while unknown.
pub fn switch_state_line(lang: Lang, on: Option<bool>) -> String {
    match on {
        Some(true) => text(lang, Key::SwitchOn).to_owned(),
        Some(false) => text(lang, Key::SwitchOff).to_owned(),
        None => DASH.to_owned(),
    }
}

/// After this long without a state message a sensor's badge stops saying "Live".
pub const QUIET_AFTER_S: u64 = 3 * 3600;

/// How fresh a sensor's reading is, from the unix time of its last state message: "Live" while
/// recent, then "3 h ago", then "2 d ago"; `None` when the hub never said.
pub fn freshness(lang: Lang, now: u64, last_update: Option<u64>) -> Option<String> {
    let age = now.saturating_sub(last_update?);
    let (key, n) = if age < QUIET_AFTER_S {
        return Some(text(lang, Key::Live).to_owned());
    } else if age < 86_400 {
        (Key::AgoHours, age / 3600)
    } else {
        (Key::AgoDays, age / 86_400)
    };
    Some(text(lang, key).replace("{n}", &n.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn freshness_cases() {
        let now = 1_000_000;
        assert_eq!(freshness(Lang::En, now, None), None);
        assert_eq!(freshness(Lang::En, now, Some(now)).as_deref(), Some("Live"));
        assert_eq!(
            freshness(Lang::En, now, Some(now - 10_799)).as_deref(),
            Some("Live")
        );
        assert_eq!(
            freshness(Lang::En, now, Some(now - 10_800)).as_deref(),
            Some("3 h ago")
        );
        assert_eq!(
            freshness(Lang::En, now, Some(now - 86_399)).as_deref(),
            Some("23 h ago")
        );
        assert_eq!(
            freshness(Lang::En, now, Some(now - 86_400)).as_deref(),
            Some("1 d ago")
        );
        assert_eq!(
            freshness(Lang::En, now, Some(now + 60)).as_deref(),
            Some("Live")
        );
        assert_eq!(
            freshness(Lang::Fr, now, Some(now - 10_800)).as_deref(),
            Some("Il y a 3 h")
        );
        assert_eq!(
            freshness(Lang::Fr, now, Some(now - 2 * 86_400)).as_deref(),
            Some("Il y a 2 j")
        );
        assert_eq!(
            freshness(Lang::Fr, now, Some(now)).as_deref(),
            Some("En direct")
        );
    }

    #[test]
    fn decimal_follows_language() {
        assert_eq!(decimal(Lang::Fr, 15.5), "15,5");
        assert_eq!(decimal(Lang::En, 15.5), "15.5");
        assert_eq!(decimal(Lang::Fr, 20.0), "20");
        assert_eq!(decimal(Lang::Fr, 20.66), "20,7");
        assert_eq!(decimal(Lang::En, 19.97), "20");
    }

    #[test]
    fn brightness_percent_rounds() {
        assert_eq!(brightness_percent(0), 0);
        assert_eq!(brightness_percent(1), 1);
        assert_eq!(brightness_percent(36), 14);
        assert_eq!(brightness_percent(255), 100);
    }

    #[test]
    fn light_state_lines() {
        assert_eq!(
            light_state_line(Lang::Fr, Some(true), Some(36)),
            "Allumé · 14 %"
        );
        assert_eq!(light_state_line(Lang::Fr, Some(true), None), "Allumé");
        assert_eq!(light_state_line(Lang::Fr, Some(false), Some(36)), "Éteint");
        assert_eq!(light_state_line(Lang::Fr, None, None), "—");
        assert_eq!(
            light_state_line(Lang::En, Some(true), Some(255)),
            "On · 100 %"
        );
        assert_eq!(light_state_line(Lang::En, Some(false), None), "Off");
    }

    #[test]
    fn switch_state_lines() {
        assert_eq!(switch_state_line(Lang::Fr, Some(true)), "Activé");
        assert_eq!(switch_state_line(Lang::Fr, Some(false)), "Désactivé");
        assert_eq!(switch_state_line(Lang::En, Some(true)), "On");
        assert_eq!(switch_state_line(Lang::En, None), "—");
    }
}
