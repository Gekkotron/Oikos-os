//! The action list of a device and the inputs of each action, as core's
//! [`oikos_core::vocabulary`] lists them from discovery: the flow Command card and the
//! ambiance device action draw the same choices. Only the wording is the web's.

use oikos_api::EntityView;
use oikos_core::command::command_name;
use oikos_core::flow::Scalar;
use oikos_core::model::{Command, Hs};
use oikos_core::vocabulary::{Action, actions};
use serde_json::Value;

use crate::cards::vacuum::action_label as vacuum_action_label;
use crate::format::word;
use crate::i18n::Lang;
use crate::i18n::flows::{
    FlowText, arm_action, control_action, fan_mode_action, fan_speed_action, flow_text,
};
use crate::i18n::settings::{SettingsText, select_option, set_mode, settings_text};
use crate::i18n::states::{StateCtx, on_off, state_word};

/// The actions of `e`; a number entity's value starts at its current reading when it has one.
pub fn entity_actions(e: &EntityView) -> Vec<Action> {
    let mut list = actions(e.kind, &e.component, &e.capabilities);
    if let Some(Scalar::Number(now)) = Scalar::of_state(&e.state.value) {
        for action in &mut list {
            if let Command::SetNumber { value } = &mut action.command {
                *value = now;
            }
        }
    }
    list
}

/// Two commands with the same key are the same choice, possibly with another input value.
pub fn command_key(c: &Command) -> String {
    match c {
        Command::SelectOption { option } => format!("select:{option}"),
        Command::SetHvacMode { mode } => format!("mode:{mode}"),
        Command::SetFanMode { mode } => format!("fan:{mode}"),
        Command::SetFanSpeed { speed } => format!("speed:{speed}"),
        Command::VacuumAction { action } => format!("vacuum:{action}"),
        Command::Arm { mode, .. } => format!("arm:{mode:?}"),
        // An on/off or choice control is one choice per value; a number or text control is
        // one choice whose value is an input.
        Command::Control { control, value } => match value {
            Some(v @ (Value::Bool(_) | Value::String(_))) if !is_text_input(v) => {
                format!("control:{control}:{v}")
            }
            _ => format!("control:{control}"),
        },
        other => command_name(other).to_owned(),
    }
}

/// A text control starts empty: its value is typed, not a choice.
fn is_text_input(v: &Value) -> bool {
    v.as_str() == Some("")
}

/// The label of a command in an action list, in `lang`.
pub fn command_label(lang: Lang, c: &Command) -> String {
    let flow = |k| flow_text(lang, k).to_owned();
    let setting = |k| settings_text(lang, k).to_owned();
    match c {
        Command::TurnOn { .. } => flow(FlowText::TurnOn),
        Command::TurnOff => flow(FlowText::TurnOff),
        Command::Toggle => flow(FlowText::Toggle),
        Command::Open => flow(FlowText::Open),
        Command::Close => flow(FlowText::Close),
        Command::Stop => flow(FlowText::Stop),
        Command::Disarm { .. } => flow(FlowText::Disarm),
        Command::Press => setting(SettingsText::Press),
        Command::SetPosition { .. } => setting(SettingsText::SetPosition),
        Command::SetTemperature { .. } => setting(SettingsText::SetTemperature),
        Command::SetNumber { .. } => setting(SettingsText::SetValue),
        Command::SelectOption { option } => select_option(lang, option),
        Command::SetHvacMode { mode } => set_mode(
            lang,
            &lower_first(lang, &state_word(lang, StateCtx::Hvac, mode)),
        ),
        Command::SetFanMode { mode } => fan_mode_action(lang, &word(mode)),
        Command::SetFanSpeed { speed } => fan_speed_action(lang, &word(speed)),
        Command::VacuumAction { action } => vacuum_action_label(lang, action),
        Command::Arm { mode, .. } => arm_action(lang, *mode),
        Command::Control { control, value } => match (control.as_str(), value) {
            ("locked", Some(Value::Bool(true))) => flow(FlowText::Lock),
            ("locked", Some(Value::Bool(false))) => flow(FlowText::Unlock),
            (_, Some(Value::Bool(on))) => {
                control_action(lang, &word(control), on_off(lang, false, *on))
            }
            (_, Some(v @ Value::String(s))) if !is_text_input(v) => {
                control_action(lang, &word(control), &word(s))
            }
            _ => word(control),
        },
    }
}

/// French mode words are capitalised for display; inside "Régler le mode …" they read lower.
fn lower_first(lang: Lang, s: &str) -> String {
    match lang {
        Lang::En => s.to_owned(),
        Lang::Fr => {
            let mut chars = s.chars();
            chars
                .next()
                .map(|c| c.to_lowercase().chain(chars).collect())
                .unwrap_or_default()
        }
    }
}

/// `actions`, with `current` first when none of them is the same choice, so a saved command
/// the device does not list stays selected rather than replaced.
pub fn command_options(current: &Command, actions: &[Action]) -> Vec<Action> {
    let key = command_key(current);
    let mut list = actions.to_vec();
    if !list.iter().any(|a| command_key(&a.command) == key) {
        list.insert(
            0,
            Action {
                command: current.clone(),
                params: Vec::new(),
            },
        );
    }
    list
}

/// `current` when the entity offers it, else the entity's first action, else `current`.
pub fn retarget(current: &Command, actions: &[Action]) -> Command {
    let key = command_key(current);
    if actions.iter().any(|a| command_key(&a.command) == key) {
        return current.clone();
    }
    actions
        .first()
        .map_or_else(|| current.clone(), |a| a.command.clone())
}

/// The value of the field `key` in the command's JSON.
pub fn param_value(c: &Command, key: &str) -> Option<Value> {
    serde_json::to_value(c)
        .ok()?
        .get(key)
        .filter(|v| !v.is_null())
        .cloned()
}

/// `c` with the field `key` set to `value` (`None` removes it). A result that is not a valid
/// command leaves `c` unchanged.
pub fn with_param(c: &Command, key: &str, value: Option<Value>) -> Command {
    let Ok(Value::Object(mut fields)) = serde_json::to_value(c) else {
        return c.clone();
    };
    match value {
        Some(v) => fields.insert(key.to_owned(), v),
        None => fields.remove(key),
    };
    serde_json::from_value(Value::Object(fields)).unwrap_or_else(|_| c.clone())
}

/// The stored JSON of a number shown as `shown`: scaled by `factor` and rounded when scaled,
/// and an integer when `step` and the result are whole (whole-number fields refuse `40.0`).
pub fn number_json(shown: f64, factor: f64, step: f64) -> Value {
    let stored = if factor == 1.0 {
        shown
    } else {
        (shown * factor).round()
    };
    if step.fract() == 0.0 && stored.fract() == 0.0 {
        Value::from(stored as i64)
    } else {
        Value::from(stored)
    }
}

/// The colours a light can be set to, each reading back as its own name in card titles.
pub const SWATCHES: [(FlowText, Hs); 10] = [
    (FlowText::ColourWhite, Hs { h: 0.0, s: 0.0 }),
    (FlowText::ColourRed, Hs { h: 0.0, s: 100.0 }),
    (FlowText::ColourOrange, Hs { h: 30.0, s: 100.0 }),
    (FlowText::ColourYellow, Hs { h: 55.0, s: 100.0 }),
    (FlowText::ColourGreen, Hs { h: 120.0, s: 100.0 }),
    (FlowText::ColourCyan, Hs { h: 180.0, s: 100.0 }),
    (FlowText::ColourBlue, Hs { h: 225.0, s: 100.0 }),
    (FlowText::ColourPurple, Hs { h: 270.0, s: 100.0 }),
    (FlowText::ColourMagenta, Hs { h: 300.0, s: 100.0 }),
    (FlowText::ColourPink, Hs { h: 335.0, s: 100.0 }),
];

/// The swatch `hs` is, or `None` for a colour set some other way.
pub fn swatch_of(hs: Hs) -> Option<usize> {
    SWATCHES.iter().position(|(_, s)| *s == hs)
}

#[cfg(test)]
mod tests {
    use oikos_core::model::{
        AlarmMode, Capabilities, EntityCategory, EntityState, Kind, StateValue, UiOverride,
    };
    use serde_json::json;

    use super::*;

    fn entity(kind: Kind, caps: Capabilities) -> EntityView {
        EntityView {
            id: "e".into(),
            device_id: None,
            name: "E".into(),
            device_name: None,
            label: None,
            kind,
            component: kind.component().to_string(),
            protocol_kind: kind,
            device_class: None,
            unit: None,
            icon: None,
            category: EntityCategory::Primary,
            enabled_by_default: true,
            area: "a".into(),
            part_of: None,
            capabilities: caps,
            ui: UiOverride::default(),
            state: EntityState::default(),
        }
    }

    fn bare(command: Command) -> Action {
        Action {
            command,
            params: vec![],
        }
    }

    #[test]
    fn command_labels_follow_the_language() {
        let on = Command::TurnOn {
            brightness: None,
            color: None,
            effect: None,
        };
        assert_eq!(command_label(Lang::En, &on), "Turn on");
        assert_eq!(command_label(Lang::Fr, &on), "Allumer");
        assert_eq!(command_label(Lang::Fr, &Command::Stop), "Arrêter");
        assert_eq!(command_label(Lang::Fr, &Command::Press), "Appuyer");
        let pos = Command::SetPosition { position: 50 };
        assert_eq!(command_label(Lang::Fr, &pos), "Régler la position");
        let pick = Command::SelectOption {
            option: "low".into(),
        };
        assert_eq!(command_label(Lang::En, &pick), "Select low");
        assert_eq!(command_label(Lang::Fr, &pick), "Choisir low");
        let mode = Command::SetHvacMode {
            mode: "heat".into(),
        };
        assert_eq!(command_label(Lang::Fr, &mode), "Régler le mode chauffage");
    }

    #[test]
    fn new_command_labels_follow_the_language() {
        let dock = Command::VacuumAction {
            action: "return_to_base".into(),
        };
        assert_eq!(command_label(Lang::Fr, &dock), "Retour à la base");
        let fan = Command::SetFanMode { mode: "low".into() };
        assert_eq!(command_label(Lang::En, &fan), "Fan: Low");
        let arm = Command::Arm {
            mode: AlarmMode::Away,
            code: None,
        };
        assert_eq!(command_label(Lang::Fr, &arm), "Activer (absent)");
        let lock = Command::Control {
            control: "locked".into(),
            value: Some(json!(true)),
        };
        assert_eq!(command_label(Lang::En, &lock), "Lock");
    }

    #[test]
    fn setting_one_param_keeps_the_others() {
        let on = Command::TurnOn {
            brightness: Some(100),
            color: Some(SWATCHES[1].1),
            effect: None,
        };
        assert_eq!(
            with_param(&on, "effect", Some(json!("blink"))),
            Command::TurnOn {
                brightness: Some(100),
                color: Some(SWATCHES[1].1),
                effect: Some("blink".into())
            }
        );
        assert_eq!(
            with_param(&on, "color", None),
            Command::TurnOn {
                brightness: Some(100),
                color: None,
                effect: None
            }
        );
        assert_eq!(param_value(&on, "brightness"), Some(json!(100)));
    }

    #[test]
    fn a_param_that_does_not_parse_is_ignored() {
        let pos = Command::SetPosition { position: 10 };
        assert_eq!(with_param(&pos, "position", Some(json!("abc"))), pos);
    }

    #[test]
    fn whole_numbers_are_written_as_integers() {
        let pos = Command::SetPosition { position: 10 };
        assert_eq!(
            with_param(&pos, "position", Some(number_json(40.0, 1.0, 1.0))),
            Command::SetPosition { position: 40 }
        );
        assert_eq!(number_json(60.0, 2.55, 1.0), json!(153));
        assert_eq!(number_json(19.5, 1.0, 0.5), json!(19.5));
    }

    #[test]
    fn unlisted_command_is_offered_first() {
        let list = vec![bare(Command::TurnOff)];
        let odd = Command::SetPosition { position: 10 };
        assert_eq!(command_options(&odd, &list)[0].command, odd);
        assert_eq!(command_options(&Command::TurnOff, &list).len(), 1);
    }

    #[test]
    fn retarget_keeps_offered_command_else_first() {
        let cover = vec![bare(Command::Open), bare(Command::Close)];
        assert_eq!(retarget(&Command::Close, &cover), Command::Close);
        assert_eq!(retarget(&Command::TurnOff, &cover), Command::Open);
        assert_eq!(retarget(&Command::TurnOff, &[]), Command::TurnOff);
    }

    #[test]
    fn swatches_read_back_as_their_own_colour_name() {
        for (name, hs) in SWATCHES {
            assert_eq!(crate::flow_text::colour_name(hs), name);
        }
        assert_eq!(swatch_of(SWATCHES[2].1), Some(2));
        assert_eq!(swatch_of(Hs { h: 12.0, s: 80.0 }), None);
    }

    #[test]
    fn lock_and_unlock_are_two_choices() {
        let lock = Command::Control {
            control: "locked".into(),
            value: Some(json!(true)),
        };
        let unlock = Command::Control {
            control: "locked".into(),
            value: Some(json!(false)),
        };
        assert_ne!(command_key(&lock), command_key(&unlock));
    }

    #[test]
    fn a_number_entity_starts_at_its_current_value() {
        let mut n = entity(Kind::Number, Capabilities::default());
        n.state.value = StateValue::Number(42.0);
        assert_eq!(
            entity_actions(&n)[0].command,
            Command::SetNumber { value: 42.0 }
        );
    }

    #[test]
    fn select_options_are_listed_from_core() {
        let caps = Capabilities {
            options: vec!["low".into(), "high".into()],
            ..Default::default()
        };
        let list = entity_actions(&entity(Kind::Select, caps));
        assert_eq!(
            list.into_iter().map(|a| a.command).collect::<Vec<_>>(),
            vec![
                Command::SelectOption {
                    option: "low".into()
                },
                Command::SelectOption {
                    option: "high".into()
                }
            ]
        );
    }
}
