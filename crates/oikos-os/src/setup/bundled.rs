//! `bundled.json`: the credentials of the broker and database the compose file ships.

use std::fmt;
use std::path::Path;

use serde::Deserialize;

pub const BUNDLED_FILE: &str = "bundled.json";

#[derive(Clone, Deserialize)]
pub struct Bundled {
    pub mqtt_user: String,
    pub mqtt_password: String,
    pub db_user: String,
    pub db_password: String,
    pub db_name: String,
}

impl fmt::Debug for Bundled {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Bundled")
            .field("mqtt_user", &self.mqtt_user)
            .field("mqtt_password", &"***")
            .field("db_user", &self.db_user)
            .field("db_password", &"***")
            .field("db_name", &self.db_name)
            .finish()
    }
}

/// `None` when the file is missing or unreadable: the wizard then pre-fills nothing.
pub fn load(dir: &Path) -> Option<Bundled> {
    let text = std::fs::read_to_string(dir.join(BUNDLED_FILE)).ok()?;
    serde_json::from_str(&text).ok()
}
