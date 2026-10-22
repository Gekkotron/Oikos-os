//! The user-defined entity groups: a top-level screen so Home-level groups are reachable
//! without a Devices tab in between.

use leptos::prelude::*;
use leptos_router::components::A;
use oikos_api::GroupView;

use crate::components::toast::Toasts;
use crate::format::wrap_id;
use crate::icons::PLUS;

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
                <div class="chip-row">
                    <A href="/groups/new" attr:class="chip chip--add">
                        <span class="chip__glyph" inner_html=PLUS></span>
                        "New group"
                    </A>
                    <For
                        each=move || groups.get()
                        key=|g| (g.id.clone(), g.name.clone(), g.members.len())
                        children=|g| {
                            view! {
                                <A
                                    href=format!("/group/{}", crate::api::encode_id(&g.id))
                                    attr:class="chip"
                                >
                                    {wrap_id(&g.name)}
                                </A>
                            }
                        }
                    />
                </div>
            </section>
        </div>
    }
}
