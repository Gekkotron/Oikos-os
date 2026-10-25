use leptos::prelude::*;
use oikos_core::dashboard::Widget;

use super::edit::EditState;
use super::widget::WidgetView;
use crate::i18n::{DashText, dash_text, use_lang};

/// Column grid that places widgets by their declared span. Column counts per breakpoint
/// live in `styles/main.css` (`.dashboard-grid`). No clamp is needed: the widest span is 2
/// columns and the grid never has fewer than 2, so no widget overflows on a phone.
///
/// In edit mode each tile gets resize and move buttons. Reordering uses move-left/right
/// buttons rather than HTML5 drag: drag needs DataTransfer plumbing (Firefox refuses to
/// start a drag without it) and has no touch support, which the phone layout needs.
#[component]
pub fn Grid(
    widgets: Signal<Vec<Widget>>,
    #[prop(optional)] edit: Option<EditState>,
) -> impl IntoView {
    let active = move || edit.is_some_and(|e| e.active.get());
    let lang = use_lang();
    // The tile of a widget just added is brought into view once it has rendered.
    #[cfg(target_arch = "wasm32")]
    if let Some(state) = edit {
        Effect::new(move |_| {
            if let Some(id) = state.fresh.get() {
                request_animation_frame(move || {
                    let selector = format!("[data-widget-id=\"{id}\"]");
                    if let Ok(Some(tile)) = document().query_selector(&selector) {
                        let options = web_sys::ScrollIntoViewOptions::new();
                        options.set_block(web_sys::ScrollLogicalPosition::Nearest);
                        options.set_behavior(web_sys::ScrollBehavior::Smooth);
                        tile.scroll_into_view_with_scroll_into_view_options(&options);
                    }
                });
            }
        });
    }
    view! {
        <div class="dashboard-grid">
            <For
                each=move || widgets.get()
                key=|w| w.id.clone()
                children=move |w: Widget| {
                    // Keyed by id, so the tile survives a span change or reorder; the class and
                    // index must therefore be looked up reactively.
                    let id = w.id.clone();
                    let position = {
                        let id = id.clone();
                        // Callback is Copy, so the three buttons can each capture it.
                        Callback::new(move |()| {
                            widgets.with(|ws| ws.iter().position(|x| x.id == id))
                        })
                    };
                    let class = {
                        let id = id.clone();
                        move || {
                            let span = widgets
                                .with(|ws| ws.iter().find(|x| x.id == id).map(|x| x.span))
                                .unwrap_or(w.span);
                            format!(concat!("widget", " widget-span-{}"), span.as_str())
                        }
                    };
                    let is_fresh = {
                        let id = id.clone();
                        move || edit.is_some_and(|e| e.fresh.with(|f| f.as_deref() == Some(id.as_str())))
                    };
                    let tools = edit.map(|state| {
                        view! {
                            <Show when=active>
                                // Stop bubbling so the tile's open-config click does not fire
                                // for the move/resize buttons.
                                <div class="widget__tools" on:click=|ev| ev.stop_propagation()>
                                    <button
                                        type="button"
                                        class="widget__move"
                                        aria-label=move || dash_text(lang(), DashText::MoveEarlier)
                                        on:click=move |_| {
                                            if let Some(i) = position.run(())
                                                && i > 0
                                            {
                                                state.reorder(i, i - 1);
                                            }
                                        }
                                    >
                                        "◀"
                                    </button>
                                    <button
                                        type="button"
                                        class="widget__move"
                                        aria-label=move || dash_text(lang(), DashText::MoveLater)
                                        on:click=move |_| {
                                            if let Some(i) = position.run(()) {
                                                state.reorder(i, i + 1);
                                            }
                                        }
                                    >
                                        "▶"
                                    </button>
                                    <button
                                        type="button"
                                        class="widget__resize"
                                        aria-label=move || dash_text(lang(), DashText::Resize)
                                        on:click=move |_| {
                                            if let Some(i) = position.run(()) {
                                                state.cycle_span(i);
                                            }
                                        }
                                    >
                                        "⤢"
                                    </button>
                                </div>
                            </Show>
                        }
                    });
                    view! {
                        <div class=class class:widget--fresh=is_fresh data-widget-id=id>
                            <WidgetView widget=w />
                            // An overlay (not a tile click handler) so the inner card's own
                            // controls, e.g. a light toggle, cannot fire while editing. It sits
                            // above the card and below `.widget__tools` (z-index 2).
                            {edit.map(|state| view! {
                                <Show when=active>
                                    <div
                                        class="widget__edit-overlay"
                                        on:click=move |_| {
                                            if let Some(i) = position.run(()) {
                                                state.selected_widget.set(Some(i));
                                            }
                                        }
                                    />
                                </Show>
                            })}
                            {tools}
                        </div>
                    }
                }
            />
        </div>
    }
}
