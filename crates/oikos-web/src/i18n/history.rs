//! FR/EN text: the history chart and the history page.

use super::*;
use oikos_api::HistoryRange;

const NBSP: char = '\u{A0}';

/// Fixed words of the history chart and page.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HistoryText {
    /// The last x-axis label.
    Now,
    NotRecorded,
    Unavailable,
    CouldNotLoad,
    Retry,
    UnknownEntity,
    /// Chart legend: "Last 24 hours".
    Last24h,
    Last7d,
    Last30d,
    LastYear,
    /// Y-axis of a chart that shows a cover position.
    Min,
    Max,
}

pub fn history_text(lang: Lang, k: HistoryText) -> &'static str {
    use HistoryText as T;
    let (en, fr) = match k {
        T::Now => ("now", "maintenant"),
        T::NotRecorded => (
            "No data recorded for this entity yet. The hub starts recording from the moment the entity first appears.",
            "Aucune donnée enregistrée pour cette entité pour l'instant. Le hub commence à enregistrer dès l'apparition de l'entité.",
        ),
        T::Unavailable => (
            "History is unavailable right now. The database may be down.",
            "L'historique est indisponible pour le moment. La base de données est peut-être arrêtée.",
        ),
        T::CouldNotLoad => (
            "Could not load history",
            "Impossible de charger l'historique",
        ),
        T::Retry => ("Retry", "Réessayer"),
        T::UnknownEntity => ("Unknown entity", "Entité inconnue"),
        T::Last24h => ("Last 24 hours", "24 dernières heures"),
        T::Last7d => ("Last 7 days", "7 derniers jours"),
        T::Last30d => ("Last 30 days", "30 derniers jours"),
        T::LastYear => ("Last year", "Dernière année"),
        T::Min => ("min", "min"),
        T::Max => ("max", "max"),
    };
    match lang {
        Lang::En => en,
        Lang::Fr => fr,
    }
}

/// The legend name of a chart range: "Last 24 hours" / "24 dernières heures".
pub fn range_name(lang: Lang, r: HistoryRange) -> &'static str {
    history_text(
        lang,
        match r {
            HistoryRange::Day => HistoryText::Last24h,
            HistoryRange::Week => HistoryText::Last7d,
            HistoryRange::Month => HistoryText::Last30d,
            HistoryRange::Year => HistoryText::LastYear,
        },
    )
}

/// The range chip of the history page: "24h" / "24 h", "7d" / "7 j", "30d" / "30 j", "1y" / "1 an".
pub fn range_chip(lang: Lang, r: HistoryRange) -> String {
    match (lang, r) {
        (Lang::En, HistoryRange::Day) => "24h".into(),
        (Lang::En, HistoryRange::Week) => "7d".into(),
        (Lang::En, HistoryRange::Month) => "30d".into(),
        (Lang::En, HistoryRange::Year) => "1y".into(),
        (Lang::Fr, HistoryRange::Day) => format!("24{NBSP}h"),
        (Lang::Fr, HistoryRange::Week) => format!("7{NBSP}j"),
        (Lang::Fr, HistoryRange::Month) => format!("30{NBSP}j"),
        (Lang::Fr, HistoryRange::Year) => format!("1{NBSP}an"),
    }
}

/// "Could not load history: boom" / "Impossible de charger l'historique : boom"; the reason is
/// the hub's own text.
pub fn load_error(lang: Lang, reason: &str) -> String {
    let lead = history_text(lang, HistoryText::CouldNotLoad);
    match lang {
        Lang::En => format!("{lead}: {reason}"),
        Lang::Fr => format!("{lead}{NBSP}: {reason}"),
    }
}

/// The legend's right side: "min 3.0 °C · max 21.5 °C" (French decimals and spacing).
pub fn min_max(lang: Lang, min: f64, max: f64, unit: Option<&str>) -> String {
    let cell = |v: f64| format::with_unit(format::decimal(lang, v, 1), unit);
    format!(
        "{} {} · {} {}",
        history_text(lang, HistoryText::Min),
        cell(min),
        history_text(lang, HistoryText::Max),
        cell(max)
    )
}

/// "Last 24 hours total: 4.2 kWh" / "24 dernières heures, total : 4,2 kWh".
pub fn total_line(lang: Lang, r: HistoryRange, total: f64) -> String {
    let value = format::with_unit(format::decimal(lang, total, 1), Some("kWh"));
    match lang {
        Lang::En => format!("{} total: {value}", range_name(lang, r)),
        Lang::Fr => format!("{}, total{NBSP}: {value}", range_name(lang, r)),
    }
}

/// "Last 24 hours · daily total" / "24 dernières heures · total quotidien".
pub fn counter_legend(lang: Lang, r: HistoryRange) -> String {
    let tail = match lang {
        Lang::En => "daily total",
        Lang::Fr => "total quotidien",
    };
    format!("{} · {tail}", range_name(lang, r))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_history_text_has_two_languages() {
        const ALL: &[HistoryText] = &[
            HistoryText::Now,
            HistoryText::NotRecorded,
            HistoryText::Unavailable,
            HistoryText::CouldNotLoad,
            HistoryText::Retry,
            HistoryText::UnknownEntity,
            HistoryText::Last24h,
            HistoryText::Last7d,
            HistoryText::Last30d,
            HistoryText::LastYear,
            HistoryText::Min,
            HistoryText::Max,
        ];
        crate::i18n::assert_table(ALL, history_text, &[HistoryText::Min, HistoryText::Max]);
    }

    #[test]
    fn legends_and_errors_follow_the_language() {
        assert_eq!(
            min_max(Lang::Fr, 3.0, 21.5, Some("°C")),
            "min 3,0\u{A0}°C · max 21,5\u{A0}°C"
        );
        assert_eq!(
            min_max(Lang::En, 3.0, 21.5, Some("°C")),
            "min 3.0\u{A0}°C · max 21.5\u{A0}°C"
        );
        assert_eq!(
            total_line(Lang::En, HistoryRange::Day, 4.25),
            "Last 24 hours total: 4.2\u{A0}kWh"
        );
        assert_eq!(
            total_line(Lang::Fr, HistoryRange::Week, 4.5),
            "7 derniers jours, total\u{A0}: 4,5\u{A0}kWh"
        );
        assert_eq!(
            load_error(Lang::Fr, "boom"),
            "Impossible de charger l'historique\u{A0}: boom"
        );
        assert_eq!(load_error(Lang::En, "boom"), "Could not load history: boom");
        assert_eq!(range_chip(Lang::En, HistoryRange::Week), "7d");
        assert_eq!(range_chip(Lang::Fr, HistoryRange::Week), "7\u{A0}j");
        assert_eq!(
            counter_legend(Lang::Fr, HistoryRange::Month),
            "30 derniers jours · total quotidien"
        );
    }
}
