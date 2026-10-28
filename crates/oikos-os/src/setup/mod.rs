//! Onboarding: the bootstrap file and (in later tasks) the settings the wizard edits.

pub mod fs;
pub mod hub_file;
pub mod import;
pub mod sections;
pub mod store;

pub use hub_file::{HUB_FILE, HubFile, HubFileError};
