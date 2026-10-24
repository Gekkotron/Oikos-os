//! FR/EN text: Dashboard widgets and group states.

use super::*;

/// What a group card or a room widget says about its members.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GroupState {
    /// No member is online.
    Unavailable,
    Opening,
    Closing,
    AllOpen,
    AllClosed,
    /// Covers that disagree.
    Covers {
        open: usize,
        closed: usize,
    },
    AllOn,
    AllOff,
    /// Lights or switches that disagree.
    SomeOn {
        on: usize,
        total: usize,
    },
}

/// The text of a [`GroupState`]: "All closed" / "Tout fermé", "2 of 3 on" / "2 sur 3 allumés".
pub fn group_state(lang: Lang, s: GroupState) -> String {
    // French plural: 0 and 1 are singular.
    let fr_plural =
        |n: usize, one: &str, many: &str| format!("{n} {}", if n > 1 { many } else { one });
    match (lang, s) {
        (Lang::En, GroupState::Unavailable) => "Unavailable".into(),
        (Lang::Fr, GroupState::Unavailable) => "Indisponible".into(),
        (Lang::En, GroupState::Opening) => "Opening…".into(),
        (Lang::Fr, GroupState::Opening) => "Ouverture…".into(),
        (Lang::En, GroupState::Closing) => "Closing…".into(),
        (Lang::Fr, GroupState::Closing) => "Fermeture…".into(),
        (Lang::En, GroupState::AllOpen) => "All open".into(),
        (Lang::Fr, GroupState::AllOpen) => "Tout ouvert".into(),
        (Lang::En, GroupState::AllClosed) => "All closed".into(),
        (Lang::Fr, GroupState::AllClosed) => "Tout fermé".into(),
        (Lang::En, GroupState::Covers { open, closed }) => format!("{open} open · {closed} closed"),
        (Lang::Fr, GroupState::Covers { open, closed }) => format!(
            "{} · {}",
            fr_plural(open, "ouvert", "ouverts"),
            fr_plural(closed, "fermé", "fermés")
        ),
        (Lang::En, GroupState::AllOn) => "All on".into(),
        (Lang::Fr, GroupState::AllOn) => "Tout allumé".into(),
        (Lang::En, GroupState::AllOff) => "All off".into(),
        (Lang::Fr, GroupState::AllOff) => "Tout éteint".into(),
        (Lang::En, GroupState::SomeOn { on, total }) => format!("{on} of {total} on"),
        (Lang::Fr, GroupState::SomeOn { on, total }) => {
            format!(
                "{on} sur {total} {}",
                if on > 1 { "allumés" } else { "allumé" }
            )
        }
    }
}

/// Text of the dashboard widgets that have their own card.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WidgetText {
    /// The flow button.
    Run,
    /// A room with neither lights nor covers.
    NoLightsOrCovers,
    // Energy stat labels: short enough for a 1x1 card, the value carries the unit.
    Today,
    ThisWeek,
    ThisMonth,
    GridPowerNow,
    SolarPowerNow,
    SelfConsumption,
}

pub fn widget_text(lang: Lang, t: WidgetText) -> &'static str {
    let (en, fr) = match t {
        WidgetText::Run => ("Run", "Lancer"),
        WidgetText::NoLightsOrCovers => ("No lights or covers", "Ni lumière ni volet"),
        WidgetText::Today => ("Today", "Aujourd'hui"),
        WidgetText::ThisWeek => ("Last 7 days", "7 derniers jours"),
        WidgetText::ThisMonth => ("Last 30 days", "30 derniers jours"),
        WidgetText::GridPowerNow => ("Grid now", "Réseau"),
        WidgetText::SolarPowerNow => ("Solar now", "Solaire"),
        WidgetText::SelfConsumption => ("Self-use", "Autoconso"),
    };
    match lang {
        Lang::En => en,
        Lang::Fr => fr,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn group_states_in_french_agree_in_number() {
        assert_eq!(group_state(Lang::Fr, GroupState::AllClosed), "Tout fermé");
        assert_eq!(
            group_state(Lang::Fr, GroupState::Covers { open: 1, closed: 2 }),
            "1 ouvert · 2 fermés"
        );
        assert_eq!(
            group_state(Lang::En, GroupState::Covers { open: 1, closed: 2 }),
            "1 open · 2 closed"
        );
        assert_eq!(
            group_state(Lang::Fr, GroupState::Unavailable),
            "Indisponible"
        );
    }
    #[test]
    fn every_dashboards_text_has_two_languages() {
        const ALL: &[WidgetText] = &[
            WidgetText::Run,
            WidgetText::NoLightsOrCovers,
            WidgetText::Today,
            WidgetText::ThisWeek,
            WidgetText::ThisMonth,
            WidgetText::GridPowerNow,
            WidgetText::SolarPowerNow,
            WidgetText::SelfConsumption,
        ];
        crate::i18n::assert_table(ALL, widget_text, &[]);
    }
}
