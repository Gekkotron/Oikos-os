//! The Climate screen's room list: one button per zone with its reading, a dot in the colour of
//! its decided temperature and a line saying where it stands. A column beside the room panel on
//! wide screens, a row of chips on narrow ones (the CSS decides).

use leptos::prelude::*;
use oikos_climate::view::ZoneView;

use crate::climate::{TempBand, degrees_text, room_band, room_subline};

#[component]
pub fn RoomList(
    zones: Signal<Vec<ZoneView>>,
    /// The zone id the panel shows.
    selected: Signal<Option<String>>,
    on_select: Callback<String>,
) -> impl IntoView {
    let ids =
        Memo::new(move |_| zones.with(|z| z.iter().map(|z| z.zone.clone()).collect::<Vec<_>>()));
    view! {
        <nav class="climate-rooms" aria-label="Rooms">
            <For each=move || ids.get() key=|id| id.clone() let:id>
                {
                    let row = {
                        let id = id.clone();
                        Memo::new(move |_| zones.with(|z| z.iter().find(|z| z.zone == id).cloned()))
                    };
                    let field = move |f: fn(&ZoneView) -> String| {
                        move || row.with(|z| z.as_ref().map(f).unwrap_or_default())
                    };
                    let band = move || row.with(|z| z.as_ref().and_then(room_band));
                    let is_selected = {
                        let id = id.clone();
                        move || selected.with(|s| s.as_deref() == Some(id.as_str()))
                    };
                    let current = is_selected.clone();
                    view! {
                        <button
                            type="button"
                            class="climate-room"
                            class:climate-room--selected=is_selected
                            aria-current=move || current().then_some("true")
                            on:click=move |_| on_select.run(id.clone())
                        >
                            <span class="climate-room__name">
                                <i
                                    class="climate-room__dot"
                                    class:climate-temp--low=move || band() == Some(TempBand::Low)
                                    class:climate-temp--mid=move || band() == Some(TempBand::Mid)
                                    class:climate-temp--high=move || band() == Some(TempBand::High)
                                    class:climate-temp--none=move || band().is_none()
                                ></i>
                                {field(|z| z.name.clone())}
                            </span>
                            <span class="climate-room__temp">
                                {field(|z| z.reading.map_or_else(|| "—".to_owned(), degrees_text))}
                            </span>
                            <span class="climate-room__sub">{field(room_subline)}</span>
                        </button>
                    }
                }
            </For>
        </nav>
    }
}
