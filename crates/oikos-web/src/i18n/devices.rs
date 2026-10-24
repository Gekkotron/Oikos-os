//! FR/EN text: the devices area (device list, entity sheet, override editor and the shared
//! card controls).

use super::*;

/// Fixed words of the devices area. Sentences with values are the helper functions below.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeviceText {
    /// The cancel button of the inline "are you sure?" bar.
    Cancel,
}

impl DeviceText {
    pub const ALL: &'static [DeviceText] = &[DeviceText::Cancel];
}

/// The text of `k` in `lang`.
pub fn device_text(lang: Lang, k: DeviceText) -> &'static str {
    use DeviceText as T;
    let (en, fr) = match k {
        T::Cancel => ("Cancel", "Annuler"),
    };
    match lang {
        Lang::En => en,
        Lang::Fr => fr,
    }
}

/// A translator for [`DeviceText`] bound to the language in context (reactive).
pub fn use_device_text() -> impl Fn(DeviceText) -> &'static str + Copy + 'static {
    let lang = use_lang();
    move |k| device_text(lang(), k)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Same word in both languages.
    const SAME: &[DeviceText] = &[];

    #[test]
    fn every_word_has_both_languages() {
        assert_table(DeviceText::ALL, device_text, SAME);
    }
}
