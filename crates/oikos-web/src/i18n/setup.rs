//! FR/EN text: the setup wizard.

use super::*;

/// Fixed words of the setup wizard. Later steps add their own entries.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SetupText {
    /// The overlay shown while the hub restarts to apply a save.
    Applying,
}

pub fn setup_text(lang: Lang, k: SetupText) -> &'static str {
    let (en, fr) = match k {
        SetupText::Applying => ("Applying…", "Application…"),
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
        assert_table(&[SetupText::Applying], setup_text, &[]);
    }
}
