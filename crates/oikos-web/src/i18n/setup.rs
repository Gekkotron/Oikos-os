//! FR/EN text: the setup wizard.

use super::*;

/// Fixed words of the setup wizard. Later steps add their own entries.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SetupText {
    /// The overlay shown while the hub restarts to apply a save.
    Applying,
    /// The overlay's note when the hub did not answer again in time.
    ApplyFailed,
    /// The overlay's button that waits for the hub again.
    Retry,
    /// The accessible name of the wizard's progress line.
    Progress,
    StepWelcome,
    StepMqtt,
    StepDatabase,
    StepDevices,
    StepServices,
    StepDone,
}

pub fn setup_text(lang: Lang, k: SetupText) -> &'static str {
    let (en, fr) = match k {
        SetupText::Applying => ("Applying…", "Application…"),
        SetupText::ApplyFailed => (
            "The hub did not come back. Check its log.",
            "Le hub n'est pas revenu. Consultez son journal.",
        ),
        SetupText::Retry => ("Retry", "Réessayer"),
        SetupText::Progress => ("Setup progress", "Progression de la configuration"),
        SetupText::StepWelcome => ("Welcome", "Bienvenue"),
        SetupText::StepMqtt => ("MQTT broker", "Broker MQTT"),
        SetupText::StepDatabase => ("Database", "Base de données"),
        SetupText::StepDevices => ("Devices", "Appareils"),
        SetupText::StepServices => ("Services", "Services"),
        SetupText::StepDone => ("Done", "Terminé"),
    };
    match lang {
        Lang::En => en,
        Lang::Fr => fr,
    }
}

/// A translator for [`SetupText`] bound to the language in context (reactive).
pub fn use_setup_text() -> impl Fn(SetupText) -> &'static str + Copy + 'static {
    let lang = use_lang();
    move |k| setup_text(lang(), k)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_entry_is_translated() {
        assert_table(
            &[
                SetupText::Applying,
                SetupText::ApplyFailed,
                SetupText::Retry,
                SetupText::Progress,
                SetupText::StepWelcome,
                SetupText::StepMqtt,
                SetupText::StepDatabase,
                SetupText::StepDevices,
                SetupText::StepServices,
                SetupText::StepDone,
            ],
            setup_text,
            &[SetupText::StepServices],
        );
    }
}
