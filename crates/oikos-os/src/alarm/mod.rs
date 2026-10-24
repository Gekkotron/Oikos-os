//! The native alarm's hub side: the retained documents, the published panel entity and the
//! settings API.

pub(crate) mod entity;
pub(crate) mod hub;

use std::sync::atomic::AtomicBool;
use std::sync::{Mutex, MutexGuard};

use oikos_alarm::doc::AlarmSettings;
use oikos_alarm::machine::Runtime;

/// The id of the alarm panel entity (its discovery `unique_id`).
pub const ALARM_ENTITY_ID: &str = "oikos_alarm_house";

/// What the hub keeps for the alarm. It exists whether or not the alarm is switched on: the
/// engine idles while the settings say disabled.
pub struct AlarmHub {
    pub(crate) prefix: String,
    pub(crate) settings: Mutex<AlarmSettings>,
    /// Why the last settings document was refused; the previous one stays in force.
    pub(crate) settings_invalid: Mutex<Option<String>>,
    pub(crate) runtime: Mutex<Runtime>,
    /// Set once the retained runtime document was read (or written by this hub).
    pub(crate) runtime_loaded: AtomicBool,
    /// Held for a whole engine pass, so two callers never drive the machine at once.
    #[allow(dead_code)] // taken by the engine loop
    pub(crate) pass: tokio::sync::Mutex<()>,
}

impl AlarmHub {
    pub(crate) fn new(prefix: &str) -> Self {
        Self {
            prefix: prefix.trim_end_matches('/').to_owned(),
            settings: Mutex::new(AlarmSettings::default()),
            settings_invalid: Mutex::new(None),
            runtime: Mutex::new(Runtime::default()),
            runtime_loaded: AtomicBool::new(false),
            pass: tokio::sync::Mutex::new(()),
        }
    }
}

/// Locks `mutex`, carrying on with the last value if a holder panicked.
pub(crate) fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}
