//! The decision tree (spec §5.1, §5.4, §7): from a zone's inputs to what each device should do.
//!
//! Pure and deterministic. The hub gathers the inputs, calls [`decide`], and commands the devices.

use std::collections::BTreeMap;

use chrono::{Duration, NaiveDateTime};

use crate::doc::{
    Decision, DeviceTarget, HouseSettings, Mode, Override, SafetyCause, Setpoint, Source,
    ZoneSettings, ZoneState,
};
use crate::schedule::Reading;

/// Step used when a device does not advertise one.
const DEFAULT_STEP: f64 = 0.5;

/// What a climate device can do.
#[derive(Clone, Debug, PartialEq)]
pub struct DeviceCaps {
    pub hvac_modes: Vec<String>,
    pub min_temp: Option<f64>,
    pub max_temp: Option<f64>,
    pub temp_step: Option<f64>,
}

/// A climate device and its current target and mode.
#[derive(Clone, Debug, PartialEq)]
pub struct Device {
    pub id: String,
    pub caps: DeviceCaps,
    pub target: Option<f64>,
    pub mode: Option<String>,
}

/// What the engine found on the devices before a safety action, to put back when it clears.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SafetyMemory {
    pub found: BTreeMap<String, DeviceTarget>,
}

impl SafetyMemory {
    /// Record each device's current target and mode.
    pub fn capture(devices: &[Device]) -> SafetyMemory {
        let found = devices
            .iter()
            .filter_map(|d| {
                Some((
                    d.id.clone(),
                    DeviceTarget {
                        setpoint: d.target,
                        mode: d.mode.clone()?,
                        held: false,
                    },
                ))
            })
            .collect();
        SafetyMemory { found }
    }
}

/// Everything `decide` reads for one zone.
pub struct ZoneInputs<'a> {
    pub zone: &'a str,
    pub settings: &'a ZoneSettings,
    pub house: &'a HouseSettings,
    pub mode: Option<Mode>,
    pub reading: Reading,
    pub outdoor_mean: Option<f64>,
    pub home: bool,
    pub holiday: bool,
    pub overrides: &'a [Override],
    pub devices: &'a [Device],
    pub local_now: NaiveDateTime,
    pub now: u64,
    pub safety_memory: Option<&'a SafetyMemory>,
    /// The cause of the zone's previous safety decision, so a limit that was crossed holds
    /// until the reading is clearly back inside it.
    pub previous_cause: Option<SafetyCause>,
}

/// How far back inside a limit the reading must be before a latched safety cause clears.
pub const SAFETY_HYSTERESIS: f64 = 0.5;

/// Clamp to the device's range and round to its step (0.5 when it has none).
pub fn fit(t: f64, caps: &DeviceCaps) -> f64 {
    let step = caps.temp_step.filter(|s| *s > 0.0).unwrap_or(DEFAULT_STEP);
    let mut v = (t / step).round() * step;
    if let Some(max) = caps.max_temp {
        v = v.min(max);
    }
    if let Some(min) = caps.min_temp {
        v = v.max(min);
    }
    v
}

pub(crate) fn mode_str(mode: Mode) -> &'static str {
    match mode {
        Mode::Heat => "heat",
        Mode::Cool => "cool",
    }
}

fn can(caps: &DeviceCaps, mode: &str) -> bool {
    caps.hvac_modes.iter().any(|m| m == mode)
}

/// What a device should be told to run `setpoint` in `mode`, or `None` when it cannot.
pub fn device_target(setpoint: Setpoint, mode: Mode, caps: &DeviceCaps) -> Option<DeviceTarget> {
    let m = mode_str(mode);
    match setpoint {
        Setpoint::Temp(t) => can(caps, m).then(|| DeviceTarget {
            setpoint: Some(fit(t, caps)),
            mode: m.to_string(),
            held: false,
        }),
        Setpoint::Off if can(caps, "off") => Some(DeviceTarget {
            setpoint: None,
            mode: "off".to_string(),
            held: false,
        }),
        Setpoint::Off => {
            let limit = match mode {
                Mode::Heat => caps.min_temp,
                Mode::Cool => caps.max_temp,
            }?;
            can(caps, m).then(|| DeviceTarget {
                setpoint: Some(fit(limit, caps)),
                mode: m.to_string(),
                held: false,
            })
        }
    }
}

fn temp(t: f64) -> String {
    format!("{t:.1} °C")
}

fn setpoint_text(sp: Setpoint) -> String {
    match sp {
        Setpoint::Temp(t) => temp(t),
        Setpoint::Off => "off".to_string(),
    }
}

/// The devices that can run `setpoint` in `mode`, with their targets.
fn targets(devices: &[Device], setpoint: Setpoint, mode: Mode) -> BTreeMap<String, DeviceTarget> {
    devices
        .iter()
        .filter_map(|d| Some((d.id.clone(), device_target(setpoint, mode, &d.caps)?)))
        .collect()
}

fn base_decision(inp: &ZoneInputs, source: Source, reason: String) -> Decision {
    let (reading, reading_age_s) = match inp.reading {
        Reading::Fresh { value, age_s, .. } => (Some(value), Some(age_s)),
        Reading::Stale { last } => (last, None),
        Reading::Missing => (None, None),
    };
    Decision {
        zone: inp.zone.to_string(),
        mode: inp.mode,
        setpoint: None,
        source,
        cause: None,
        reason,
        reading,
        reading_age_s,
        outdoor_mean: inp.outdoor_mean,
        home: inp.home,
        applied: false,
        at: inp.now,
        entities: BTreeMap::new(),
    }
}

fn safety_decision(inp: &ZoneInputs, cause: SafetyCause, reason: String) -> Decision {
    let mut d = base_decision(inp, Source::Safety, reason);
    d.cause = Some(cause);
    d
}

/// Decide what the zone's devices should do now. `applied` is left `false` for the hub to set.
pub fn decide(inp: &ZoneInputs) -> Decision {
    let mut d = if inp.settings.state == ZoneState::Off {
        base_decision(inp, Source::Idle, "Zone is off".to_string())
    } else {
        safety(inp)
            .or_else(|| pending(inp))
            .or_else(|| zone_override(inp))
            .unwrap_or_else(|| rules(inp))
    };
    if matches!(
        inp.reading,
        Reading::Fresh {
            sensor_stale: true,
            ..
        }
    ) && inp.settings.state != ZoneState::Off
    {
        d.reason
            .push_str("; the configured sensor is stale, using the devices' own readings");
    }
    d
}

fn safety(inp: &ZoneInputs) -> Option<Decision> {
    let house = inp.house;
    match inp.reading {
        Reading::Fresh { value, .. }
            if value < house.frost
                || (latched(inp, SafetyCause::Frost)
                    && value < house.frost + SAFETY_HYSTERESIS) =>
        {
            let sp = Setpoint::Temp(house.frost + 2.0);
            let reason = if value < house.frost {
                format!("Frost protection: {} < {}", temp(value), temp(house.frost))
            } else {
                format!(
                    "Frost protection: {}, holding until {}",
                    temp(value),
                    temp(house.frost + SAFETY_HYSTERESIS)
                )
            };
            let mut d = safety_decision(inp, SafetyCause::Frost, reason);
            d.mode = Some(Mode::Heat);
            d.setpoint = Some(sp);
            d.entities = inp
                .devices
                .iter()
                .filter_map(|dev| {
                    let t = dev
                        .target
                        .map_or(house.frost + 2.0, |c| c.max(house.frost + 2.0));
                    Some((
                        dev.id.clone(),
                        device_target(Setpoint::Temp(t), Mode::Heat, &dev.caps)?,
                    ))
                })
                .collect();
            Some(d)
        }
        Reading::Fresh { value, .. }
            if inp.mode == Some(Mode::Heat)
                && (value > house.overheat
                    || (latched(inp, SafetyCause::Overheat)
                        && value > house.overheat - SAFETY_HYSTERESIS)) =>
        {
            let base = inp
                .settings
                .rules
                .get(Mode::Heat)
                .map(|r| r.base)
                .and_then(|b| match b {
                    Setpoint::Temp(t) if t <= house.overheat => Some(Setpoint::Temp(t)),
                    _ => None,
                });
            let mut d = safety_decision(
                inp,
                SafetyCause::Overheat,
                format!(
                    "Overheat protection: {} > {}",
                    temp(value),
                    temp(house.overheat)
                ),
            );
            d.entities = match base {
                Some(sp) => {
                    d.setpoint = Some(sp);
                    targets(inp.devices, sp, Mode::Heat)
                }
                None => inp
                    .devices
                    .iter()
                    .filter_map(|dev| {
                        let sp = Setpoint::Temp(dev.caps.min_temp.unwrap_or(house.frost));
                        Some((dev.id.clone(), device_target(sp, Mode::Heat, &dev.caps)?))
                    })
                    .collect(),
            };
            if d.setpoint.is_none() {
                d.setpoint = d
                    .entities
                    .values()
                    .filter_map(|t| t.setpoint)
                    .min_by(f64::total_cmp)
                    .map(Setpoint::Temp);
            }
            Some(d)
        }
        Reading::Fresh { value, .. }
            if inp.mode == Some(Mode::Cool)
                && (value < house.overcool
                    || (latched(inp, SafetyCause::Overcool)
                        && value < house.overcool + SAFETY_HYSTERESIS)) =>
        {
            let mut d = safety_decision(
                inp,
                SafetyCause::Overcool,
                format!(
                    "Overcool protection: {} < {}",
                    temp(value),
                    temp(house.overcool)
                ),
            );
            d.setpoint = Some(Setpoint::Off);
            d.entities = targets(inp.devices, Setpoint::Off, Mode::Cool);
            let stuck: Vec<&str> = inp
                .devices
                .iter()
                .filter(|dev| can(&dev.caps, "cool") && !d.entities.contains_key(&dev.id))
                .map(|dev| dev.id.as_str())
                .collect();
            if !stuck.is_empty() {
                d.reason
                    .push_str(&format!("; cannot stop cooling: {}", stuck.join(", ")));
            }
            Some(d)
        }
        Reading::Fresh { .. } => None,
        Reading::Stale { .. } | Reading::Missing => stale(inp),
    }
}

fn latched(inp: &ZoneInputs, cause: SafetyCause) -> bool {
    inp.previous_cause == Some(cause)
}

/// The stale-reading row: no usable reading, so never let a schedule, override or away setpoint
/// through in an active zone.
fn stale(inp: &ZoneInputs) -> Option<Decision> {
    let what = match inp.reading {
        Reading::Stale { last: Some(v) } => format!("Reading is stale (last {})", temp(v)),
        Reading::Stale { last: None } => "Reading is stale".to_string(),
        _ => "No temperature reading".to_string(),
    };
    if inp.settings.state != ZoneState::Active {
        return Some(safety_decision(
            inp,
            SafetyCause::Stale,
            format!("{what}: not acting until approved rules exist"),
        ));
    }
    let house = inp.house;
    let rules = inp.mode.and_then(|m| inp.settings.rules.get(m));
    let (Some(mode), Some(rules)) = (inp.mode, rules) else {
        return Some(safety_decision(
            inp,
            SafetyCause::Stale,
            format!("{what}: no rules to fall back on, not acting"),
        ));
    };
    let base = rules.base;
    let mut d = safety_decision(
        inp,
        SafetyCause::Stale,
        format!("{what}: holding the safer of the current target and base"),
    );
    match mode {
        Mode::Heat => {
            let base_t = match base {
                Setpoint::Temp(b) => b,
                _ => house.frost,
            };
            d.setpoint = Some(Setpoint::Temp(base_t.max(house.frost)));
            d.entities = inp
                .devices
                .iter()
                .filter(|dev| dev.mode.as_deref() != Some("off"))
                .filter_map(|dev| {
                    let t = dev
                        .target
                        .map_or(base_t, |c| c.min(base_t))
                        .max(house.frost);
                    Some((
                        dev.id.clone(),
                        device_target(Setpoint::Temp(t), Mode::Heat, &dev.caps)?,
                    ))
                })
                .collect();
        }
        Mode::Cool => {
            d.setpoint = Some(base);
            d.entities = inp
                .devices
                .iter()
                .filter(|dev| dev.mode.as_deref() != Some("off"))
                .filter_map(|dev| {
                    let sp = match (base, dev.target) {
                        (Setpoint::Temp(b), Some(c)) => Setpoint::Temp(c.max(b)),
                        (Setpoint::Temp(b), None) => Setpoint::Temp(b),
                        _ => Setpoint::Off,
                    };
                    Some((dev.id.clone(), device_target(sp, Mode::Cool, &dev.caps)?))
                })
                .collect();
        }
    }
    Some(d)
}

/// A pending zone acts only on safety: restore what safety changed, or just show what it would do.
fn pending(inp: &ZoneInputs) -> Option<Decision> {
    if inp.settings.state != ZoneState::Pending {
        return None;
    }
    if let Some(mem) = inp.safety_memory {
        let mut d = base_decision(
            inp,
            Source::Idle,
            "Safety cleared: restoring previous settings".to_string(),
        );
        d.entities = mem.found.clone();
        return Some(d);
    }
    let choice = choose(inp, inp.mode);
    let mut d = base_decision(inp, Source::Idle, "Waiting for approved rules".to_string());
    if let Some(sp) = choice.setpoint {
        d.setpoint = Some(sp);
        d.reason
            .push_str(&format!("; would set {}", setpoint_text(sp)));
    }
    Some(d)
}

fn live(o: &Override, now: u64) -> bool {
    o.until > now
}

fn latest<'a>(it: impl Iterator<Item = &'a Override>) -> Option<&'a Override> {
    it.max_by_key(|o| o.until)
}

fn clock(inp: &ZoneInputs, until: u64) -> String {
    let secs = i64::try_from(until.saturating_sub(inp.now)).unwrap_or(i64::MAX / 2);
    (inp.local_now + Duration::seconds(secs))
        .format("%H:%M")
        .to_string()
}

/// A whole-zone override: every device follows it.
fn zone_override(inp: &ZoneInputs) -> Option<Decision> {
    let ov = latest(
        inp.overrides
            .iter()
            .filter(|o| o.entity.is_none() && live(o, inp.now)),
    )?;
    let mode = ov.mode.or(inp.mode)?;
    let sp = ov.setpoint.or_else(|| choose(inp, Some(mode)).setpoint)?;
    let mut d = base_decision(
        inp,
        Source::Override,
        format!(
            "Override to {} until {}",
            setpoint_text(sp),
            clock(inp, ov.until)
        ),
    );
    d.mode = Some(mode);
    d.setpoint = Some(sp);
    d.entities = targets(inp.devices, sp, mode);
    hold_devices(inp, &mut d, Some(sp));
    Some(d)
}

/// What the holiday, away and schedule rows choose.
struct Choice {
    source: Source,
    setpoint: Option<Setpoint>,
    reason: String,
}

fn choose(inp: &ZoneInputs, mode: Option<Mode>) -> Choice {
    let idle = |reason: &str| Choice {
        source: Source::Idle,
        setpoint: None,
        reason: reason.to_string(),
    };
    let Some(mode) = mode else {
        return idle("No heating or cooling season");
    };
    if inp.holiday {
        let sp = match mode {
            Mode::Heat => Setpoint::Temp(inp.house.frost),
            Mode::Cool => Setpoint::Off,
        };
        return Choice {
            source: Source::Holiday,
            setpoint: Some(sp),
            reason: format!("Holiday: {}", setpoint_text(sp)),
        };
    }
    if !inp.home {
        let away = &inp.settings.away_setpoint;
        let sp = Setpoint::Temp(match mode {
            Mode::Heat => away.heat,
            Mode::Cool => away.cool,
        });
        return Choice {
            source: Source::Away,
            setpoint: Some(sp),
            reason: format!("Nobody home: away setpoint {}", setpoint_text(sp)),
        };
    }
    match inp.settings.rules.get(mode) {
        None => idle(&format!("No rules for {} mode", mode_str(mode))),
        Some(r) => {
            let (sp, block) = r.at(inp.local_now);
            let reason = match block {
                Some(i) => format!("Schedule block {}: {}", i + 1, setpoint_text(sp)),
                None => format!("Schedule base: {}", setpoint_text(sp)),
            };
            Choice {
                source: Source::Schedule,
                setpoint: Some(sp),
                reason,
            }
        }
    }
}

/// Holiday, away and schedule, then per-device overrides on top.
fn rules(inp: &ZoneInputs) -> Decision {
    let choice = choose(inp, inp.mode);
    let mut d = base_decision(inp, choice.source, choice.reason);
    if let (Some(mode), Some(sp)) = (inp.mode, choice.setpoint) {
        d.setpoint = Some(sp);
        d.entities = targets(inp.devices, sp, mode);
    }
    hold_devices(inp, &mut d, choice.setpoint);
    d
}

/// Per-device overrides (hand changes) hold their device whatever the zone does (§7).
fn hold_devices(inp: &ZoneInputs, d: &mut Decision, fallback: Option<Setpoint>) {
    let zone_mode = d.mode.or(inp.mode);
    for dev in inp.devices {
        let Some(ov) = latest(
            inp.overrides
                .iter()
                .filter(|o| o.entity.as_deref() == Some(dev.id.as_str()) && live(o, inp.now)),
        ) else {
            continue;
        };
        let (Some(mode), Some(sp)) = (ov.mode.or(zone_mode), ov.setpoint.or(fallback)) else {
            continue;
        };
        if let Some(mut t) = device_target(sp, mode, &dev.caps) {
            t.held = true;
            d.entities.insert(dev.id.clone(), t);
            d.reason
                .push_str(&format!("; {} held until {}", dev.id, clock(inp, ov.until)));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::doc::{
        AwaySetpoints, Block, Day, HhMm, ModeRules, OverrideOrigin, Rules, ZoneSettings,
    };

    const NOW: u64 = 1_000_000;

    fn caps(modes: &[&str]) -> DeviceCaps {
        DeviceCaps {
            hvac_modes: modes.iter().map(|m| m.to_string()).collect(),
            min_temp: Some(5.0),
            max_temp: Some(30.0),
            temp_step: Some(0.5),
        }
    }

    fn dev(id: &str, modes: &[&str], target: Option<f64>) -> Device {
        Device {
            id: id.into(),
            caps: caps(modes),
            target,
            mode: Some("heat".into()),
        }
    }

    fn trv() -> Device {
        dev("trv", &["off", "heat"], Some(21.0))
    }

    fn fresh(v: f64) -> Reading {
        Reading::Fresh {
            value: v,
            age_s: 30,
            from_sensor: true,
            sensor_stale: false,
        }
    }

    /// Wednesday 10:00 local.
    fn local() -> NaiveDateTime {
        NaiveDateTime::parse_from_str("2026-10-21 10:00", "%Y-%m-%d %H:%M").unwrap()
    }

    fn all_days() -> Vec<Day> {
        vec![
            Day::Mon,
            Day::Tue,
            Day::Wed,
            Day::Thu,
            Day::Fri,
            Day::Sat,
            Day::Sun,
        ]
    }

    fn mode_rules(base: Setpoint) -> ModeRules {
        ModeRules {
            base,
            blocks: vec![Block {
                days: all_days(),
                from: HhMm(420),
                to: HhMm(1320),
                setpoint: Setpoint::Temp(20.5),
                evidence: None,
            }],
        }
    }

    fn active(base: Setpoint) -> ZoneSettings {
        ZoneSettings {
            state: ZoneState::Active,
            rules: Rules {
                heat: Some(mode_rules(base)),
                cool: Some(mode_rules(Setpoint::Temp(26.0))),
            },
            away_setpoint: AwaySetpoints::default(),
            ..Default::default()
        }
    }

    struct Case {
        settings: ZoneSettings,
        house: HouseSettings,
        mode: Option<Mode>,
        reading: Reading,
        home: bool,
        holiday: bool,
        overrides: Vec<Override>,
        devices: Vec<Device>,
        memory: Option<SafetyMemory>,
        previous: Option<SafetyCause>,
    }

    impl Case {
        fn new() -> Self {
            Case {
                settings: active(Setpoint::Temp(17.0)),
                house: HouseSettings::default(),
                mode: Some(Mode::Heat),
                reading: fresh(19.0),
                home: true,
                holiday: false,
                overrides: vec![],
                devices: vec![trv()],
                memory: None,
                previous: None,
            }
        }

        fn run(&self) -> Decision {
            decide(&ZoneInputs {
                zone: "z",
                settings: &self.settings,
                house: &self.house,
                mode: self.mode,
                reading: self.reading,
                outdoor_mean: Some(8.0),
                home: self.home,
                holiday: self.holiday,
                overrides: &self.overrides,
                devices: &self.devices,
                local_now: local(),
                now: NOW,
                safety_memory: self.memory.as_ref(),
                previous_cause: self.previous,
            })
        }
    }

    fn target(sp: Option<f64>, mode: &str, held: bool) -> DeviceTarget {
        DeviceTarget {
            setpoint: sp,
            mode: mode.into(),
            held,
        }
    }

    fn zone_ov(sp: f64, until: u64) -> Override {
        Override {
            entity: None,
            setpoint: Some(Setpoint::Temp(sp)),
            mode: None,
            until,
            origin: OverrideOrigin::User,
        }
    }

    #[test]
    fn off_zone_does_nothing() {
        let mut c = Case::new();
        c.settings.state = ZoneState::Off;
        c.reading = fresh(2.0);
        let d = c.run();
        assert_eq!(d.source, Source::Idle);
        assert!(d.entities.is_empty());
        assert_eq!(d.reason, "Zone is off");
        assert!(!d.applied);
        assert_eq!(d.at, NOW);
    }

    #[test]
    fn frost_beats_override_in_active_zone() {
        let mut c = Case::new();
        c.devices = vec![dev("trv", &["off", "heat"], Some(5.0))];
        c.reading = fresh(6.5);
        c.overrides = vec![zone_ov(22.0, NOW + 600)];
        let d = c.run();
        assert_eq!(d.source, Source::Safety);
        assert_eq!(d.reason, "Frost protection: 6.5 °C < 7.0 °C");
        assert_eq!(d.entities["trv"], target(Some(9.0), "heat", false));
        assert_eq!(d.reading, Some(6.5));
        assert_eq!(d.reading_age_s, Some(30));
    }

    #[test]
    fn every_safety_decision_names_its_cause() {
        let mut c = Case::new();
        c.reading = fresh(6.5);
        assert_eq!(c.run().cause, Some(SafetyCause::Frost));
        c.reading = fresh(31.0);
        assert_eq!(c.run().cause, Some(SafetyCause::Overheat));
        c.mode = Some(Mode::Cool);
        c.reading = fresh(15.0);
        assert_eq!(c.run().cause, Some(SafetyCause::Overcool));
        c.reading = Reading::Missing;
        assert_eq!(c.run().cause, Some(SafetyCause::Stale));
        c.reading = fresh(19.0);
        assert_eq!(c.run().cause, None);
    }

    #[test]
    fn frost_holds_until_half_a_degree_above() {
        let mut c = Case::new();
        c.devices = vec![dev("trv", &["off", "heat"], Some(5.0))];
        c.previous = Some(SafetyCause::Frost);
        for (reading, safety) in [
            (6.9, true),
            (7.0, true),
            (7.2, true),
            (7.49, true),
            (7.5, false),
        ] {
            c.reading = fresh(reading);
            assert_eq!(c.run().source == Source::Safety, safety, "{reading}");
        }
        // Not latched: the limit itself.
        c.previous = None;
        c.reading = fresh(7.0);
        assert_ne!(c.run().source, Source::Safety);
        c.reading = fresh(6.99);
        assert_eq!(c.run().source, Source::Safety);
        // Another latched cause does not hold frost.
        c.previous = Some(SafetyCause::Stale);
        c.reading = fresh(7.2);
        assert_ne!(c.run().source, Source::Safety);
    }

    #[test]
    fn overheat_and_overcool_hold_with_hysteresis() {
        let mut c = Case::new();
        c.previous = Some(SafetyCause::Overheat);
        for (reading, safety) in [(30.1, true), (29.6, true), (29.5, false)] {
            c.reading = fresh(reading);
            assert_eq!(c.run().source == Source::Safety, safety, "{reading}");
        }
        c.mode = Some(Mode::Cool);
        c.previous = Some(SafetyCause::Overcool);
        for (reading, safety) in [(15.9, true), (16.4, true), (16.5, false)] {
            c.reading = fresh(reading);
            assert_eq!(c.run().source == Source::Safety, safety, "{reading}");
        }
    }

    #[test]
    fn frost_acts_in_pending_zone() {
        let mut c = Case::new();
        c.settings.state = ZoneState::Pending;
        c.devices = vec![dev("trv", &["off", "heat"], Some(5.0))];
        c.reading = fresh(6.5);
        let d = c.run();
        assert_eq!(d.source, Source::Safety);
        assert_eq!(d.entities["trv"], target(Some(9.0), "heat", false));
    }

    #[test]
    fn overheat_uses_base_or_device_min() {
        let mut c = Case::new();
        c.reading = fresh(31.0);
        let d = c.run();
        assert_eq!(d.source, Source::Safety);
        assert_eq!(d.entities["trv"], target(Some(17.0), "heat", false));
        c.settings = active(Setpoint::Temp(31.0));
        let d = c.run();
        assert_eq!(d.entities["trv"], target(Some(5.0), "heat", false));
        c.settings = active(Setpoint::Off);
        let d = c.run();
        assert_eq!(d.entities["trv"], target(Some(5.0), "heat", false));
    }

    #[test]
    fn overheat_ignored_in_cool_mode() {
        let mut c = Case::new();
        c.mode = Some(Mode::Cool);
        c.devices = vec![dev("ac", &["off", "cool"], Some(25.0))];
        c.reading = fresh(31.0);
        assert_eq!(c.run().source, Source::Schedule);
    }

    #[test]
    fn overcool_stops_cooling() {
        let mut c = Case::new();
        c.mode = Some(Mode::Cool);
        c.devices = vec![dev("ac", &["off", "cool"], Some(24.0))];
        c.reading = fresh(15.0);
        let d = c.run();
        assert_eq!(d.source, Source::Safety);
        assert_eq!(d.entities["ac"], target(None, "off", false));
    }

    #[test]
    fn overcool_without_off_uses_max_temp() {
        let mut c = Case::new();
        c.mode = Some(Mode::Cool);
        c.devices = vec![dev("ac", &["cool"], Some(24.0))];
        c.reading = fresh(15.0);
        let d = c.run();
        assert_eq!(d.entities["ac"], target(Some(30.0), "cool", false));
    }

    #[test]
    fn stale_active_heat_takes_lower_of_target_and_base() {
        let mut c = Case::new();
        c.reading = Reading::Stale { last: Some(19.0) };
        let d = c.run(); // target 21, base 17
        assert_eq!(d.source, Source::Safety);
        assert_eq!(d.entities["trv"], target(Some(17.0), "heat", false));
        assert!(d.reason.contains("stale"));
        c.devices = vec![dev("trv", &["off", "heat"], Some(15.0))];
        assert_eq!(c.run().entities["trv"].setpoint, Some(15.0));
        c.devices = vec![dev("trv", &["off", "heat"], Some(3.0))];
        assert_eq!(c.run().entities["trv"].setpoint, Some(7.0));
    }

    #[test]
    fn stale_active_heat_base_off_falls_back_to_frost() {
        let mut c = Case::new();
        c.settings = active(Setpoint::Off);
        c.reading = Reading::Missing;
        let d = c.run();
        assert_eq!(d.entities["trv"], target(Some(7.0), "heat", false));
    }

    #[test]
    fn stale_active_cool_takes_higher() {
        let mut c = Case::new();
        c.mode = Some(Mode::Cool);
        c.reading = Reading::Stale { last: None };
        c.devices = vec![dev("ac", &["off", "cool"], Some(22.0))];
        let d = c.run(); // base 26
        assert_eq!(d.entities["ac"], target(Some(26.0), "cool", false));
        c.devices = vec![dev("ac", &["off", "cool"], Some(27.0))];
        assert_eq!(c.run().entities["ac"].setpoint, Some(27.0));
    }

    #[test]
    fn stale_never_lets_schedule_override_or_away_through() {
        let mut c = Case::new();
        c.reading = Reading::Missing;
        c.overrides = vec![zone_ov(25.0, NOW + 600)];
        c.home = false;
        c.holiday = true;
        let d = c.run();
        assert_eq!(d.source, Source::Safety);
        assert_eq!(d.entities["trv"].setpoint, Some(17.0));
    }

    #[test]
    fn stale_pending_only_notifies() {
        let mut c = Case::new();
        c.settings.state = ZoneState::Pending;
        c.reading = Reading::Missing;
        let d = c.run();
        assert_eq!(d.source, Source::Safety);
        assert!(d.entities.is_empty());
        assert!(d.reason.contains("No temperature reading"));
    }

    #[test]
    fn stale_with_season_off_commands_nothing() {
        let mut c = Case::new();
        c.mode = None;
        c.reading = Reading::Missing;
        let d = c.run();
        assert_eq!(d.source, Source::Safety);
        assert!(d.entities.is_empty());
    }

    fn device_ov(id: &str, sp: f64) -> Override {
        Override {
            entity: Some(id.into()),
            setpoint: Some(Setpoint::Temp(sp)),
            mode: Some(Mode::Heat),
            until: NOW + 600,
            origin: OverrideOrigin::Hand,
        }
    }

    #[test]
    fn stale_no_season_beats_overrides() {
        let mut c = Case::new();
        c.mode = None;
        c.reading = Reading::Stale { last: Some(19.0) };
        let mut zone = zone_ov(25.0, NOW + 600);
        zone.mode = Some(Mode::Heat);
        c.overrides = vec![zone];
        let d = c.run();
        assert_eq!(d.source, Source::Safety);
        assert!(d.entities.is_empty());
        c.overrides = vec![device_ov("trv", 25.0)];
        let d = c.run();
        assert_eq!(d.source, Source::Safety);
        assert!(d.entities.is_empty());
    }

    #[test]
    fn stale_without_rules_for_mode_only_notifies() {
        let mut c = Case::new();
        c.settings.rules.heat = None;
        c.reading = Reading::Missing;
        let d = c.run();
        assert_eq!(d.source, Source::Safety);
        assert!(d.entities.is_empty());
    }

    #[test]
    fn stale_leaves_off_devices_alone() {
        let mut c = Case::new();
        c.reading = Reading::Missing;
        let mut off = dev("off", &["off", "heat"], Some(5.0));
        off.mode = Some("off".into());
        c.devices = vec![trv(), off];
        let d = c.run();
        assert!(d.entities.contains_key("trv"));
        assert!(!d.entities.contains_key("off"));
    }

    #[test]
    fn capture_skips_devices_with_unknown_mode() {
        let mut unknown = dev("u", &["heat"], Some(20.0));
        unknown.mode = None;
        let m = SafetyMemory::capture(&[trv(), unknown]);
        assert!(m.found.contains_key("trv"));
        assert!(!m.found.contains_key("u"));
    }

    #[test]
    fn device_override_holds_under_zone_override() {
        let mut c = Case::new();
        c.devices = vec![trv(), dev("trv2", &["off", "heat"], Some(21.0))];
        c.overrides = vec![zone_ov(22.0, NOW + 3600), device_ov("trv2", 24.0)];
        let d = c.run();
        assert_eq!(d.source, Source::Override);
        assert_eq!(d.entities["trv"], target(Some(22.0), "heat", false));
        assert_eq!(d.entities["trv2"], target(Some(24.0), "heat", true));
    }

    #[test]
    fn zone_override_mode_without_setpoint_uses_its_own_mode_rules() {
        let mut c = Case::new();
        c.devices = vec![dev("ac", &["off", "cool"], Some(24.0))];
        c.overrides = vec![Override {
            entity: None,
            setpoint: None,
            mode: Some(Mode::Cool),
            until: NOW + 600,
            origin: OverrideOrigin::User,
        }];
        let d = c.run(); // heat season, cool rules block 20.5
        assert_eq!(d.source, Source::Override);
        assert_eq!(d.mode, Some(Mode::Cool));
        assert_eq!(d.entities["ac"], target(Some(20.5), "cool", false));
    }

    #[test]
    fn frost_never_lowers_a_warmer_device() {
        let mut c = Case::new();
        c.reading = fresh(6.5);
        c.devices = vec![
            dev("hot", &["heat"], Some(21.0)),
            dev("cold", &["heat"], Some(3.0)),
        ];
        let d = c.run();
        assert_eq!(d.entities["hot"].setpoint, Some(21.0));
        assert_eq!(d.entities["cold"].setpoint, Some(9.0));
        assert_eq!(d.reason, "Frost protection: 6.5 °C < 7.0 °C");
    }

    #[test]
    fn overcool_names_devices_it_cannot_stop() {
        let mut c = Case::new();
        c.mode = Some(Mode::Cool);
        c.reading = fresh(15.0);
        let mut stuck = dev("stuck", &["cool"], Some(24.0));
        stuck.caps.max_temp = None;
        c.devices = vec![dev("ac", &["off", "cool"], Some(24.0)), stuck];
        let d = c.run();
        assert!(d.entities.contains_key("ac"));
        assert!(!d.entities.contains_key("stuck"));
        assert!(
            d.reason.contains("cannot stop cooling: stuck"),
            "{}",
            d.reason
        );
    }

    #[test]
    fn pending_restores_after_safety() {
        let mut c = Case::new();
        c.settings.state = ZoneState::Pending;
        c.memory = Some(SafetyMemory {
            found: [("trv".to_string(), target(Some(19.0), "heat", false))].into(),
        });
        let d = c.run();
        assert_eq!(d.source, Source::Idle);
        assert_eq!(d.entities["trv"], target(Some(19.0), "heat", false));
        assert_eq!(d.reason, "Safety cleared: restoring previous settings");
    }

    #[test]
    fn pending_shows_would_do() {
        let mut c = Case::new();
        c.settings.state = ZoneState::Pending;
        let d = c.run();
        assert_eq!(d.source, Source::Idle);
        assert_eq!(d.setpoint, Some(Setpoint::Temp(20.5)));
        assert!(d.entities.is_empty());
        assert!(d.reason.starts_with("Waiting for approved rules"));
        c.settings.rules = Rules::default();
        let d = c.run();
        assert_eq!(d.setpoint, None);
        assert!(d.entities.is_empty());
    }

    #[test]
    fn pending_ignores_overrides() {
        let mut c = Case::new();
        c.settings.state = ZoneState::Pending;
        c.overrides = vec![zone_ov(25.0, NOW + 600)];
        let d = c.run();
        assert_eq!(d.source, Source::Idle);
        assert!(d.entities.is_empty());
    }

    #[test]
    fn zone_override_wins_over_schedule() {
        let mut c = Case::new();
        c.overrides = vec![zone_ov(22.0, NOW + 3600)];
        let d = c.run();
        assert_eq!(d.source, Source::Override);
        assert_eq!(d.entities["trv"], target(Some(22.0), "heat", false));
        assert_eq!(d.reason, "Override to 22.0 °C until 11:00");
    }

    #[test]
    fn expired_override_is_ignored() {
        let mut c = Case::new();
        c.overrides = vec![zone_ov(22.0, NOW)];
        let d = c.run();
        assert_eq!(d.source, Source::Schedule);
        assert_eq!(d.entities["trv"].setpoint, Some(20.5));
    }

    #[test]
    fn device_override_holds_only_that_device() {
        let mut c = Case::new();
        c.devices = vec![trv(), dev("trv2", &["off", "heat"], Some(21.0))];
        c.overrides = vec![Override {
            entity: Some("trv2".into()),
            setpoint: Some(Setpoint::Temp(23.0)),
            mode: None,
            until: NOW + 3600,
            origin: OverrideOrigin::Hand,
        }];
        let d = c.run();
        assert_eq!(d.source, Source::Schedule);
        assert_eq!(d.entities["trv"], target(Some(20.5), "heat", false));
        assert_eq!(d.entities["trv2"], target(Some(23.0), "heat", true));
        assert!(
            d.reason.ends_with("; trv2 held until 11:00"),
            "{}",
            d.reason
        );
    }

    #[test]
    fn holiday_heat_is_frost() {
        let mut c = Case::new();
        c.holiday = true;
        let d = c.run();
        assert_eq!(d.source, Source::Holiday);
        assert_eq!(d.entities["trv"].setpoint, Some(7.0));
        c.mode = Some(Mode::Cool);
        c.devices = vec![dev("ac", &["off", "cool"], Some(24.0))];
        let d = c.run();
        assert_eq!(d.source, Source::Holiday);
        assert_eq!(d.entities["ac"], target(None, "off", false));
    }

    #[test]
    fn away_uses_away_setpoint() {
        let mut c = Case::new();
        c.home = false;
        let d = c.run();
        assert_eq!(d.source, Source::Away);
        assert_eq!(d.entities["trv"].setpoint, Some(16.0));
        assert!(!d.home);
    }

    #[test]
    fn schedule_block_and_base() {
        let c = Case::new();
        let d = c.run();
        assert_eq!(d.source, Source::Schedule);
        assert_eq!(d.setpoint, Some(Setpoint::Temp(20.5)));
        assert_eq!(d.entities["trv"], target(Some(20.5), "heat", false));
        let mut c = Case::new();
        c.settings.rules.heat.as_mut().unwrap().blocks.clear();
        let d = c.run();
        assert_eq!(d.setpoint, Some(Setpoint::Temp(17.0)));
        assert_eq!(d.entities["trv"].setpoint, Some(17.0));
    }

    #[test]
    fn rules_off_setpoint_turns_device_off() {
        let mut c = Case::new();
        c.settings.rules.heat.as_mut().unwrap().blocks[0].setpoint = Setpoint::Off;
        let d = c.run();
        assert_eq!(d.entities["trv"], target(None, "off", false));
    }

    #[test]
    fn device_without_mode_is_left_out() {
        let mut c = Case::new();
        c.devices = vec![trv(), dev("ac", &["off", "cool"], Some(24.0))];
        let d = c.run();
        assert!(d.entities.contains_key("trv"));
        assert!(!d.entities.contains_key("ac"));
    }

    #[test]
    fn fit_clamps_and_rounds() {
        let c = caps(&["heat"]);
        assert_eq!(fit(30.3, &c), 30.0);
        assert_eq!(fit(20.26, &c), 20.5);
        assert_eq!(fit(20.24, &c), 20.0);
        assert_eq!(fit(2.0, &c), 5.0);
        let none = DeviceCaps {
            temp_step: None,
            ..c
        };
        assert_eq!(fit(20.26, &none), 20.5);
    }

    #[test]
    fn device_target_off_without_off_mode() {
        let heat_only = caps(&["heat"]);
        assert_eq!(
            device_target(Setpoint::Off, Mode::Heat, &heat_only),
            Some(target(Some(5.0), "heat", false))
        );
        assert_eq!(
            device_target(Setpoint::Temp(20.0), Mode::Cool, &heat_only),
            None
        );
        assert_eq!(device_target(Setpoint::Off, Mode::Cool, &heat_only), None);
    }

    #[test]
    fn no_mode_is_idle() {
        let mut c = Case::new();
        c.mode = None;
        let d = c.run();
        assert_eq!(d.source, Source::Idle);
        assert!(d.entities.is_empty());
    }

    #[test]
    fn no_rules_for_mode_is_idle() {
        let mut c = Case::new();
        c.settings.rules.heat = None;
        let d = c.run();
        assert_eq!(d.source, Source::Idle);
        assert!(d.entities.is_empty());
    }

    #[test]
    fn stale_sensor_is_mentioned_in_reason() {
        let mut c = Case::new();
        c.reading = Reading::Fresh {
            value: 19.0,
            age_s: 10,
            from_sensor: false,
            sensor_stale: true,
        };
        let d = c.run();
        assert_eq!(d.source, Source::Schedule);
        assert!(d.reason.contains("configured sensor is stale"));
        assert!(d.reason.contains("devices' own readings"));
    }

    #[test]
    fn safety_memory_captures_devices() {
        let m = SafetyMemory::capture(&[trv()]);
        assert_eq!(m.found["trv"], target(Some(21.0), "heat", false));
    }
}
