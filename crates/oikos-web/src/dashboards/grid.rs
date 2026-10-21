use leptos::prelude::*;
use oikos_core::dashboard::Widget;

use super::edit::EditState;
use super::widget::WidgetView;

/// Column grid that places widgets by their declared span. Column counts per breakpoint
/// live in `styles/main.css` (`.dashboard-grid`); a span wider than the column count is
/// clamped there so no widget overflows on a phone.
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
                            format!("widget widget-span-{}", span.as_str())
                        }
                    };
                    let tools = edit.map(|state| {
                        view! {
                            <Show when=active>
                                <div class="widget__tools">
                                    <button
                                        type="button"
                                        class="widget__move"
                                        aria-label="Move earlier"
                                        on:click=move |_| {
                                            if let Some(i) = position.run(()) {
                                                if i > 0 {
                                                    state.reorder(i, i - 1);
                                                }
                                            }
                                        }
                                    >
                                        "◀"
                                    </button>
                                    <button
                                        type="button"
                                        class="widget__move"
                                        aria-label="Move later"
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
                                        aria-label="Resize"
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
                        <div class=class>
                            <WidgetView widget=w />
                            {tools}
                        </div>
                    }
                }
            />
        </div>
    }
}
