//! Onboarding: the bootstrap file, the settings the wizard edits, and how the hub boots from them.

pub mod boot;
pub mod fs;
pub mod hub_file;
pub mod import;
pub mod restart;
pub mod sections;
pub mod store;

use std::path::PathBuf;
use std::sync::atomic::AtomicBool;

use tokio::sync::Mutex;

use crate::history::db::Db;
pub use hub_file::{HUB_FILE, HubFile, HubFileError};
pub use restart::RestartHook;
use sections::Sections;

/// Which half of the hub is running.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Not configured yet: only the setup screens are served.
    Setup,
    /// The whole hub.
    Run,
}

/// What the setup code shares: the files, the database and the restart switch.
pub struct SetupCtx {
    pub data_dir: PathBuf,
    pub mode: Mode,
    /// Every write of `hub.json` goes through this lock (see [`SetupCtx::update_file`]).
    pub file: Mutex<HubFile>,
    pub db: Option<Db>,
    pub sections: Mutex<Sections>,
    pub restarting: AtomicBool,
    pub restart: RestartHook,
}

impl SetupCtx {
    pub fn new(
        data_dir: PathBuf,
        mode: Mode,
        file: HubFile,
        db: Option<Db>,
        sections: Sections,
        restart: RestartHook,
    ) -> SetupCtx {
        SetupCtx {
            data_dir,
            mode,
            file: Mutex::new(file),
            db,
            sections: Mutex::new(sections),
            restarting: AtomicBool::new(false),
            restart,
        }
    }

    /// Edits `hub.json` under the lock: changes a copy, saves the copy, and only then replaces
    /// the held value, so a failed save leaves memory and disk alike, and two writers never
    /// meet on `hub.json.tmp`.
    pub async fn update_file(&self, edit: impl FnOnce(&mut HubFile)) -> std::io::Result<()> {
        let mut held = self.file.lock().await;
        let mut next = held.clone();
        edit(&mut next);
        next.save(&self.data_dir)?;
        *held = next;
        Ok(())
    }
}
