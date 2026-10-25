//! Eco's extra actions (spec §7): which fan modes to set when a zone enters eco, and which to
//! put back when it leaves.
//!
//! Pure. The hub passes the memory kept in the previous decision, the new decision and the
//! devices, sends the returned commands and keeps the returned memory in the new decision.

use std::collections::BTreeMap;

use crate::decide::Device;
use crate::doc::{Decision, EcoAction, FanRestore, Source, fan_mode_of};

/// A fan mode to send to one climate entity.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FanCommand {
    pub entity: String,
    pub fan_mode: String,
}

/// The fan commands for `decision` and the memory to keep with it. `previous` is the memory of
/// the zone's previous decision; `zone_off` puts every remembered fan back, once.
pub fn fan_plan(
    previous: &BTreeMap<String, FanRestore>,
    decision: &Decision,
    devices: &[Device],
    actions: &[EcoAction],
    zone_off: bool,
) -> (BTreeMap<String, FanRestore>, Vec<FanCommand>) {
    let mut memory = previous.clone();
    let mut commands = Vec::new();
    let in_eco = matches!(decision.source, Source::Eco | Source::Away);
    let interrupted = matches!(decision.source, Source::Safety | Source::Override);
    if !zone_off && interrupted {
        return (memory, commands);
    }
    let want = fan_mode_of(actions);
    for device in devices {
        let entry = memory.get(&device.id).cloned();
        let current = device.fan_mode.as_deref();
        let mut restore = |memory: &mut BTreeMap<String, FanRestore>, r: &FanRestore| {
            if current == Some(r.set.as_str()) {
                commands.push(FanCommand {
                    entity: device.id.clone(),
                    fan_mode: r.was.clone(),
                });
            }
            memory.remove(&device.id);
        };
        if zone_off || !in_eco {
            if let Some(r) = &entry {
                restore(&mut memory, r);
            }
            continue;
        }
        let listed = want.filter(|v| device.caps.fan_modes.iter().any(|m| m == v));
        let Some(v) = listed else {
            if let Some(r) = &entry {
                restore(&mut memory, r);
            }
            continue;
        };
        let driven = decision
            .entities
            .get(&device.id)
            .is_some_and(|t| !t.held && t.mode != "off");
        if !driven {
            continue;
        }
        match entry {
            None => {
                if let Some(f) = current.filter(|f| *f != v) {
                    memory.insert(
                        device.id.clone(),
                        FanRestore {
                            was: f.to_owned(),
                            set: v.to_owned(),
                        },
                    );
                    commands.push(FanCommand {
                        entity: device.id.clone(),
                        fan_mode: v.to_owned(),
                    });
                }
            }
            Some(r) if current != Some(r.set.as_str()) => {
                // Moved by hand while in eco: the user's choice, never undone.
                memory.remove(&device.id);
            }
            Some(r) if r.set != v => {
                memory.insert(
                    device.id.clone(),
                    FanRestore {
                        was: r.was,
                        set: v.to_owned(),
                    },
                );
                commands.push(FanCommand {
                    entity: device.id.clone(),
                    fan_mode: v.to_owned(),
                });
            }
            Some(_) => {}
        }
    }
    (memory, commands)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decide::DeviceCaps;
    use crate::doc::DeviceTarget;

    fn device(id: &str, fan: &str, modes: &[&str]) -> Device {
        Device {
            id: id.into(),
            caps: DeviceCaps {
                hvac_modes: vec!["heat".into(), "off".into()],
                fan_modes: modes.iter().map(|m| m.to_string()).collect(),
                min_temp: None,
                max_temp: None,
                temp_step: None,
            },
            target: Some(16.0),
            mode: Some("heat".into()),
            fan_mode: Some(fan.into()),
        }
    }

    fn unit(fan: &str) -> Device {
        device("a", fan, &["auto", "low", "high"])
    }

    fn decision(source: Source, targets: &[(&str, &str, bool)]) -> Decision {
        Decision {
            zone: "z".into(),
            mode: None,
            setpoint: None,
            source,
            cause: None,
            reason: String::new(),
            reading: None,
            reading_age_s: None,
            outdoor_mean: None,
            home: true,
            applied: false,
            at: 0,
            entities: targets
                .iter()
                .map(|(id, mode, held)| {
                    (
                        id.to_string(),
                        DeviceTarget {
                            setpoint: Some(16.0),
                            mode: mode.to_string(),
                            held: *held,
                        },
                    )
                })
                .collect(),
            eco_restore: BTreeMap::new(),
        }
    }

    fn eco() -> Decision {
        decision(Source::Eco, &[("a", "heat", false)])
    }

    fn want(m: &str) -> Vec<EcoAction> {
        vec![EcoAction::FanMode(m.into())]
    }

    fn memory(was: &str, set: &str) -> BTreeMap<String, FanRestore> {
        BTreeMap::from([(
            "a".to_string(),
            FanRestore {
                was: was.into(),
                set: set.into(),
            },
        )])
    }

    fn cmd(m: &str) -> Vec<FanCommand> {
        vec![FanCommand {
            entity: "a".into(),
            fan_mode: m.into(),
        }]
    }

    #[test]
    fn entering_eco_sets_the_fan_and_remembers() {
        let (mem, cmds) = fan_plan(
            &BTreeMap::new(),
            &eco(),
            &[unit("auto")],
            &want("low"),
            false,
        );
        assert_eq!(cmds, cmd("low"));
        assert_eq!(mem, memory("auto", "low"));
    }

    #[test]
    fn away_counts_as_eco() {
        let d = decision(Source::Away, &[("a", "heat", false)]);
        let (_, cmds) = fan_plan(&BTreeMap::new(), &d, &[unit("auto")], &want("low"), false);
        assert_eq!(cmds, cmd("low"));
    }

    #[test]
    fn already_at_eco_value_sends_nothing() {
        let (mem, cmds) = fan_plan(
            &BTreeMap::new(),
            &eco(),
            &[unit("low")],
            &want("low"),
            false,
        );
        assert!(cmds.is_empty());
        assert!(mem.is_empty());
    }

    #[test]
    fn unit_without_the_value_is_left_alone() {
        let d = device("a", "auto", &["auto"]);
        let (mem, cmds) = fan_plan(&BTreeMap::new(), &eco(), &[d], &want("low"), false);
        assert!(cmds.is_empty());
        assert!(mem.is_empty());
    }

    #[test]
    fn unit_without_fan_state_is_left_alone() {
        let mut d = unit("auto");
        d.fan_mode = None;
        let (mem, cmds) = fan_plan(&BTreeMap::new(), &eco(), &[d], &want("low"), false);
        assert!(cmds.is_empty());
        assert!(mem.is_empty());
    }

    #[test]
    fn held_or_off_units_get_no_fan_command() {
        for target in [("a", "heat", true), ("a", "off", false)] {
            let d = decision(Source::Eco, &[target]);
            let (mem, cmds) = fan_plan(&BTreeMap::new(), &d, &[unit("auto")], &want("low"), false);
            assert!(cmds.is_empty(), "{target:?}");
            assert!(mem.is_empty());
        }
        let not_driven = decision(Source::Eco, &[]);
        let (_, cmds) = fan_plan(
            &BTreeMap::new(),
            &not_driven,
            &[unit("auto")],
            &want("low"),
            false,
        );
        assert!(cmds.is_empty());
    }

    #[test]
    fn a_held_unit_keeps_its_memory() {
        let d = decision(Source::Eco, &[("a", "heat", true)]);
        let (mem, cmds) = fan_plan(
            &memory("auto", "low"),
            &d,
            &[unit("low")],
            &want("low"),
            false,
        );
        assert!(cmds.is_empty());
        assert_eq!(mem, memory("auto", "low"));
    }

    #[test]
    fn leaving_eco_restores_when_unchanged() {
        let d = decision(Source::Schedule, &[("a", "heat", false)]);
        let (mem, cmds) = fan_plan(
            &memory("auto", "low"),
            &d,
            &[unit("low")],
            &want("low"),
            false,
        );
        assert_eq!(cmds, cmd("auto"));
        assert!(mem.is_empty());
    }

    #[test]
    fn leaving_eco_leaves_a_hand_changed_fan() {
        let d = decision(Source::Schedule, &[("a", "heat", false)]);
        let (mem, cmds) = fan_plan(
            &memory("auto", "low"),
            &d,
            &[unit("high")],
            &want("low"),
            false,
        );
        assert!(cmds.is_empty());
        assert!(mem.is_empty());
    }

    #[test]
    fn a_fan_moved_during_eco_is_forgotten() {
        let (mem, cmds) = fan_plan(
            &memory("auto", "low"),
            &eco(),
            &[unit("high")],
            &want("low"),
            false,
        );
        assert!(cmds.is_empty());
        assert!(mem.is_empty());
    }

    #[test]
    fn safety_or_override_keeps_memory() {
        for source in [Source::Safety, Source::Override] {
            let d = decision(source, &[("a", "heat", false)]);
            let (mem, cmds) = fan_plan(
                &memory("auto", "low"),
                &d,
                &[unit("low")],
                &want("low"),
                false,
            );
            assert!(cmds.is_empty(), "{source:?}");
            assert_eq!(mem, memory("auto", "low"));
        }
    }

    #[test]
    fn eco_to_away_sends_nothing_again() {
        let d = decision(Source::Away, &[("a", "heat", false)]);
        let (mem, cmds) = fan_plan(
            &memory("auto", "low"),
            &d,
            &[unit("low")],
            &want("low"),
            false,
        );
        assert!(cmds.is_empty());
        assert_eq!(mem, memory("auto", "low"));
    }

    #[test]
    fn changing_the_eco_value_moves_units_still_at_the_old_one() {
        let (mem, cmds) = fan_plan(
            &memory("auto", "low"),
            &eco(),
            &[unit("low")],
            &want("high"),
            false,
        );
        assert_eq!(cmds, cmd("high"));
        assert_eq!(mem, memory("auto", "high"));
    }

    #[test]
    fn removing_the_action_restores() {
        let (mem, cmds) = fan_plan(&memory("auto", "low"), &eco(), &[unit("low")], &[], false);
        assert_eq!(cmds, cmd("auto"));
        assert!(mem.is_empty());
    }

    #[test]
    fn zone_off_restores() {
        let (mem, cmds) = fan_plan(
            &memory("auto", "low"),
            &eco(),
            &[unit("low")],
            &want("low"),
            true,
        );
        assert_eq!(cmds, cmd("auto"));
        assert!(mem.is_empty());
    }

    #[test]
    fn absent_device_keeps_its_memory() {
        let d = decision(Source::Schedule, &[]);
        let (mem, cmds) = fan_plan(&memory("auto", "low"), &d, &[], &want("low"), false);
        assert!(cmds.is_empty());
        assert_eq!(mem, memory("auto", "low"));
    }
}
