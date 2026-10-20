//! Group cards: a room's covers or lights as one wide card on Home.

use leptos::prelude::*;
use oikos_api::EntityView;
use oikos_core::model::{Command, GroupKey, GroupKind, Kind};

use super::{PowerToggle, Sender};
use crate::groups::{
    GroupSummary, cover_text, group_name, group_path, light_text, light_toggle, members, summary,
    targets,
};
use crate::house::House;
use crate::icons::{CHEVRON_DOWN, CHEVRON_UP, STOP, kind_icon};

/// What a group card reads from the house, each part behind its own memo: `members` and
/// `summary` only notify when they change, so a state echo repaints only the card whose group
/// it touches.
#[derive(Clone, Copy)]
pub(crate) struct GroupView {
    pub(crate) members: Memo<Vec<EntityView>>,
    pub(crate) summary: Memo<GroupSummary>,
    pub(crate) name: Memo<String>,
    pub(crate) area_name: Memo<String>,
    pub(crate) pending: Signal<bool>,
}

impl GroupView {
    /// A group card's view: the `ids` it folds, which under Favorites can be a part of the room's
    /// group.
    pub(crate) fn of_ids(key: &GroupKey, ids: Vec<String>, sender: Sender) -> Self {
        let house = expect_context::<House>();
        let members = Memo::new(move |_| {
            house.entities.with(|m| {
                ids.iter()
                    .filter_map(|id| m.get(id).cloned())
                    .collect::<Vec<_>>()
            })
        });
        Self::new(key, members, sender)
    }

    /// The whole room group, as its sheet shows it.
    pub(crate) fn of_group(key: &GroupKey, sender: Sender) -> Self {
        let house = expect_context::<House>();
        let members = {
            let key = key.clone();
            Memo::new(move |_| house.entities.with(|m| members(m, &key)))
        };
        Self::new(key, members, sender)
    }

    fn new(key: &GroupKey, members: Memo<Vec<EntityView>>, sender: Sender) -> Self {
        let house = expect_context::<House>();
        let summary =
            Memo::new(move |_| members.with(|ms| house.entities.with(|m| summary(ms, m))));
        let name = {
            let key = key.clone();
            Memo::new(move |_| house.areas.with(|a| group_name(a, &key)))
        };
        let area_name = {
            let area = key.area.clone();
            Memo::new(move |_| {
                house.areas.with(|a| {
                    a.iter()
                        .find(|x| x.id == area)
                        .map_or_else(|| area.clone(), |x| x.name.clone())
                })
            })
        };
        let pending = Signal::derive(move || {
            members.with(|ms| sender.any_pending(ms.iter().map(|e| e.id.as_str())))
        });
        Self {
            members,
            summary,
            name,
            area_name,
            pending,
        }
    }

    /// Sends `cmd` to the members that can take it.
    pub(crate) fn send(&self, sender: Sender, cmd: Command) {
        let ids = self.members.with_untracked(|ms| targets(ms, &cmd));
        sender.send_group(&ids, cmd);
    }
}

/// The shell of a group card: icon, name, `×N` badge and status dot in a header that opens the
/// group sheet, the body below.
#[component]
fn GroupFrame(
    kind: Kind,
    key: GroupKey,
    view: GroupView,
    #[prop(into)] label: Signal<String>,
    children: Children,
) -> impl IntoView {
    let navigate = leptos_router::hooks::use_navigate();
    let path = group_path(&key);
    let icon = kind_icon(kind, None);
    let GroupView {
        members,
        summary,
        name,
        pending,
        ..
    } = view;
    view! {
        <article
            class="card card--wide"
            class:card--on=move || summary.with(|s| s.on > 0)
            class:card--offline=move || summary.with(|s| !s.available)
            class:card--pending=move || pending.get()
            aria-label=move || label.get()
        >
            <button
                type="button"
                class="card__header"
                on:click=move |_| navigate(&path, Default::default())
            >
                <span class="card__icon" inner_html=icon></span>
                <span class="card__name">{move || name.get()}</span>
                <span class="group__count">{move || format!("×{}", members.with(Vec::len))}</span>
                <span class="card__dots">
                    <span class="dot dot--pending" class:dot--active=move || pending.get()></span>
                </span>
            </button>
            <div class="card__body">{children()}</div>
        </article>
    }
}

/// The accessible name of a group card, "Shutters, Salon, 2 open, 1 closed".
fn card_label(view: GroupView, state: fn(&GroupSummary) -> String) -> Signal<String> {
    Signal::derive(move || {
        format!(
            "{}, {}, {}",
            view.name.get(),
            view.area_name.get(),
            view.summary.with(state)
        )
    })
}

/// ▲ ■ ▼ as a row of buttons sending Open, Stop and Close to `on_command`. `suffix` ends their
/// accessible names ("Open" + " all shutters").
#[component]
pub fn CoverButtons(
    #[prop(into)] suffix: String,
    on_command: Callback<Command>,
    /// Greys the buttons out (a group with no member online).
    #[prop(optional, into)]
    disabled: Signal<bool>,
) -> impl IntoView {
    let button = move |text: &'static str, icon: &'static str, cmd: Command| {
        let label = format!("{text}{suffix}");
        view! {
            <button
                type="button"
                class="btn btn--icon"
                aria-label=label.clone()
                title=label
                disabled=move || disabled.get()
                inner_html=icon
                on:click=move |_| on_command.run(cmd.clone())
            ></button>
        }
    };
    view! {
        <div class="card__controls">
            {button("Open", CHEVRON_UP, Command::Open)}
            {button("Stop", STOP, Command::Stop)}
            {button("Close", CHEVRON_DOWN, Command::Close)}
        </div>
    }
}

/// A room's shutters as one card: how many are open, ▲ ■ ▼ for all of them, a bar for each.
#[component]
pub fn CoverGroupCard(key: GroupKey, members: Vec<String>) -> impl IntoView {
    debug_assert_eq!(key.kind, GroupKind::Cover);
    let sender = Sender::from_context();
    let view = GroupView::of_ids(&key, members, sender);
    let summary = view.summary;
    let label = card_label(view, |s| {
        format!("{} open, {} closed", s.on, s.total - s.on)
    });

    let bars = move || {
        summary.with(|s| {
            s.positions
                .iter()
                .map(|p| {
                    view! {
                        <span class="group__bar">
                            <span class="group__fill" style:height=format!("{p}%")></span>
                        </span>
                    }
                })
                .collect_view()
        })
    };
    view! {
        <GroupFrame kind=Kind::Cover key=key view=view label=label>
            <div class="group__row">
                <div class="group__status">
                    <span class="card__value">{move || summary.with(cover_text)}</span>
                    <div class="group__bars">{bars}</div>
                </div>
                <CoverButtons
                    suffix=" all shutters"
                    on_command=Callback::new(move |cmd| view.send(sender, cmd))
                    disabled=Signal::derive(move || summary.with(|s| !s.available))
                />
            </div>
        </GroupFrame>
    }
}

/// A room's lights as one card: how many are on and a switch for all of them.
#[component]
pub fn LightGroupCard(key: GroupKey, members: Vec<String>) -> impl IntoView {
    debug_assert_eq!(key.kind, GroupKind::Light);
    let sender = Sender::from_context();
    let view = GroupView::of_ids(&key, members, sender);
    let summary = view.summary;
    let label = card_label(view, |s| format!("{} of {} on", s.on, s.total));

    let toggle = Callback::new(move |()| {
        view.send(sender, summary.with_untracked(light_toggle));
    });
    view! {
        <GroupFrame kind=Kind::Light key=key view=view label=label>
            {move || {
                let (on, text, available) = summary
                    .with(|s| (s.on > 0, light_text(s), s.available));
                view! {
                    <PowerToggle
                        on=on
                        label=text
                        on_toggle=toggle
                        aria_label="All lights"
                        disabled=!available
                    />
                }
            }}
        </GroupFrame>
    }
}
