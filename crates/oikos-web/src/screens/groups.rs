//! The user-defined entity groups: a top-level screen so Home-level groups are reachable
//! without a Devices tab in between.

use leptos::prelude::*;
use leptos_router::components::A;
use oikos_api::GroupView;

use crate::components::toast::Toasts;
use crate::format::wrap_id;
use crate::icons::{PLUS, kind_icon};

#[component]
pub fn GroupsScreen() -> impl IntoView {
    let toasts = expect_context::<Toasts>();
    let groups = RwSignal::new(Vec::<GroupView>::new());
    leptos::task::spawn_local(async move {
        match crate::api::list_groups().await {
            Ok(list) => groups.set(list),
            Err(e) => toasts.push(e.error),
        }
    });

    view! {
        <div class="devices">
            <h1 class="devices__title">"Groups"</h1>
            <section class="sheet__panel groups-card">
                <div class="groups-card__head">
                    <h2 class="sheet__heading">"Groups"</h2>
                    <span class="groups-card__count">
                        {move || groups.with(Vec::len)} " groups"
                    </span>
                </div>
                <Show
                    when=move || groups.with(|g| !g.is_empty())
                    fallback=|| view! {
                        <p class="groups-card__empty">
                            "No groups yet. Create one to control several entities together."
                        </p>
                    }
                >
                    <ul class="group-list">
                        <For
                            each=move || groups.get()
                            key=|g| (g.id.clone(), g.name.clone(), g.members.len())
                            children=|g| {
                                let href = format!("/group/{}", crate::api::encode_id(&g.id));
                                let glyph = kind_icon(g.kind, None);
                                let count = g.members.len();
                                let count_label = if count == 1 {
                                    "1 member".to_owned()
                                } else {
                                    format!("{count} members")
                                };
                                view! {
                                    <li>
                                        <A href=href attr:class="group-list__row">
                                            <span class="group-list__glyph" inner_html=glyph></span>
                                            <span class="group-list__name">{wrap_id(&g.name)}</span>
                                            <span class="group-list__count">{count_label}</span>
                                        </A>
                                    </li>
                                }
                            }
                        />
                    </ul>
                </Show>
                <div class="groups-card__actions">
                    <A href="/groups/new" attr:class="chip chip--add">
                        <span class="chip__glyph" inner_html=PLUS></span>
                        "New group"
                    </A>
                </div>
            </section>
        </div>
    }
}
