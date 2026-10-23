//! The left sidebar: brand, the main sections and dashboards, the rooms, the house screens, then
//! Settings and the connection status.

use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::hooks::{use_location, use_navigate};

use super::route::{Section, section_for};
use crate::app::landing_dashboard;
use crate::components::status_pill::StatusPill;
use crate::dashboards::create_dashboard;
use crate::house::House;
use crate::i18n::{Key, Lang, unseen_alerts, use_text};
use crate::icons::{BELL, BOLT, FLOW_NODES, GEAR, HOUSE, LAYERS, PLUS, SHIELD, THERMOMETER};
use crate::nav::BackStack;
use crate::rooms::{sidebar_rooms, use_room_chips};

#[component]
pub fn Sidebar(
    climate_on: Signal<bool>,
    alarm_armed: Signal<bool>,
    solar_active: Signal<bool>,
    alerts_badge: Signal<Option<String>>,
) -> impl IntoView {
    let t = use_text();
    let lang = use_context::<RwSignal<Lang>>();
    let house = expect_context::<House>();
    let back = expect_context::<BackStack>().0;
    let location = use_location();
    let navigate = use_navigate();

    let section = Memo::new(move |_| {
        let path = location.pathname.get();
        back.with(|stack| section_for(&path, stack))
    });
    let current = move |entry: Section| move || (section.get() == entry).then_some("page");

    // The rooms that have something on Home; Favorites and Unassigned are not rooms of their own.
    let chips = use_room_chips();
    let rooms = Memo::new(move |_| chips.with(|list| sidebar_rooms(list)));

    // With two or more dashboards (the house counts) they unfold under Overview.
    let unfolded = Memo::new(move |_| house.dashboards.with(|list| list.len() >= 2));
    // The id in a `/dashboards/:id` path.
    let open_dashboard = Memo::new(move |_| {
        location
            .pathname
            .get()
            .strip_prefix("/dashboards/")
            .map(|rest| rest.split('/').next().unwrap_or("").to_owned())
    });
    let overview_href = move || {
        house
            .dashboards
            .with(|list| landing_dashboard(list))
            .map_or_else(|| "/".to_owned(), |id| format!("/dashboards/{id}"))
    };
    // When the open dashboard is listed underneath, it is the page and Overview only its section.
    let overview_current = move || {
        (section.get() == Section::Overview).then(|| {
            let listed = unfolded.get()
                && open_dashboard.with(|open| {
                    open.as_ref().is_some_and(|id| {
                        house
                            .dashboards
                            .with(|list| list.iter().any(|d| &d.dashboard.id == id))
                    })
                });
            if listed { "true" } else { "page" }
        })
    };
    // The house first, then the others in the hub's order.
    let ordered = move || {
        let mut list = house.dashboards.get();
        list.sort_by_key(|d| d.dashboard.id != oikos_api::HOUSE_ID);
        list
    };
    let new_dashboard = move |_| {
        let navigate = navigate.clone();
        spawn_local(async move {
            if let Some(id) = create_dashboard(house).await {
                navigate(&format!("/dashboards/{id}?edit=1"), Default::default());
            }
        });
    };

    // Easter egg: double-clicking the logo rings the doorbell and gives the house a little shake.
    let ringing = RwSignal::new(false);
    let ring = move |_| {
        crate::chime::ring();
        ringing.set(true);
        gloo_timers::callback::Timeout::new(1200, move || {
            ringing.try_set(false);
        })
        .forget();
    };

    view! {
        <aside class="sidebar" id="sidebar">
            <div class="sidebar__brand">
                <img
                    class="sidebar__logo"
                    class:sidebar__logo--ringing=move || ringing.get()
                    src="/assets/icon.svg"
                    alt=""
                    width="28"
                    height="28"
                    draggable="false"
                    on:dblclick=ring
                />
                <span class="sidebar__wordmark">"oikos."</span>
            </div>
            <nav class="sidebar__nav" aria-label=move || t(Key::Navigation)>
                <ul class="sidebar__list">
                    <li>
                        <a class="sidebar__link" href=overview_href aria-current=overview_current>
                            <span class="nav__glyph" inner_html=HOUSE></span>
                            <span class="nav__label">{move || t(Key::Overview)}</span>
                        </a>
                        <Show when=move || unfolded.get()>
                            <ul class="sidebar__dashboards">
                                <For
                                    each=ordered
                                    key=|dv| (dv.dashboard.id.clone(), dv.dashboard.name.clone())
                                    children=move |dv| {
                                        let id = dv.dashboard.id;
                                        let href = format!("/dashboards/{id}");
                                        let is_open = move || {
                                            open_dashboard
                                                .with(|open| open.as_deref() == Some(id.as_str()))
                                                .then_some("page")
                                        };
                                        view! {
                                            <li>
                                                <a class="sidebar__sublink" href=href aria-current=is_open>
                                                    {dv.dashboard.name}
                                                </a>
                                            </li>
                                        }
                                    }
                                />
                                <li>
                                    <button
                                        type="button"
                                        class="sidebar__sublink sidebar__new"
                                        on:click=new_dashboard.clone()
                                    >
                                        <span class="sidebar__new-glyph" inner_html=PLUS></span>
                                        {move || t(Key::NewDashboard)}
                                    </button>
                                </li>
                            </ul>
                        </Show>
                    </li>
                    <li>
                        <a class="sidebar__link" href="/flows" aria-current=current(Section::Automations)>
                            <span class="nav__glyph" inner_html=FLOW_NODES></span>
                            <span class="nav__label">{move || t(Key::Automations)}</span>
                        </a>
                    </li>
                    <li>
                        <a class="sidebar__link" href="/groups" aria-current=current(Section::Groups)>
                            <span class="nav__glyph" inner_html=LAYERS></span>
                            <span class="nav__label">{move || t(Key::Groups)}</span>
                        </a>
                    </li>
                </ul>
                <h2 class="sidebar__heading">{move || t(Key::MyRooms)}</h2>
                <ul class="sidebar__rooms">
                    <For
                        each=move || rooms.get()
                        key=|r| (r.id.clone(), r.name.clone(), r.primary_count)
                        children=move |room| {
                            let entry = Section::Room(room.id.clone());
                            view! {
                                <li>
                                    <a
                                        class="sidebar__link sidebar__room"
                                        href=format!("/rooms/{}", room.id)
                                        aria-current=current(entry)
                                    >
                                        <span class="sidebar__room-name">{room.name}</span>
                                        <span class="sidebar__count">{room.primary_count}</span>
                                    </a>
                                </li>
                            }
                        }
                    />
                </ul>
                <h2 class="sidebar__heading">{move || t(Key::HomeSection)}</h2>
                <ul class="sidebar__list">
                    <li>
                        <a class="sidebar__link" href="/energy" aria-current=current(Section::Energy)>
                            <span
                                class="nav__glyph"
                                class:nav__glyph--solar=move || solar_active.get()
                                inner_html=BOLT
                            ></span>
                            <span class="nav__label">{move || t(Key::Energy)}</span>
                        </a>
                    </li>
                    <li>
                        <a class="sidebar__link" href="/security" aria-current=current(Section::Security)>
                            <span
                                class="nav__glyph"
                                class:nav__glyph--armed=move || alarm_armed.get()
                                inner_html=SHIELD
                            ></span>
                            <span class="nav__label">{move || t(Key::Security)}</span>
                        </a>
                    </li>
                    <Show when=move || climate_on.get()>
                        <li>
                            <a class="sidebar__link" href="/climate" aria-current=current(Section::Climate)>
                                <span class="nav__glyph" inner_html=THERMOMETER></span>
                                <span class="nav__label">{move || t(Key::Climate)}</span>
                                <span class="nav__badge nav__badge--beta">{move || t(Key::Beta)}</span>
                            </a>
                        </li>
                    </Show>
                    <li>
                        <a class="sidebar__link" href="/alerts" aria-current=current(Section::Alerts)>
                            <span class="nav__glyph" inner_html=BELL></span>
                            <span class="nav__label">{move || t(Key::Alerts)}</span>
                            {move || alerts_badge.get().map(|text| {
                                let label = unseen_alerts(lang.map_or(Lang::En, |l| l.get()), &text);
                                view! {
                                    <span class="nav__badge" aria-label=label>
                                        {text}
                                    </span>
                                }
                            })}
                        </a>
                    </li>
                </ul>
                <ul class="sidebar__list sidebar__footer">
                    <li>
                        <a class="sidebar__link" href="/settings" aria-current=current(Section::Settings)>
                            <span class="nav__glyph" inner_html=GEAR></span>
                            <span class="nav__label">{move || t(Key::Settings)}</span>
                        </a>
                    </li>
                </ul>
            </nav>
            <div class="sidebar__status">
                <StatusPill />
            </div>
        </aside>
    }
}
