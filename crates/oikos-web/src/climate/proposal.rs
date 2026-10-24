//! The proposal panel and the approved-rules panel of a zone card.
//!
//! Both show the rules on a week timeline. A proposal is reviewed as learned, or edited first
//! and approved as edited; a direct edit of the approved rules saves through the zone settings
//! and counts as approval (spec §4.4). Edits are local until Approve or Save, and live in
//! signals the card owns, so a refetch of the zone leaves them alone.

use leptos::prelude::*;
use leptos::task::spawn_local;
use oikos_api::ApiError;
use oikos_climate::doc::{Day, Mode, ModeRules, Proposal, ProposalKind, Rules, Setpoint};
use oikos_climate::learn::diff;
use oikos_climate::view::ZoneView;

use super::timeline::{NEW_BASE, WeekTimeline};
use crate::api::climate as api;
use crate::components::toast::Toasts;

fn present(rules: &Rules) -> Vec<Mode> {
    [Mode::Heat, Mode::Cool]
        .into_iter()
        .filter(|m| rules.get(*m).is_some())
        .collect()
}

/// `rules` with the schedule of `mode` replaced.
fn with_mode(rules: &Rules, mode: Mode, next: ModeRules) -> Rules {
    let mut out = rules.clone();
    match mode {
        Mode::Heat => out.heat = Some(next),
        Mode::Cool => out.cool = Some(next),
    }
    out
}

/// A timeline of `rules`, one tab per mode when both exist.
#[component]
fn RulesTimeline(
    rules: Signal<Rules>,
    editable: Signal<bool>,
    on_change: Callback<Rules>,
    now: Signal<(Day, u16)>,
) -> impl IntoView {
    let modes = Memo::new(move |_| rules.with(present));
    let mode = RwSignal::new(modes.with_untracked(|m| m.first().copied().unwrap_or(Mode::Heat)));
    let tabs = move || {
        let list = modes.get();
        (list.len() > 1).then(|| {
            let buttons = list
                .into_iter()
                .map(|m| {
                    view! {
                        <button
                            type="button"
                            class="chip segmented__option"
                            class:chip--selected=move || mode.get() == m
                            aria-pressed=move || (mode.get() == m).to_string()
                            on:click=move |_| mode.set(m)
                        >
                            {super::mode_word(Some(m))}
                        </button>
                    }
                })
                .collect_view();
            view! { <div class="segmented" role="group" aria-label="Mode">{buttons}</div> }
        })
    };
    // Rebuilt only when the mode tab or the edit toggle changes, never while dragging.
    let body = move || {
        let m = mode.get();
        let editable = editable.get();
        let current = Signal::derive(move || {
            rules.with(|r| {
                r.get(m).cloned().unwrap_or(ModeRules {
                    base: Setpoint::Off,
                    blocks: Vec::new(),
                })
            })
        });
        let change = Callback::new(move |next: ModeRules| {
            on_change.run(with_mode(&rules.get_untracked(), m, next));
        });
        rules.with_untracked(|r| r.get(m).is_some()).then(
            || view! { <WeekTimeline rules=current editable=editable on_change=change now=now /> },
        )
    };
    view! {
        {tabs}
        {body}
    }
}

fn report(toasts: Toasts, result: Result<(), ApiError>, on_changed: Callback<()>) {
    match result {
        Ok(()) => on_changed.run(()),
        Err(e) => toasts.push(e.error),
    }
}

/// A learned proposal: its changes, the proposed week, Edit, Approve and Reject.
#[component]
pub fn ProposalPanel(
    zone: String,
    proposal: Proposal,
    approved: Signal<Rules>,
    /// Called after an approval or rejection so the screen refetches.
    on_changed: Callback<()>,
    /// Today and the minute of the day in the house time zone.
    now: Signal<(Day, u16)>,
) -> impl IntoView {
    let toasts = expect_context::<Toasts>();
    let learned = proposal.rules.clone();
    let edited = RwSignal::new(proposal.rules.clone());
    let editing = RwSignal::new(false);
    let busy = RwSignal::new(false);
    let title = match proposal.kind {
        ProposalKind::Initial => "Proposed schedule",
        ProposalKind::Change => "Proposed changes",
    };
    let dirty = {
        let learned = learned.clone();
        move || edited.with(|e| *e != learned)
    };
    let changes = {
        let learned = proposal.changes.clone();
        move || {
            if editing.get() {
                approved.with_untracked(|a| edited.with(|e| diff(Some(a), e)))
            } else {
                learned.clone()
            }
        }
    };
    let changes_view = move || {
        let lines = changes();
        if lines.is_empty() {
            view! { <p class="climate-proposal__none">"No change from the approved rules"</p> }
                .into_any()
        } else {
            let items = lines
                .into_iter()
                .map(|l| view! { <li>{l}</li> })
                .collect_view();
            view! { <ul class="climate-proposal__changes">{items}</ul> }.into_any()
        }
    };

    let approve_zone = zone.clone();
    let approve_dirty = dirty.clone();
    let approve = move |_| {
        if busy.get_untracked() {
            return;
        }
        let rules = edited.get_untracked();
        let send_edited = approve_dirty();
        if send_edited && let Err(e) = rules.validate() {
            toasts.push(e);
            return;
        }
        busy.set(true);
        let zone = approve_zone.clone();
        spawn_local(async move {
            let result = api::approve(&zone, send_edited.then_some(&rules)).await;
            busy.try_set(false);
            report(toasts, result, on_changed);
        });
    };
    let reject_zone = zone;
    let reject = move |_| {
        if busy.get_untracked() {
            return;
        }
        busy.set(true);
        let zone = reject_zone.clone();
        spawn_local(async move {
            let result = api::reject(&zone).await;
            busy.try_set(false);
            report(toasts, result, on_changed);
        });
    };
    let discard = {
        let learned = learned.clone();
        move |_| {
            edited.set(learned.clone());
            editing.set(false);
        }
    };

    view! {
        <section class="climate-proposal" aria-label=title>
            <h3 class="climate-proposal__title">{title}</h3>
            // The learned changes read above the week; while editing, the live diff sits under it.
            {
                let learned = changes_view.clone();
                move || (!editing.get()).then(&learned)
            }
            <RulesTimeline
                rules=edited.into()
                editable=editing.into()
                on_change=Callback::new(move |next| edited.set(next))
                now=now
            />
            {move || editing.get().then(&changes_view)}
            <div class="climate-proposal__actions">
                {move || {
                    if editing.get() {
                        view! {
                            <button type="button" class="btn btn--compact" on:click=discard.clone()>
                                "Discard edits"
                            </button>
                        }
                            .into_any()
                    } else {
                        view! {
                            <button
                                type="button"
                                class="btn btn--compact"
                                on:click=move |_| editing.set(true)
                            >
                                "Edit"
                            </button>
                        }
                            .into_any()
                    }
                }}
                <button
                    type="button"
                    class="btn btn--primary"
                    disabled=move || busy.get()
                    on:click=approve
                >
                    {
                        let dirty = dirty.clone();
                        move || if dirty() { "Approve edited" } else { "Approve" }
                    }
                </button>
                <button type="button" class="btn" disabled=move || busy.get() on:click=reject>
                    "Reject"
                </button>
            </div>
        </section>
    }
}

/// The zone's approved rules on a timeline; "Edit rules" saves through the zone settings.
#[component]
pub fn RulesPanel(
    zone: Signal<ZoneView>,
    on_changed: Callback<()>,
    /// Today and the minute of the day in the house time zone.
    now: Signal<(Day, u16)>,
) -> impl IntoView {
    let toasts = expect_context::<Toasts>();
    let id = zone.with_untracked(|z| z.zone.clone());
    let editing = RwSignal::new(false);
    let draft = RwSignal::new(Rules::default());
    let busy = RwSignal::new(false);
    // The approved rules while viewing, the draft while editing.
    let shown = Signal::derive(move || {
        if editing.get() {
            draft.get()
        } else {
            zone.with(|z| z.settings.rules.clone())
        }
    });
    let empty = move || zone.with(|z| z.settings.rules.is_empty());
    let save = Callback::new(move |()| {
        if busy.get_untracked() {
            return;
        }
        let rules = draft.get_untracked();
        if let Err(e) = rules.validate() {
            toasts.push(e);
            return;
        }
        let mut settings = zone.with_untracked(|z| z.settings.clone());
        settings.rules = rules;
        busy.set(true);
        let id = id.clone();
        spawn_local(async move {
            let result = api::put_zone(&id, &settings).await;
            busy.try_set(false);
            if result.is_ok() {
                editing.try_set(false);
            }
            report(toasts, result, on_changed);
        });
    });
    view! {
        <section class="climate-rules" aria-label="Approved rules">
            <h3 class="climate-proposal__title">"This week"</h3>
            // A room without rules starts an empty heating week here; saving it approves it, and
            // the hub turns a pending room active.
            <Show
                when=move || !empty() || editing.get()
                fallback=move || {
                    view! {
                        <p class="climate-proposal__none">"No approved schedule yet"</p>
                        <div class="climate-proposal__actions">
                            <button
                                type="button"
                                class="btn btn--compact"
                                on:click=move |_| {
                                    draft.set(Rules {
                                        heat: Some(ModeRules {
                                            base: Setpoint::Temp(NEW_BASE),
                                            blocks: Vec::new(),
                                        }),
                                        cool: None,
                                    });
                                    editing.set(true);
                                }
                            >
                                "Create schedule"
                            </button>
                        </div>
                    }
                }
            >
                <RulesTimeline
                    rules=shown
                    editable=editing.into()
                    on_change=Callback::new(move |next| draft.set(next))
                    now=now
                />
                <div class="climate-proposal__actions">
                    {move || {
                        if editing.get() {
                            view! {
                                <button
                                    type="button"
                                    class="btn btn--primary"
                                    disabled=move || busy.get()
                                    on:click=move |_| save.run(())
                                >
                                    "Save"
                                </button>
                                <button
                                    type="button"
                                    class="btn"
                                    on:click=move |_| editing.set(false)
                                >
                                    "Cancel"
                                </button>
                            }
                                .into_any()
                        } else {
                            view! {
                                <button
                                    type="button"
                                    class="btn btn--compact"
                                    on:click=move |_| {
                                        draft.set(zone.with_untracked(|z| z.settings.rules.clone()));
                                        editing.set(true);
                                    }
                                >
                                    "Edit rules"
                                </button>
                            }
                                .into_any()
                        }
                    }}
                </div>
            </Show>
        </section>
    }
}
