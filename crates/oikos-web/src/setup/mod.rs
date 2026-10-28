//! The onboarding wizard: shared state and the rules for when to send the browser to it.

use leptos::prelude::*;
use oikos_api::{SetupStatus, SetupStep};

/// What `GET /api/v1/setup` last said. `None` means the hub has no setup API (an older hub) or
/// the call failed; nothing redirects then.
#[derive(Clone, Copy)]
pub struct SetupState(pub RwSignal<Option<SetupStatus>>);

/// Whether `path` must be replaced by `/setup`: any path while the hub is in setup mode, and
/// the landing page until setup is finished.
pub fn redirects_to_setup(status: &SetupStatus, path: &str) -> bool {
    if path == "/setup" {
        return false;
    }
    status.mode == "setup" || (!status.finished && path == "/")
}

/// The step the wizard opens on. `query_step` (from `/setup?step=…`) wins when given, else a
/// finished hub starts over at `Welcome` and an unfinished one resumes at its stored step. In
/// setup mode only the steps up to `Database` work, so it never goes past that.
pub fn opening_step(status: &SetupStatus, query_step: Option<SetupStep>) -> SetupStep {
    let step = query_step.unwrap_or(if status.finished {
        SetupStep::Welcome
    } else {
        status.step
    });
    if status.mode == "setup" {
        step.min(SetupStep::Database)
    } else {
        step
    }
}

/// The `/setup?step=…` address a wizard save reloads into, so a restart resumes at that step.
/// The name comes from serde, so it is always what the hub parses.
pub fn setup_href(step: SetupStep) -> String {
    let name = serde_json::to_string(&step).unwrap_or_default();
    format!("/setup?step={}", name.trim_matches('"'))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn status(mode: &str, finished: bool, step: SetupStep) -> SetupStatus {
        SetupStatus {
            mode: mode.into(),
            step,
            finished,
            bundled: false,
            location: None,
            mqtt: None,
            database: None,
            restarting: false,
        }
    }

    #[test]
    fn setup_mode_sends_every_path_to_setup() {
        let s = status("setup", false, SetupStep::Welcome);
        assert!(redirects_to_setup(&s, "/"));
        assert!(redirects_to_setup(&s, "/dashboards/house"));
        assert!(!redirects_to_setup(&s, "/setup"));
    }

    #[test]
    fn a_running_hub_redirects_only_landing_until_finished() {
        assert!(redirects_to_setup(
            &status("running", false, SetupStep::Devices),
            "/"
        ));
        assert!(!redirects_to_setup(
            &status("running", false, SetupStep::Devices),
            "/rooms/x"
        ));
        assert!(!redirects_to_setup(
            &status("running", true, SetupStep::Done),
            "/"
        ));
    }

    #[test]
    fn opening_step_resumes_but_never_past_database_in_setup_mode() {
        assert_eq!(
            opening_step(&status("running", true, SetupStep::Done), None),
            SetupStep::Welcome
        );
        assert_eq!(
            opening_step(&status("setup", false, SetupStep::Devices), None),
            SetupStep::Database
        );
        assert_eq!(
            opening_step(&status("running", false, SetupStep::Services), None),
            SetupStep::Services
        );
        // Run setup again on a finished hub: a save restarts it, and the reload must resume, not loop.
        assert_eq!(
            opening_step(
                &status("running", true, SetupStep::Done),
                Some(SetupStep::Mqtt)
            ),
            SetupStep::Mqtt
        );
        assert_eq!(
            opening_step(
                &status("setup", false, SetupStep::Welcome),
                Some(SetupStep::Services)
            ),
            SetupStep::Database
        );
        assert_eq!(setup_href(SetupStep::Mqtt), "/setup?step=mqtt");
    }
}
