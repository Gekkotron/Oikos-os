//! The Home room heading, renamed in place: the name is saved to the hub's areas document.

use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::api::rename_area;
use crate::components::toast::Toasts;
use crate::house::House;
use crate::i18n::current_lang;
use crate::i18n::devices::{DeviceText, device_text, use_device_text};
use crate::icons;

/// The room heading. `id` is `None` for a chip that is not a room (Favorites, Unassigned), which
/// gets no rename button.
#[component]
pub fn RoomTitle(
    #[prop(into)] id: Signal<Option<String>>,
    #[prop(into)] name: Signal<String>,
) -> impl IntoView {
    let house = expect_context::<House>();
    let toasts = expect_context::<Toasts>();
    let t = use_device_text();
    let editing = RwSignal::new(false);
    let busy = RwSignal::new(false);
    let draft = RwSignal::new(String::new());
    let input = NodeRef::<leptos::html::Input>::new();

    // Picking another chip drops an unsaved rename.
    Effect::new(move |_| {
        id.track();
        editing.set(false);
    });
    Effect::new(move |_| {
        if editing.get()
            && let Some(input) = input.get()
        {
            let _ = input.focus();
            input.select();
        }
    });

    let start = move |_| {
        draft.set(name.get_untracked());
        editing.set(true);
    };
    let save = move || {
        let Some(id) = id.get_untracked() else {
            return;
        };
        let next = draft.get_untracked().trim().to_owned();
        if next.is_empty() {
            toasts.push(device_text(current_lang(), DeviceText::RoomNeedsName));
            return;
        }
        if next == name.get_untracked() {
            editing.set(false);
            return;
        }
        busy.set(true);
        spawn_local(async move {
            match rename_area(&id, &next).await {
                Ok(areas) => {
                    house.areas.set(areas);
                    editing.set(false);
                }
                Err(e) => toasts.push(e.error),
            }
            busy.set(false);
        });
    };

    view! {
        <Show
            when=move || editing.get()
            fallback=move || {
                view! {
                    <div class="room__heading">
                        <h1 class="room__title">{move || name.get()}</h1>
                        <Show when=move || id.with(Option::is_some)>
                            <button
                                type="button"
                                class="btn btn--round btn--icon room__rename"
                                aria-label=move || t(DeviceText::RenameRoom)
                                title=move || t(DeviceText::RenameRoom)
                                on:click=start
                                inner_html=icons::PENCIL
                            ></button>
                        </Show>
                    </div>
                }
            }
        >
            <form
                class="room__heading room__form"
                on:submit=move |ev| {
                    ev.prevent_default();
                    save();
                }
            >
                <input
                    node_ref=input
                    class="field room__field"
                    type="text"
                    aria-label=move || t(DeviceText::RoomName)
                    prop:value=move || draft.get()
                    prop:disabled=move || busy.get()
                    on:input=move |ev| draft.set(event_target_value(&ev))
                    on:keydown=move |ev| {
                        if ev.key() == "Escape" {
                            editing.set(false);
                        }
                    }
                />
                <button type="submit" class="btn room__action" disabled=move || busy.get()>
                    {move || t(DeviceText::Save)}
                </button>
                <button type="button" class="btn room__action" on:click=move |_| editing.set(false)>
                    {move || t(DeviceText::Cancel)}
                </button>
            </form>
        </Show>
    }
}
