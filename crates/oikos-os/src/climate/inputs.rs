//! What the engine reads for one zone: devices, room reading, presence, holiday and outdoor
//! temperature, gathered from the registry, the retained documents and the runtime state.

use std::collections::HashMap;

use chrono::NaiveDateTime;
use chrono_tz::Tz;
use oikos_climate::decide::{Device, DeviceCaps, SafetyMemory, ZoneInputs};
use oikos_climate::doc::{HouseSettings, Mode, Override, SafetyCause, ZoneSettings};
use oikos_climate::schedule::{Reading, ReadingSource, room_reading, season_mode};
use oikos_core::model::{Availability, EntityState, Kind, StateValue};

use super::GROUP_PREFIX;
use super::hub::zone_devices;
use crate::hub::Hub;

/// Step assumed for a device that does not advertise one; the same as the decision tree's.
const DEFAULT_STEP: f64 = 0.5;
/// Age given to a report whose time is unknown: older than any `stale_after`.
const UNKNOWN_AGE_S: u64 = u64::MAX / 4;
/// How long the history mean of the outdoor sensor is reused.
pub(crate) const OUTDOOR_CACHE_S: u64 = 600;

/// [`ZoneInputs`] with everything owned, so the registry lock is long gone when `decide` runs.
pub(crate) struct OwnedZoneInputs {
    pub zone: String,
    pub settings: ZoneSettings,
    pub house: HouseSettings,
    pub mode: Option<Mode>,
    pub reading: Reading,
    pub outdoor_mean: Option<f64>,
    pub home: bool,
    pub holiday: bool,
    pub overrides: Vec<Override>,
    /// The devices the engine may command: reachable, with a known state.
    pub devices: Vec<Device>,
    /// Every climate entity the zone lists, reachable or not.
    pub entity_ids: Vec<String>,
    pub local_now: NaiveDateTime,
    pub now: u64,
    pub safety_memory: Option<SafetyMemory>,
    /// The cause of the zone's previous safety decision, set by the caller.
    pub previous_cause: Option<SafetyCause>,
}

impl OwnedZoneInputs {
    pub(crate) fn borrowed(&self) -> ZoneInputs<'_> {
        ZoneInputs {
            zone: &self.zone,
            settings: &self.settings,
            house: &self.house,
            mode: self.mode,
            reading: self.reading,
            outdoor_mean: self.outdoor_mean,
            home: self.home,
            holiday: self.holiday,
            overrides: &self.overrides,
            devices: &self.devices,
            local_now: self.local_now,
            now: self.now,
            safety_memory: self.safety_memory.as_ref(),
            previous_cause: self.previous_cause,
        }
    }
}

/// The wall-clock time of the house at `now`.
pub(crate) fn local_time(tz: Tz, now: u64) -> NaiveDateTime {
    i64::try_from(now)
        .ok()
        .and_then(|secs| chrono::DateTime::from_timestamp(secs, 0))
        .unwrap_or(chrono::DateTime::UNIX_EPOCH)
        .with_timezone(&tz)
        .naive_local()
}

/// The temperature step a device advertises, or the default.
pub(crate) fn step_from(advertised: Option<f64>) -> f64 {
    advertised.filter(|s| *s > 0.0).unwrap_or(DEFAULT_STEP)
}

/// The device's temperature step, or the default.
pub(crate) fn step_of(caps: &DeviceCaps) -> f64 {
    step_from(caps.temp_step)
}

/// The climate entities of `zone`: `settings.entities`, else the area's that no other zone
/// lists, never a group.
pub(crate) fn zone_entity_ids(hub: &Hub, zone: &str, settings: &ZoneSettings) -> Vec<String> {
    let claims = hub.climate_claims();
    match &settings.entities {
        Some(_) => {
            let listed = zone_devices(zone, settings, &[], &claims);
            let reg = hub.read();
            listed
                .into_iter()
                .filter(|id| {
                    !id.starts_with(GROUP_PREFIX)
                        && reg.entity(id).is_some_and(|e| e.kind == Kind::Climate)
                })
                .collect()
        }
        None => {
            let area = hub
                .climate_areas()
                .remove(zone)
                .map(|(_, devices)| devices)
                .unwrap_or_default();
            zone_devices(zone, settings, &area, &claims)
        }
    }
}

/// The numeric value of a sensor-like state: a number, a numeric text, or a climate's reading.
fn number_of(state: &EntityState) -> Option<f64> {
    if state.availability == Availability::Offline {
        return None;
    }
    match &state.value {
        StateValue::Number(n) => Some(*n),
        StateValue::Text(text) => text.trim().parse().ok(),
        StateValue::Climate(climate) => climate.current,
        _ => None,
    }
}

/// How old the last report of `id` is.
fn age_of(seen: &HashMap<String, u64>, id: &str, state: &EntityState, now: u64) -> u64 {
    let last = seen.get(id).copied().max(state.last_update);
    last.map_or(UNKNOWN_AGE_S, |at| now.saturating_sub(at))
}

/// Whether an entity says someone is home: `Some(false)` is a definite no, `None` is unknown
/// (no value yet, or unreachable).
fn home_of(state: &EntityState) -> Option<bool> {
    if state.availability == Availability::Offline {
        return None;
    }
    match &state.value {
        StateValue::Text(text) => match text.trim() {
            "" | "unknown" | "unavailable" => None,
            other => Some(other == "home"),
        },
        StateValue::OnOff(on) => Some(*on),
        _ => None,
    }
}

/// Whether the house is home right now: some presence entity says so, or none says anything
/// definite (an unknown tracker never sends the house away). No presence entity means home.
pub(crate) fn presence_home_now(hub: &Hub, house: &HouseSettings) -> bool {
    if house.presence.is_empty() {
        return true;
    }
    let reg = hub.read();
    let known: Vec<bool> = house
        .presence
        .iter()
        .filter_map(|id| reg.state(id).and_then(home_of))
        .collect();
    known.is_empty() || known.contains(&true)
}

fn holiday_now(hub: &Hub, house: &HouseSettings) -> bool {
    house
        .holiday_entity
        .as_ref()
        .is_some_and(|id| hub.read().state(id).and_then(home_of).unwrap_or(false))
}

/// The room reading sources of a zone: its sensor, and its devices' own readings.
fn reading_sources(
    hub: &Hub,
    settings: &ZoneSettings,
    entity_ids: &[String],
    seen: &HashMap<String, u64>,
    now: u64,
) -> (Option<ReadingSource>, Vec<ReadingSource>) {
    let reg = hub.read();
    let source = |id: &str| {
        let state = reg.state(id)?;
        Some(ReadingSource {
            value: number_of(state)?,
            age_s: age_of(seen, id, state, now),
        })
    };
    let sensor = settings.sensor.as_deref().and_then(source);
    let devices = entity_ids.iter().filter_map(|id| source(id)).collect();
    (sensor, devices)
}

/// The reading and its age for the read view, recomputed from what is seen now.
pub(crate) fn view_reading(
    hub: &Hub,
    settings: &ZoneSettings,
    entity_ids: &[String],
    stale_after_s: u64,
    now: u64,
    seen: &HashMap<String, u64>,
) -> (Option<f64>, Option<u64>) {
    let (sensor, devices) = reading_sources(hub, settings, entity_ids, seen, now);
    match room_reading(sensor, &devices, stale_after_s) {
        Reading::Fresh { value, age_s, .. } => (Some(value), Some(age_s)),
        Reading::Stale { last } => {
            let youngest = sensor
                .iter()
                .chain(&devices)
                .map(|source| source.age_s)
                .min()
                .filter(|age| *age < UNKNOWN_AGE_S);
            (last, youngest)
        }
        Reading::Missing => (None, None),
    }
}

/// The mean outdoor temperature: the history's, while its cache is fresh, else the mean of the
/// live readings seen since boot.
pub(crate) fn outdoor_mean(hub: &Hub, now: u64) -> Option<f64> {
    let rt = hub.climate_runtime();
    if let Some((fetched, Some(mean))) = rt.outdoor_cache
        && now.saturating_sub(fetched) < OUTDOOR_CACHE_S
    {
        return Some(mean);
    }
    let (sum, n) = rt
        .outdoor_live
        .iter()
        .fold((0.0, 0u32), |(sum, n), (_, v)| (sum + v, n + 1));
    (n > 0).then(|| sum / f64::from(n))
}

/// The current reading of the outdoor sensor, when fresh.
pub(crate) fn outdoor_reading(hub: &Hub, house: &HouseSettings, now: u64) -> Option<f64> {
    let id = house.outdoor_entity.as_deref()?;
    let seen = hub.climate_runtime().seen_at.clone();
    let reg = hub.read();
    let state = reg.state(id)?;
    let age = age_of(&seen, id, state, now);
    (age <= u64::from(house.stale_after_min) * 60)
        .then(|| number_of(state))
        .flatten()
}

/// Everything `decide` needs for `zone`, or `None` while the engine is off.
pub(crate) fn zone_inputs_owned(hub: &Hub, zone: &str, now: u64) -> Option<OwnedZoneInputs> {
    hub.climate.as_ref()?;
    let (house, settings, overrides) = {
        let store = hub.climate_store();
        let docs = store.zones.get(zone);
        (
            store.house.clone(),
            docs.map(|d| d.settings.clone()).unwrap_or_default(),
            docs.map(|d| d.overrides.clone()).unwrap_or_default(),
        )
    };
    let entity_ids = zone_entity_ids(hub, zone, &settings);
    let seen = hub.climate_runtime().seen_at.clone();
    let devices: Vec<Device> = {
        let reg = hub.read();
        entity_ids
            .iter()
            .filter_map(|id| {
                let entity = reg.entity(id)?;
                let state = reg.state(id)?;
                let StateValue::Climate(climate) = &state.value else {
                    return None;
                };
                (state.availability != Availability::Offline).then(|| Device {
                    id: id.clone(),
                    caps: DeviceCaps {
                        hvac_modes: entity.capabilities.hvac_modes.clone(),
                        min_temp: entity.capabilities.min_temp,
                        max_temp: entity.capabilities.max_temp,
                        temp_step: entity.capabilities.temp_step,
                    },
                    target: climate.target,
                    mode: climate.mode.clone(),
                })
            })
            .collect()
    };
    let stale_after_s = u64::from(house.stale_after_min) * 60;
    let (sensor, device_readings) = reading_sources(hub, &settings, &entity_ids, &seen, now);
    let reading = room_reading(sensor, &device_readings, stale_after_s);
    let outdoor_mean = outdoor_mean(hub, now);
    let home_now = presence_home_now(hub, &house);
    let holiday = holiday_now(hub, &house);
    let (mode, home, safety_memory) = {
        let rt = hub.climate_runtime();
        let grace_s = u64::from(house.away_grace_min) * 60;
        let recently_home = rt
            .last_home_at
            .is_some_and(|at| now.saturating_sub(at) <= grace_s);
        (
            season_mode(&house, outdoor_mean, rt.previous_mode),
            home_now || recently_home,
            rt.safety_memory.get(zone).cloned(),
        )
    };
    Some(OwnedZoneInputs {
        zone: zone.to_owned(),
        settings,
        house,
        mode,
        reading,
        outdoor_mean,
        home,
        holiday,
        overrides,
        devices,
        entity_ids,
        local_now: local_time(hub.zone(), now),
        now,
        safety_memory,
        previous_cause: None,
    })
}
