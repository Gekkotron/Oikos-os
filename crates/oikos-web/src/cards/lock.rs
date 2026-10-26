//! The lock card: the lock's state in words and one button that does the opposite, asking
//! first before it unlocks.

use oikos_core::model::{Availability, Command, EntityState, GenericValue, StateValue};

/// What the card says about the lock.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LockWord {
    Locked,
    Unlocked,
    Locking,
    Unlocking,
    Jammed,
    Open,
    Opening,
    Offline,
    Unknown,
}

/// What the card's one button does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LockAction {
    Lock,
    Unlock,
}

/// The card's face: its word, its button (`None` shows the button disabled) and whether the
/// state wants attention.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LockFace {
    pub word: LockWord,
    pub action: Option<LockAction>,
    pub warn: bool,
}

/// The face for `state`: an offline lock is offline whatever it last reported, and a word the
/// lock type does not know reads as unknown.
pub fn lock_face(state: &EntityState) -> LockFace {
    use LockAction::*;
    use LockWord::*;
    if state.availability == Availability::Offline {
        return LockFace {
            word: Offline,
            action: None,
            warn: false,
        };
    }
    let raw = match &state.value {
        StateValue::Generic(g) => match g.fields.get("state") {
            Some(GenericValue::Text(s)) => s.as_str(),
            _ => "",
        },
        _ => "",
    };
    let (word, action, warn) = match raw {
        "locked" => (Locked, Some(Unlock), false),
        "unlocked" => (Unlocked, Some(Lock), true),
        "jammed" => (Jammed, Some(Lock), true),
        "open" => (Open, Some(Lock), true),
        "opening" => (Opening, None, true),
        "locking" => (Locking, None, false),
        "unlocking" => (Unlocking, None, false),
        _ => (Unknown, None, false),
    };
    LockFace { word, action, warn }
}

/// The lock type's `locked` on/off control: on locks, off unlocks.
pub fn lock_command(action: LockAction) -> Command {
    Command::Control {
        control: "locked".into(),
        value: Some(serde_json::Value::Bool(action == LockAction::Lock)),
    }
}

/// Whether an "Unlock?" answered now should still send: only while the lock reads locked.
pub fn unlock_still_wanted(state: &EntityState) -> bool {
    lock_face(state).action == Some(LockAction::Unlock)
}

#[cfg(test)]
mod tests {
    use oikos_core::model::GenericState;

    use super::*;

    fn state(raw: &str, availability: Availability) -> EntityState {
        let mut g = GenericState::default();
        g.fields
            .insert("state".into(), GenericValue::Text(raw.into()));
        EntityState {
            value: StateValue::Generic(g),
            availability,
            ..EntityState::default()
        }
    }

    #[test]
    fn face_per_state() {
        use LockAction::*;
        use LockWord::*;
        for (raw, word, action, warn) in [
            ("locked", Locked, Some(Unlock), false),
            ("unlocked", Unlocked, Some(Lock), true),
            ("jammed", Jammed, Some(Lock), true),
            ("open", Open, Some(Lock), true),
            ("opening", Opening, None, true),
            ("locking", Locking, None, false),
            ("unlocking", Unlocking, None, false),
            ("weird", Unknown, None, false),
        ] {
            assert_eq!(
                lock_face(&state(raw, Availability::Online)),
                LockFace { word, action, warn },
                "{raw}"
            );
        }
    }

    #[test]
    fn offline_wins_and_disables() {
        assert_eq!(
            lock_face(&state("locked", Availability::Offline)),
            LockFace {
                word: LockWord::Offline,
                action: None,
                warn: false
            }
        );
    }

    #[test]
    fn no_state_is_unknown() {
        assert_eq!(lock_face(&EntityState::default()).word, LockWord::Unknown);
    }

    #[test]
    fn unlock_is_wanted_only_while_locked() {
        assert!(unlock_still_wanted(&state("locked", Availability::Online)));
        assert!(!unlock_still_wanted(&state("unlocked", Availability::Online)));
        assert!(!unlock_still_wanted(&state("locked", Availability::Offline)));
    }

    #[test]
    fn commands_use_the_locked_control() {
        assert_eq!(
            lock_command(LockAction::Lock),
            Command::Control {
                control: "locked".into(),
                value: Some(serde_json::Value::Bool(true))
            }
        );
        assert_eq!(
            lock_command(LockAction::Unlock),
            Command::Control {
                control: "locked".into(),
                value: Some(serde_json::Value::Bool(false))
            }
        );
    }
}
