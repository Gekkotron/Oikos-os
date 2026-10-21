//! The sheet panel of a generic entity: one row per announced control, drawn by shape, plus the
//! signal, update and picture subpanels its placement calls for.

use leptos::prelude::*;
use oikos_api::EntityView;
use oikos_core::model::{Command, GenericState, GenericValue, Kind, StateValue};
use oikos_core::types::{self, Placement, Shape, TypeDef};

use crate::cards::Sender;
use crate::cards::generic::{control_label, control_view};
use crate::components::now::{Now, unix_now};
use crate::format::word;

/// "just now", "5 minutes ago", "3 hours ago", "2 days ago".
pub(crate) fn relative_time(now: u64, then: u64) -> String {
    let secs = now.saturating_sub(then);
    let plural = |n: u64, unit: &str| format!("{n} {unit}{} ago", if n == 1 { "" } else { "s" });
    match secs {
        0..=59 => "just now".into(),
        60..=3599 => plural(secs / 60, "minute"),
        3600..=86_399 => plural(secs / 3600, "hour"),
        _ => plural(secs / 86_400, "day"),
    }
}

fn generic_state(entity: &EntityView) -> GenericState {
    match &entity.state.value {
        StateValue::Generic(g) => g.clone(),
        _ => GenericState::default(),
    }
}

fn text_field(
    entity: &EntityView,
    def: &TypeDef,
    name: &str,
    secret: bool,
    sender: Sender,
) -> AnyView {
    let id = entity.id.clone();
    let control = name.to_owned();
    let current = match (
        secret,
        def.controls
            .get(name)
            .and_then(|c| c.field.as_deref())
            .and_then(|f| generic_state(entity).fields.get(f).cloned()),
    ) {
        (false, Some(GenericValue::Text(s))) => s,
        _ => String::new(),
    };
    view! {
        <input
            class="input"
            type=if secret { "password" } else { "text" }
            aria-label=control_label(name)
            value=current
            on:change=move |ev| {
                sender
                    .send(
                        &id,
                        Command::Control {
                            control: control.clone(),
                            value: Some(serde_json::Value::String(event_target_value(&ev))),
                        },
                        None,
                    );
            }
        />
    }
    .into_any()
}

fn signal_line(entity: &EntityView, def: &TypeDef, now: u64) -> Option<AnyView> {
    let g = generic_state(entity);
    let last = match g.fields.get(def.primary.as_deref().unwrap_or("last"))? {
        GenericValue::Text(s) => word(s),
        _ => return None,
    };
    let at = match g.fields.get("at") {
        Some(GenericValue::Number(ms)) if *ms >= 0.0 => Some((*ms / 1000.0) as u64),
        _ => entity.state.last_update,
    };
    let text = match at {
        Some(t) => format!("Last: {last}, {}", relative_time(now, t)),
        None => format!("Last: {last}"),
    };
    Some(view! { <p class="sheet__generic__last">{text}</p> }.into_any())
}

fn update_line(entity: &EntityView) -> Option<AnyView> {
    let g = generic_state(entity);
    let text = |k: &str| match g.fields.get(k) {
        Some(GenericValue::Text(s)) if !s.is_empty() => Some(s.clone()),
        _ => None,
    };
    let installed = text("installed")?;
    let latest = text("latest").unwrap_or_else(|| installed.clone());
    let available = installed != latest;
    Some(
        view! {
            <div class="sheet__generic__row">
                <span>{format!("Firmware {installed} → {latest}")}</span>
                {available.then(|| view! { <span class="chip chip--selected">"Update available"</span> })}
            </div>
        }
        .into_any(),
    )
}

fn picture_primary(def: &TypeDef) -> bool {
    def.primary
        .as_deref()
        .and_then(|p| def.fields.get(p))
        .is_some_and(|f| f.shape == Shape::Picture)
}

#[component]
pub fn GenericPanel(entity: Memo<Option<EntityView>>) -> impl IntoView {
    let sender = Sender::from_context();
    let now = use_context::<Now>();
    let applies = Memo::new(move |_| {
        entity.with(|e| {
            e.as_ref()
                .is_some_and(|e| e.kind == Kind::Generic && types::type_def(&e.component).is_some())
        })
    });
    view! {
        <Show when=move || applies.get()>
            {move || {
                let e = entity.get()?;
                let def = types::type_def(&e.component)?;
                let caps = e.capabilities.generic.clone().unwrap_or_default();
                let now_s = now.map_or_else(unix_now, |n| n.0.get());
                let mut sections: Vec<AnyView> = Vec::new();
                match def.placement {
                    Placement::Card => {
                        for name in &caps.controls {
                            let Some(ctl) = def.controls.get(name) else { continue };
                            let widget = if ctl.shape == Shape::Text && !ctl.press {
                                text_field(&e, def, name, caps.secret, sender)
                            } else {
                                control_view(&e, def, &caps, name, sender)
                            };
                            sections
                                .push(
                                    view! {
                                        <div class="sheet__generic__row">
                                            <span class="sheet__generic__label">{control_label(name)}</span>
                                            {widget}
                                        </div>
                                    }
                                        .into_any(),
                                );
                        }
                    }
                    Placement::Signal => sections.extend(signal_line(&e, def, now_s)),
                    Placement::Update => sections.extend(update_line(&e)),
                    Placement::Target => {}
                }
                if picture_primary(def) {
                    let src = format!(
                        "/api/v1/entities/{}/picture?v={}",
                        crate::api::encode_id(&e.id),
                        e.state.last_update.unwrap_or(0),
                    );
                    sections
                        .push(
                            view! { <img class="sheet__generic__picture" src=src loading="lazy" alt="" /> }
                                .into_any(),
                        );
                }
                (!sections.is_empty())
                    .then(|| {
                        view! {
                            <section class="sheet__panel sheet__generic">
                                <h2 class="sheet__heading">"Details"</h2>
                                {sections}
                            </section>
                        }
                    })
            }}
        </Show>
    }
}

#[cfg(test)]
mod tests {
    use super::relative_time;

    #[test]
    fn relative_time_ladder() {
        assert_eq!(relative_time(100, 70), "just now");
        assert_eq!(relative_time(1000, 1000 - 300), "5 minutes ago");
        assert_eq!(relative_time(10_000, 10_000 - 7200), "2 hours ago");
        assert_eq!(relative_time(1_000_000, 1_000_000 - 86_400), "1 day ago");
        assert_eq!(relative_time(5, 10), "just now");
    }
}
