//! Navigation state that outlives a screen: the Home room and where an entity sheet's Back goes.

use leptos::prelude::*;
use leptos_router::hooks::use_location;

/// The Home chip the user picked. Provided above the router, so opening a sheet and coming back
/// lands on the same room instead of the first one.
#[derive(Clone, Copy)]
pub struct HomeRoom(pub RwSignal<Option<String>>);

/// The last page outside the entity sheets, path and query. A sheet's Back returns there, so a
/// sheet opened from Energy, Security or Devices goes back to that screen, and hopping between
/// sibling sheets still leads out to the page the first one was opened from.
#[derive(Clone, Copy)]
pub struct ReturnPath(pub RwSignal<String>);

/// Provides [`HomeRoom`] and [`ReturnPath`]. Call it once, above the router.
pub fn install_nav() {
    provide_context(HomeRoom(RwSignal::new(None)));
    provide_context(ReturnPath(RwSignal::new("/".to_owned())));
}

/// The page a sheet's Back should return to when the app is at `path` with query `search`
/// (without its `?`), or `None` for an entity sheet, which is never a return target.
fn return_target(path: &str, search: &str) -> Option<String> {
    if path.starts_with("/entity/") || path.starts_with("/history/") {
        None
    } else if search.is_empty() {
        Some(path.to_owned())
    } else {
        Some(format!("{path}?{search}"))
    }
}

/// The Home room to select when a group sheet in `area` goes Back; `None` for a group with no room.
pub fn home_room_for(area: &str) -> Option<String> {
    (!area.is_empty()).then(|| area.to_owned())
}

/// Keeps [`ReturnPath`] on the current page. Render it inside the router.
#[component]
pub fn TrackReturnPath() -> impl IntoView {
    let location = use_location();
    let return_path = expect_context::<ReturnPath>();
    Effect::new(move |_| {
        if let Some(target) = return_target(&location.pathname.get(), &location.search.get()) {
            return_path.0.set(target);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::{home_room_for, return_target};

    #[test]
    fn group_back_selects_its_room() {
        assert_eq!(home_room_for("living").as_deref(), Some("living"));
        assert_eq!(home_room_for(""), None);
    }

    #[test]
    fn screens_are_return_targets() {
        assert_eq!(return_target("/", "").as_deref(), Some("/"));
        assert_eq!(return_target("/energy", "").as_deref(), Some("/energy"));
        assert_eq!(
            return_target("/devices", "q=lamp").as_deref(),
            Some("/devices?q=lamp")
        );
    }

    #[test]
    fn group_sheet_is_a_return_target() {
        assert_eq!(
            return_target("/group/salon/light", "").as_deref(),
            Some("/group/salon/light")
        );
    }

    #[test]
    fn history_detail_is_not_a_return_target() {
        // When the user is on /history/..., ReturnPath keeps pointing at the pre-history page
        // so the Back arrow actually leaves instead of self-looping (reviewer finding #1).
        assert_eq!(
            return_target("/history/sensor.living_temp", "range=24h"),
            None
        );
    }

    #[test]
    fn sheets_are_not() {
        assert_eq!(return_target("/entity/lamp", ""), None);
        assert_eq!(return_target("/entity/a%2Fb", "x=1"), None);
    }
}
