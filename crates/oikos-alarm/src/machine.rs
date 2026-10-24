//! The alarm state machine: pure and deterministic, so the hub only feeds it events and clock ticks.
//!
//! `step` applies one [`Event`] to the persisted [`Runtime`] and returns the [`Effect`]s the hub must
//! carry out. `next_deadline` says when the hub must send the next [`Event::Tick`].

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::doc::{AlarmSettings, ArmMode, Entry, RemoteAction, SensorKind};

/// Where the alarm is in its cycle.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "phase", rename_all = "snake_case")]
pub enum Phase {
    Disarmed,
    Arming {
        mode: ArmMode,
        until: u64,
    },
    Armed {
        mode: ArmMode,
    },
    Pending {
        mode: ArmMode,
        until: u64,
    },
    Triggered {
        back_to: Option<ArmMode>,
        until: u64,
    },
}

/// What asked for the last arm or disarm.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    Presence,
    Remote,
    Sos,
    Command,
}

/// The last state seen for a presence tracker.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Presence {
    Home,
    Away,
    Unknown,
}

/// Everything the engine persists between events (retained, §9).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Runtime {
    pub phase: Phase,
    pub source: Option<Source>,
    pub since: u64,
    pub motion_hits: Vec<(u64, String)>,
    pub open_at_arm: BTreeSet<String>,
    pub cause: Vec<String>,
    pub alert_ids: Vec<String>,
    pub trackers: BTreeMap<String, Presence>,
    pub auto_arm_ready: bool,
    pub everyone_away_since: Option<u64>,
}

impl Default for Runtime {
    fn default() -> Self {
        Self {
            phase: Phase::Disarmed,
            source: None,
            since: 0,
            motion_hits: Vec::new(),
            open_at_arm: BTreeSet::new(),
            cause: Vec::new(),
            alert_ids: Vec::new(),
            trackers: BTreeMap::new(),
            auto_arm_ready: false,
            everyone_away_since: None,
        }
    }
}

impl Runtime {
    /// The published alarm state name.
    pub fn state_name(&self) -> &'static str {
        match self.phase {
            Phase::Disarmed => "disarmed",
            Phase::Arming { .. } => "arming",
            Phase::Armed { mode } => match mode {
                ArmMode::Away => "armed_away",
                ArmMode::Home => "armed_home",
                ArmMode::Night => "armed_night",
            },
            Phase::Pending { .. } => "pending",
            Phase::Triggered { .. } => "triggered",
        }
    }

    /// The arm mode this phase belongs to (for `Triggered`, the mode it returns to).
    pub fn mode(&self) -> Option<ArmMode> {
        match self.phase {
            Phase::Disarmed => None,
            Phase::Arming { mode, .. } | Phase::Armed { mode } | Phase::Pending { mode, .. } => {
                Some(mode)
            }
            Phase::Triggered { back_to, .. } => back_to,
        }
    }

    /// When the current phase ends by itself.
    pub fn deadline(&self) -> Option<u64> {
        match self.phase {
            Phase::Arming { until, .. }
            | Phase::Pending { until, .. }
            | Phase::Triggered { until, .. } => Some(until),
            Phase::Disarmed | Phase::Armed { .. } => None,
        }
    }

    fn enter(&mut self, phase: Phase, source: Option<Source>, now: u64) {
        self.phase = phase;
        if source.is_some() {
            self.source = source;
        }
        self.since = now;
    }
}

/// The watched sensors that are on, as the hub sees them right now.
pub struct World<'a> {
    pub on: &'a BTreeSet<String>,
}

/// Something that happened.
#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    Arm { mode: ArmMode, source: Source },
    Disarm { source: Source },
    Sensor { entity: String, on: bool },
    Signal { entity: String, signal: String },
    Tracker { entity: String, seen: Presence },
    Tick,
    Resume,
}

/// Something the hub must do.
#[derive(Clone, Debug, PartialEq)]
pub enum Effect {
    SirensOn,
    SirensOff,
    Chirp,
    Alert { cause: Vec<String> },
    ResolveAlerts,
    OpenAtArm(Vec<String>),
}

/// The action a remote's signal is mapped to.
fn remote_action(s: &AlarmSettings, entity: &str, signal: &str) -> Option<RemoteAction> {
    let remote = s.remotes.iter().find(|r| r.entity == entity)?;
    remote.signals.get(signal).copied()
}

/// A hit is inside the window while `now < at + window_s`.
fn hit_alive(at: u64, now: u64, s: &AlarmSettings) -> bool {
    now < at + u64::from(s.motion.window_s)
}

/// The watched openings of `mode` that are on right now.
fn open_openings(s: &AlarmSettings, world: &World, mode: ArmMode) -> BTreeSet<String> {
    s.sensors
        .iter()
        .filter(|r| r.kind == SensorKind::Opening && r.modes.contains(&mode))
        .filter(|r| world.on.contains(&r.entity))
        .map(|r| r.entity.clone())
        .collect()
}

/// Enters `Armed`: what is open now is watched for closing (§4.3), and the panel chirps.
fn enter_armed(
    rt: &mut Runtime,
    s: &AlarmSettings,
    world: &World,
    mode: ArmMode,
    source: Option<Source>,
    now: u64,
) -> Vec<Effect> {
    rt.enter(Phase::Armed { mode }, source, now);
    rt.motion_hits.clear();
    rt.open_at_arm = open_openings(s, world, mode);
    let mut fx = vec![Effect::Chirp];
    if !rt.open_at_arm.is_empty() {
        fx.push(Effect::OpenAtArm(rt.open_at_arm.iter().cloned().collect()));
    }
    fx
}

fn trigger(
    rt: &mut Runtime,
    s: &AlarmSettings,
    back_to: Option<ArmMode>,
    cause: Vec<String>,
    source: Option<Source>,
    now: u64,
) -> Vec<Effect> {
    rt.enter(
        Phase::Triggered {
            back_to,
            until: now + u64::from(s.siren_s),
        },
        source,
        now,
    );
    rt.motion_hits.clear();
    rt.cause = cause.clone();
    vec![Effect::SirensOn, Effect::Alert { cause }]
}

/// The siren period is over: back to the state before the trigger, without a chirp. The alert
/// stays open until a disarm.
pub fn return_from_ring(
    rt: &mut Runtime,
    s: &AlarmSettings,
    world: &World,
    back_to: Option<ArmMode>,
    now: u64,
) -> Vec<Effect> {
    match back_to {
        Some(mode) => {
            rt.enter(Phase::Armed { mode }, None, now);
            rt.open_at_arm = open_openings(s, world, mode);
        }
        None => {
            rt.enter(Phase::Disarmed, None, now);
            rt.open_at_arm.clear();
        }
    }
    rt.motion_hits.clear();
    vec![Effect::SirensOff]
}

fn arm(
    rt: &mut Runtime,
    s: &AlarmSettings,
    world: &World,
    mode: ArmMode,
    source: Source,
    now: u64,
) -> Vec<Effect> {
    match rt.phase {
        Phase::Armed { mode: current } if current == mode => return vec![],
        Phase::Arming { .. } | Phase::Pending { .. } | Phase::Triggered { .. } => return vec![],
        Phase::Disarmed | Phase::Armed { .. } => {}
    }
    rt.cause.clear();
    if source == Source::Presence || s.exit_delay_s == 0 {
        return enter_armed(rt, s, world, mode, Some(source), now);
    }
    rt.motion_hits.clear();
    rt.open_at_arm.clear();
    rt.enter(
        Phase::Arming {
            mode,
            until: now + u64::from(s.exit_delay_s),
        },
        Some(source),
        now,
    );
    vec![]
}

fn disarm(rt: &mut Runtime, source: Source, now: u64) -> Vec<Effect> {
    let mut fx = Vec::new();
    if matches!(rt.phase, Phase::Triggered { .. }) {
        fx.push(Effect::SirensOff);
    }
    if !rt.alert_ids.is_empty() {
        fx.push(Effect::ResolveAlerts);
    }
    rt.enter(Phase::Disarmed, Some(source), now);
    rt.motion_hits.clear();
    rt.open_at_arm.clear();
    rt.cause.clear();
    rt.auto_arm_ready = rt.trackers.values().any(|p| *p == Presence::Home);
    fx
}

/// An opening event (it went on, or a watched-for-closing one went off), per its `entry` (§4.2).
fn opening(
    rt: &mut Runtime,
    s: &AlarmSettings,
    entity: &str,
    entry: Entry,
    now: u64,
) -> Vec<Effect> {
    match (rt.phase.clone(), entry) {
        (Phase::Armed { mode }, Entry::Instant) | (Phase::Pending { mode, .. }, Entry::Instant) => {
            trigger(rt, s, Some(mode), vec![entity.to_string()], None, now)
        }
        (Phase::Armed { mode }, Entry::Delayed) => {
            rt.cause = vec![entity.to_string()];
            rt.enter(
                Phase::Pending {
                    mode,
                    until: now + u64::from(s.entry_delay_s),
                },
                None,
                now,
            );
            vec![]
        }
        _ => vec![],
    }
}

fn sensor(rt: &mut Runtime, s: &AlarmSettings, entity: &str, on: bool, now: u64) -> Vec<Effect> {
    let Some(mode) = (match rt.phase {
        Phase::Armed { mode } | Phase::Pending { mode, .. } => Some(mode),
        _ => None,
    }) else {
        return vec![];
    };
    let Some(rule) = s.sensor(entity) else {
        return vec![];
    };
    if !rule.modes.contains(&mode) {
        return vec![];
    }
    match (rule.kind, on) {
        (SensorKind::Opening, true) => opening(rt, s, entity, rule.entry, now),
        (SensorKind::Opening, false) => {
            if rt.open_at_arm.remove(entity) {
                opening(rt, s, entity, rule.entry, now)
            } else {
                vec![]
            }
        }
        (SensorKind::Motion, true) => {
            if matches!(rt.phase, Phase::Pending { .. }) {
                return vec![];
            }
            rt.motion_hits.push((now, entity.to_string()));
            rt.motion_hits.retain(|(at, _)| hit_alive(*at, now, s));
            if rt.motion_hits.len() < usize::from(s.motion.count) {
                return vec![];
            }
            let mut cause: Vec<String> = Vec::new();
            for (_, e) in &rt.motion_hits {
                if !cause.contains(e) {
                    cause.push(e.clone());
                }
            }
            trigger(rt, s, Some(mode), cause, None, now)
        }
        (SensorKind::Motion, false) => vec![],
    }
}

fn sos(rt: &mut Runtime, s: &AlarmSettings, remote: &str, now: u64) -> Vec<Effect> {
    let back_to = match rt.phase {
        // Already ringing: no second ring until it has returned to armed.
        Phase::Triggered { .. } => return vec![],
        Phase::Armed { mode } | Phase::Pending { mode, .. } => Some(mode),
        Phase::Disarmed | Phase::Arming { .. } => None,
    };
    trigger(
        rt,
        s,
        back_to,
        vec![format!("sos:{remote}")],
        Some(Source::Sos),
        now,
    )
}

fn tick(rt: &mut Runtime, s: &AlarmSettings, world: &World, now: u64) -> Vec<Effect> {
    rt.motion_hits.retain(|(at, _)| hit_alive(*at, now, s));
    match rt.phase.clone() {
        Phase::Arming { mode, until } if now >= until => enter_armed(rt, s, world, mode, None, now),
        Phase::Pending { mode, until } if now >= until => {
            let cause = rt.cause.clone();
            trigger(rt, s, Some(mode), cause, None, now)
        }
        Phase::Triggered { back_to, until } if now >= until => {
            return_from_ring(rt, s, world, back_to, now)
        }
        _ => vec![],
    }
}

/// Applies `ev` at time `now` (seconds) and returns what the hub must do.
pub fn step(
    rt: &mut Runtime,
    s: &AlarmSettings,
    world: &World,
    ev: Event,
    now: u64,
) -> Vec<Effect> {
    // Settings drift: forget sensors that are no longer watched.
    rt.open_at_arm.retain(|e| s.sensor(e).is_some());
    rt.motion_hits.retain(|(_, e)| s.sensor(e).is_some());
    match ev {
        Event::Arm { mode, source } => arm(rt, s, world, mode, source, now),
        Event::Disarm { source } => disarm(rt, source, now),
        Event::Sensor { entity, on } => sensor(rt, s, &entity, on, now),
        Event::Signal { entity, signal } => match remote_action(s, &entity, &signal) {
            Some(RemoteAction::Sos) => sos(rt, s, &entity, now),
            _ => vec![],
        },
        Event::Tracker { .. } | Event::Resume => vec![],
        Event::Tick => tick(rt, s, world, now),
    }
}

/// The earliest time the hub must send a `Tick`; never a time a `Tick` would not act on.
pub fn next_deadline(rt: &Runtime, s: &AlarmSettings) -> Option<u64> {
    let auto_arm = (s.presence.arm
        && !s.presence.trackers.is_empty()
        && rt.auto_arm_ready
        && matches!(rt.phase, Phase::Disarmed))
    .then(|| {
        rt.everyone_away_since
            .map(|t| t + u64::from(s.presence.arm_after_s))
    })
    .flatten();
    let hit_expiry = rt
        .motion_hits
        .iter()
        .map(|(at, _)| at + u64::from(s.motion.window_s))
        .min();
    [rt.deadline(), auto_arm, hit_expiry]
        .into_iter()
        .flatten()
        .min()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::doc::{Remote, SensorRule, default_sensor};

    fn settings() -> AlarmSettings {
        let all = BTreeSet::from([ArmMode::Away, ArmMode::Home, ArmMode::Night]);
        AlarmSettings {
            enabled: true,
            sensors: vec![
                SensorRule {
                    entity: "door".into(),
                    kind: SensorKind::Opening,
                    modes: all,
                    entry: Entry::Delayed,
                },
                SensorRule {
                    entity: "window".into(),
                    kind: SensorKind::Opening,
                    modes: BTreeSet::from([ArmMode::Away, ArmMode::Night]),
                    entry: Entry::Instant,
                },
                default_sensor("hall", SensorKind::Motion),
                default_sensor("kitchen", SensorKind::Motion),
            ],
            remotes: vec![Remote {
                entity: "remote.x".into(),
                signals: BTreeMap::from([("emergency".to_string(), RemoteAction::Sos)]),
            }],
            ..Default::default()
        }
    }

    const NONE: BTreeSet<String> = BTreeSet::new();

    fn step_on(
        rt: &mut Runtime,
        s: &AlarmSettings,
        on: &BTreeSet<String>,
        ev: Event,
        now: u64,
    ) -> Vec<Effect> {
        step(rt, s, &World { on }, ev, now)
    }

    fn go(rt: &mut Runtime, s: &AlarmSettings, ev: Event, now: u64) -> Vec<Effect> {
        step_on(rt, s, &NONE, ev, now)
    }

    fn sensor_ev(entity: &str, on: bool) -> Event {
        Event::Sensor {
            entity: entity.into(),
            on,
        }
    }

    fn armed(s: &AlarmSettings, mode: ArmMode) -> Runtime {
        let mut rt = Runtime::default();
        let ev = Event::Arm {
            mode,
            source: Source::Presence,
        };
        go(&mut rt, s, ev, 0);
        assert_eq!(rt.phase, Phase::Armed { mode });
        rt
    }

    #[test]
    fn manual_arm_goes_through_exit_delay() {
        let s = settings();
        let mut rt = Runtime::default();
        let ev = Event::Arm {
            mode: ArmMode::Away,
            source: Source::Command,
        };
        assert!(go(&mut rt, &s, ev, 0).is_empty());
        assert_eq!(
            rt.phase,
            Phase::Arming {
                mode: ArmMode::Away,
                until: 60
            }
        );
        let fx = go(&mut rt, &s, Event::Tick, 60);
        assert_eq!(
            rt.phase,
            Phase::Armed {
                mode: ArmMode::Away
            }
        );
        assert!(fx.contains(&Effect::Chirp));
    }

    #[test]
    fn zero_exit_delay_arms_at_once() {
        let mut s = settings();
        s.exit_delay_s = 0;
        let mut rt = Runtime::default();
        let ev = Event::Arm {
            mode: ArmMode::Home,
            source: Source::Command,
        };
        assert_eq!(go(&mut rt, &s, ev, 5), vec![Effect::Chirp]);
        assert_eq!(
            rt.phase,
            Phase::Armed {
                mode: ArmMode::Home
            }
        );
    }

    #[test]
    fn presence_arm_skips_exit_delay() {
        let s = settings();
        let mut rt = Runtime::default();
        let ev = Event::Arm {
            mode: ArmMode::Away,
            source: Source::Presence,
        };
        let fx = go(&mut rt, &s, ev, 0);
        assert_eq!(
            rt.phase,
            Phase::Armed {
                mode: ArmMode::Away
            }
        );
        assert_eq!(rt.source, Some(Source::Presence));
        assert!(fx.contains(&Effect::Chirp));
    }

    #[test]
    fn arm_same_mode_is_noop_and_other_mode_rearms() {
        let s = settings();
        let mut rt = armed(&s, ArmMode::Away);
        let same = Event::Arm {
            mode: ArmMode::Away,
            source: Source::Command,
        };
        assert!(go(&mut rt, &s, same, 10).is_empty());
        assert_eq!(rt.since, 0);
        let other = Event::Arm {
            mode: ArmMode::Night,
            source: Source::Command,
        };
        go(&mut rt, &s, other, 10);
        assert!(matches!(
            rt.phase,
            Phase::Arming {
                mode: ArmMode::Night,
                until: 70
            }
        ));
    }

    #[test]
    fn arm_while_busy_is_noop() {
        let s = settings();
        let mut rt = armed(&s, ArmMode::Away);
        go(&mut rt, &s, sensor_ev("window", true), 5);
        let ev = Event::Arm {
            mode: ArmMode::Night,
            source: Source::Command,
        };
        assert!(go(&mut rt, &s, ev, 6).is_empty());
        assert!(matches!(rt.phase, Phase::Triggered { .. }));
    }

    #[test]
    fn instant_opening_triggers() {
        let s = settings();
        let mut rt = armed(&s, ArmMode::Away);
        let fx = go(&mut rt, &s, sensor_ev("window", true), 100);
        assert_eq!(
            rt.phase,
            Phase::Triggered {
                back_to: Some(ArmMode::Away),
                until: 280
            }
        );
        assert_eq!(
            fx,
            vec![
                Effect::SirensOn,
                Effect::Alert {
                    cause: vec!["window".into()]
                }
            ]
        );
    }

    #[test]
    fn delayed_opening_waits_entry_delay() {
        let s = settings();
        let mut rt = armed(&s, ArmMode::Away);
        go(&mut rt, &s, sensor_ev("door", true), 100);
        assert_eq!(
            rt.phase,
            Phase::Pending {
                mode: ArmMode::Away,
                until: 130
            }
        );
        let mut disarmed = rt.clone();
        let fx = go(
            &mut disarmed,
            &s,
            Event::Disarm {
                source: Source::Command,
            },
            110,
        );
        assert_eq!(disarmed.phase, Phase::Disarmed);
        assert!(!fx.contains(&Effect::SirensOn));
        let fx = go(&mut rt, &s, Event::Tick, 130);
        assert!(matches!(rt.phase, Phase::Triggered { .. }));
        assert_eq!(
            fx,
            vec![
                Effect::SirensOn,
                Effect::Alert {
                    cause: vec!["door".into()]
                }
            ]
        );
    }

    #[test]
    fn pending_ignores_motion_and_delayed_but_not_instant() {
        let s = settings();
        let mut rt = armed(&s, ArmMode::Away);
        go(&mut rt, &s, sensor_ev("door", true), 100);
        assert!(go(&mut rt, &s, sensor_ev("hall", true), 101).is_empty());
        assert!(go(&mut rt, &s, sensor_ev("kitchen", true), 102).is_empty());
        assert!(go(&mut rt, &s, sensor_ev("door", true), 103).is_empty());
        assert!(matches!(rt.phase, Phase::Pending { .. }));
        assert!(rt.motion_hits.is_empty());
        go(&mut rt, &s, sensor_ev("window", true), 104);
        assert!(matches!(rt.phase, Phase::Triggered { .. }));
    }

    #[test]
    fn two_motions_within_window_trigger() {
        let s = settings();
        let mut rt = armed(&s, ArmMode::Away);
        go(&mut rt, &s, sensor_ev("hall", true), 1000);
        let fx = go(&mut rt, &s, sensor_ev("kitchen", true), 1119);
        assert!(matches!(rt.phase, Phase::Triggered { .. }));
        assert_eq!(
            fx[1],
            Effect::Alert {
                cause: vec!["hall".into(), "kitchen".into()]
            }
        );

        let mut rt = armed(&s, ArmMode::Away);
        go(&mut rt, &s, sensor_ev("hall", true), 1000);
        go(&mut rt, &s, sensor_ev("kitchen", true), 1121);
        assert_eq!(
            rt.phase,
            Phase::Armed {
                mode: ArmMode::Away
            }
        );
        assert_eq!(rt.motion_hits, vec![(1121, "kitchen".to_string())]);
    }

    #[test]
    fn same_motion_sensor_twice_counts() {
        let s = settings();
        let mut rt = armed(&s, ArmMode::Away);
        go(&mut rt, &s, sensor_ev("hall", true), 10);
        go(&mut rt, &s, sensor_ev("hall", false), 20);
        let fx = go(&mut rt, &s, sensor_ev("hall", true), 30);
        assert!(matches!(rt.phase, Phase::Triggered { .. }));
        assert_eq!(
            fx[1],
            Effect::Alert {
                cause: vec!["hall".into()]
            }
        );
    }

    #[test]
    fn mode_filters_sensors() {
        let s = settings();
        let mut rt = armed(&s, ArmMode::Night);
        go(&mut rt, &s, sensor_ev("hall", true), 10);
        go(&mut rt, &s, sensor_ev("hall", true), 20);
        assert_eq!(
            rt.phase,
            Phase::Armed {
                mode: ArmMode::Night
            }
        );
        let mut rt = armed(&s, ArmMode::Home);
        assert!(go(&mut rt, &s, sensor_ev("window", true), 10).is_empty());
    }

    #[test]
    fn open_window_at_arm_is_watched_for_closing() {
        let s = settings();
        let on = BTreeSet::from(["window".to_string()]);
        let mut rt = Runtime::default();
        let ev = Event::Arm {
            mode: ArmMode::Away,
            source: Source::Presence,
        };
        let fx = step_on(&mut rt, &s, &on, ev, 0);
        assert_eq!(rt.open_at_arm, BTreeSet::from(["window".to_string()]));
        assert_eq!(
            fx,
            vec![Effect::Chirp, Effect::OpenAtArm(vec!["window".into()])]
        );
        let fx = go(&mut rt, &s, sensor_ev("window", false), 50);
        assert!(rt.open_at_arm.is_empty());
        assert_eq!(
            fx[1],
            Effect::Alert {
                cause: vec!["window".into()]
            }
        );
    }

    #[test]
    fn closing_during_exit_delay_is_not_an_event() {
        let s = settings();
        let on = BTreeSet::from(["window".to_string()]);
        let mut rt = Runtime::default();
        let ev = Event::Arm {
            mode: ArmMode::Away,
            source: Source::Command,
        };
        step_on(&mut rt, &s, &on, ev, 0);
        assert!(go(&mut rt, &s, sensor_ev("window", false), 30).is_empty());
        let fx = go(&mut rt, &s, Event::Tick, 60);
        assert_eq!(
            rt.phase,
            Phase::Armed {
                mode: ArmMode::Away
            }
        );
        assert!(rt.open_at_arm.is_empty());
        assert_eq!(fx, vec![Effect::Chirp]);
    }

    #[test]
    fn ring_ends_and_rearms() {
        let s = settings();
        let mut rt = armed(&s, ArmMode::Away);
        go(&mut rt, &s, sensor_ev("window", true), 100);
        rt.alert_ids = vec!["alarm-1".into()];
        let on = BTreeSet::new();
        let fx = step_on(&mut rt, &s, &on, Event::Tick, 280);
        assert_eq!(fx, vec![Effect::SirensOff]);
        assert_eq!(
            rt.phase,
            Phase::Armed {
                mode: ArmMode::Away
            }
        );
        assert_eq!(rt.alert_ids, vec!["alarm-1".to_string()]);
        assert!(rt.motion_hits.is_empty());
        let fx = go(&mut rt, &s, sensor_ev("window", true), 300);
        assert_eq!(fx[0], Effect::SirensOn);
        assert!(matches!(fx[1], Effect::Alert { .. }));
        assert!(matches!(rt.phase, Phase::Triggered { .. }));
    }

    #[test]
    fn ring_end_watches_what_is_still_open() {
        let s = settings();
        let mut rt = armed(&s, ArmMode::Away);
        go(&mut rt, &s, sensor_ev("window", true), 100);
        let on = BTreeSet::from(["window".to_string()]);
        step_on(&mut rt, &s, &on, Event::Tick, 280);
        assert_eq!(rt.open_at_arm, BTreeSet::from(["window".to_string()]));
    }

    #[test]
    fn detections_while_ringing_are_ignored() {
        let s = settings();
        let mut rt = armed(&s, ArmMode::Away);
        go(&mut rt, &s, sensor_ev("window", true), 100);
        let before = rt.clone();
        assert!(go(&mut rt, &s, sensor_ev("hall", true), 110).is_empty());
        assert!(go(&mut rt, &s, sensor_ev("window", true), 111).is_empty());
        assert_eq!(rt, before);
    }

    #[test]
    fn sos_from_disarmed_returns_to_disarmed() {
        let s = settings();
        let mut rt = Runtime::default();
        let ev = Event::Signal {
            entity: "remote.x".into(),
            signal: "emergency".into(),
        };
        let fx = go(&mut rt, &s, ev, 0);
        assert_eq!(
            rt.phase,
            Phase::Triggered {
                back_to: None,
                until: 180
            }
        );
        assert_eq!(
            fx,
            vec![
                Effect::SirensOn,
                Effect::Alert {
                    cause: vec!["sos:remote.x".into()]
                }
            ]
        );
        assert_eq!(go(&mut rt, &s, Event::Tick, 180), vec![Effect::SirensOff]);
        assert_eq!(rt.phase, Phase::Disarmed);
    }

    #[test]
    fn sos_from_armed_returns_to_armed_and_unmapped_signal_is_ignored() {
        let s = settings();
        let mut rt = armed(&s, ArmMode::Night);
        let unmapped = Event::Signal {
            entity: "remote.x".into(),
            signal: "other".into(),
        };
        assert!(go(&mut rt, &s, unmapped, 5).is_empty());
        let ev = Event::Signal {
            entity: "remote.x".into(),
            signal: "emergency".into(),
        };
        go(&mut rt, &s, ev, 10);
        assert!(matches!(
            rt.phase,
            Phase::Triggered {
                back_to: Some(ArmMode::Night),
                ..
            }
        ));
        assert_eq!(rt.source, Some(Source::Sos));
    }

    #[test]
    fn double_disarm_is_harmless() {
        let s = settings();
        let mut rt = armed(&s, ArmMode::Away);
        go(&mut rt, &s, sensor_ev("window", true), 10);
        rt.alert_ids = vec!["alarm-1".into()];
        let ev = Event::Disarm {
            source: Source::Command,
        };
        assert_eq!(
            go(&mut rt, &s, ev.clone(), 20),
            vec![Effect::SirensOff, Effect::ResolveAlerts]
        );
        assert_eq!(rt.phase, Phase::Disarmed);
        assert!(rt.cause.is_empty());
        rt.alert_ids.clear();
        assert!(go(&mut rt, &s, ev, 30).is_empty());
    }

    #[test]
    fn disarm_sets_auto_arm_ready_from_trackers() {
        let s = settings();
        let mut rt = armed(&s, ArmMode::Away);
        rt.trackers.insert("tracker.a".into(), Presence::Home);
        let ev = Event::Disarm {
            source: Source::Command,
        };
        go(&mut rt, &s, ev, 5);
        assert!(rt.auto_arm_ready);
    }

    #[test]
    fn removed_sensor_is_forgotten() {
        let s = settings();
        let mut rt = armed(&s, ArmMode::Away);
        rt.open_at_arm.insert("window".into());
        rt.motion_hits.push((0, "hall".into()));
        let bare = AlarmSettings {
            enabled: true,
            ..Default::default()
        };
        go(&mut rt, &bare, Event::Tick, 1);
        assert!(rt.open_at_arm.is_empty());
        assert!(rt.motion_hits.is_empty());
        assert!(go(&mut rt, &bare, sensor_ev("window", true), 2).is_empty());
    }

    #[test]
    fn state_names() {
        let mut rt = Runtime::default();
        assert_eq!(rt.state_name(), "disarmed");
        let m = ArmMode::Home;
        for (phase, name) in [
            (Phase::Arming { mode: m, until: 1 }, "arming"),
            (Phase::Armed { mode: m }, "armed_home"),
            (
                Phase::Armed {
                    mode: ArmMode::Away,
                },
                "armed_away",
            ),
            (
                Phase::Armed {
                    mode: ArmMode::Night,
                },
                "armed_night",
            ),
            (Phase::Pending { mode: m, until: 1 }, "pending"),
            (
                Phase::Triggered {
                    back_to: None,
                    until: 1,
                },
                "triggered",
            ),
        ] {
            rt.phase = phase;
            assert_eq!(rt.state_name(), name);
        }
    }

    #[test]
    fn deadlines_follow_the_phase() {
        let s = settings();
        let mut rt = armed(&s, ArmMode::Away);
        assert_eq!(next_deadline(&rt, &s), None);
        go(&mut rt, &s, sensor_ev("hall", true), 100);
        assert_eq!(next_deadline(&rt, &s), Some(220));
        go(&mut rt, &s, sensor_ev("door", true), 105);
        assert_eq!(rt.deadline(), Some(135));
        assert_eq!(next_deadline(&rt, &s), Some(135));
    }

    #[test]
    fn next_deadline_moves_forward_after_a_tick() {
        let s = settings();
        let mut rt = armed(&s, ArmMode::Away);
        go(&mut rt, &s, sensor_ev("hall", true), 100);
        let at = next_deadline(&rt, &s).unwrap();
        go(&mut rt, &s, Event::Tick, at);
        assert!(rt.motion_hits.is_empty());
        assert_eq!(next_deadline(&rt, &s), None);

        // Same through a phase deadline with a stale hit alongside.
        let mut rt = armed(&s, ArmMode::Away);
        go(&mut rt, &s, sensor_ev("hall", true), 100);
        go(&mut rt, &s, sensor_ev("door", true), 110);
        for _ in 0..4 {
            let Some(at) = next_deadline(&rt, &s) else {
                return;
            };
            go(&mut rt, &s, Event::Tick, at);
            assert!(next_deadline(&rt, &s).is_none_or(|d| d > at));
        }
    }

    #[test]
    fn auto_arm_deadline_only_when_it_could_fire() {
        let mut s = settings();
        let mut rt = Runtime {
            auto_arm_ready: true,
            everyone_away_since: Some(1000),
            ..Default::default()
        };
        assert_eq!(next_deadline(&rt, &s), None, "no trackers");
        s.presence.trackers = vec!["tracker.a".into()];
        assert_eq!(next_deadline(&rt, &s), Some(1120));
        rt.auto_arm_ready = false;
        assert_eq!(next_deadline(&rt, &s), None);
        rt.auto_arm_ready = true;
        s.presence.arm = false;
        assert_eq!(next_deadline(&rt, &s), None);
        s.presence.arm = true;
        rt.phase = Phase::Armed {
            mode: ArmMode::Away,
        };
        assert_eq!(next_deadline(&rt, &s), None);
    }

    #[test]
    fn runtime_roundtrips_json() {
        let rt = Runtime {
            phase: Phase::Triggered {
                back_to: Some(ArmMode::Away),
                until: 280,
            },
            source: Some(Source::Remote),
            since: 100,
            motion_hits: vec![(90, "hall".into())],
            open_at_arm: BTreeSet::from(["window".to_string()]),
            cause: vec!["window".into()],
            alert_ids: vec!["alarm-1".into()],
            trackers: BTreeMap::from([("tracker.a".to_string(), Presence::Away)]),
            auto_arm_ready: true,
            everyone_away_since: Some(5),
        };
        let json = serde_json::to_string(&rt).unwrap();
        assert!(json.contains(r#""phase":"triggered""#));
        assert_eq!(serde_json::from_str::<Runtime>(&json).unwrap(), rt);
    }
}
