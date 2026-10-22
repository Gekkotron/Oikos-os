//! The climate engine's hub side: the retained documents, the read view and the evaluation loop.

pub(crate) mod alerts;
pub(crate) mod engine;
pub(crate) mod hub;
pub(crate) mod inputs;
pub mod store;

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Mutex;

use oikos_climate::decide::SafetyMemory;
use oikos_climate::doc::{Decision, DeviceTarget, Mode};
use oikos_climate::ledger::Ledger;
use oikos_core::model::Availability;

pub use store::{Applied, ClimateStore, ZoneDocs};

/// Group entities are published by the hub itself; they are never a zone's device.
pub(crate) const GROUP_PREFIX: &str = "group_";

/// What the hub keeps while `[climate] enabled`: the topic prefix, the retained documents and
/// the engine's runtime state.
pub struct ClimateHub {
    pub(crate) prefix: String,
    pub(crate) store: Mutex<ClimateStore>,
    pub(crate) runtime: Mutex<ClimateRuntime>,
    /// Held for a whole evaluation pass, so two callers never command the same device at once.
    pub(crate) pass: tokio::sync::Mutex<()>,
    /// Held while an override list is read, changed and written, so two writers never lose
    /// each other's change. Never held across an `await`.
    pub(crate) overrides_write: Mutex<()>,
}

impl ClimateHub {
    pub(crate) fn new(prefix: &str) -> Self {
        Self {
            prefix: prefix.trim_end_matches('/').to_owned(),
            store: Mutex::new(ClimateStore::default()),
            runtime: Mutex::new(ClimateRuntime::default()),
            pass: tokio::sync::Mutex::new(()),
            overrides_write: Mutex::new(()),
        }
    }
}

/// What the engine remembers between evaluations. Never held across an `await`; always taken
/// after (never while holding) the store and the registry.
#[derive(Default)]
pub(crate) struct ClimateRuntime {
    /// What the engine last commanded (or acknowledged as the user's) per entity.
    pub ledger: Ledger,
    /// Unix seconds of the last state report per watched entity.
    pub seen_at: HashMap<String, u64>,
    /// When an entity last came back from unavailable. Set by a real availability transition
    /// only, never at boot: a device that differs after a restart is a hand change.
    pub online_since: HashMap<String, u64>,
    /// The availability last observed per entity, to spot an unavailable -> online transition.
    pub availability: HashMap<String, Availability>,
    /// Zone -> what safety found on its devices before acting on a pending zone.
    pub safety_memory: HashMap<String, SafetyMemory>,
    /// Entity -> (consecutive failed commands, paused until).
    pub failures: HashMap<String, (u32, Option<u64>)>,
    /// Entity -> when the engine last tried to command it.
    pub last_sent: HashMap<String, u64>,
    /// Entity -> a command that was published but never echoed, and when.
    pub unconfirmed: HashMap<String, (DeviceTarget, u64)>,
    /// Entities whose command is on the wire right now; their echo is not a hand change.
    pub inflight: HashSet<String>,
    /// The last decisions per zone, newest last.
    pub decisions: HashMap<String, VecDeque<Decision>>,
    pub last_home_at: Option<u64>,
    pub previous_mode: Option<Mode>,
    /// (fetched at, mean) of the outdoor sensor's last 24 h, from history.
    pub outdoor_cache: Option<(u64, Option<f64>)>,
    /// Live outdoor readings of the last 24 h, the fallback without history.
    pub outdoor_live: VecDeque<(u64, f64)>,
    /// Alert ids currently raised.
    pub active_alerts: HashSet<String>,
    /// Safety alert id -> since when its cause has been absent; it resolves after a while.
    pub absent_since: HashMap<String, u64>,
    /// Entities whose changes matter to the engine; rebuilt each pass.
    pub watched: HashSet<String>,
    /// The ledger was seeded from the retained decisions.
    pub seeded: bool,
    /// Every zone device was classified once against the seeded ledger.
    pub swept: bool,
}
