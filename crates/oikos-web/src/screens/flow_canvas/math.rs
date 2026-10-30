//! The settings of a math card: its inputs (a name, an entity and an optional average window
//! each), the formula with its live error or preview, the result's name and the number entity
//! the result is saved to. While the card is invalid Save is blocked, with the reason under the
//! formula.

use leptos::prelude::*;
use oikos_core::flow::{Flow, MAX_INPUTS, MathInput, MathParams, NodeKind, Scalar, is_valid_name};
use oikos_core::model::Kind;

use super::settings::{Form, word};
use crate::entity_picker::EntityPickField;
use crate::entity_picker::needs::keep;
use crate::flow_math::{Preview, input_name_for, is_placeholder_name, preview};
use crate::format::entity_title;
use crate::house::House;
use crate::i18n::flows::{FlowText, math_averaged_note, math_unknown, math_value, use_flow_text};
use crate::i18n::use_lang;
use crate::screens::flow_schedule::SaveBlocked;

/// The average windows the select offers; any other window the card holds is added to the list.
const WINDOWS: [&str; 4] = ["1m", "5m", "15m", "1h"];

/// The math params of node `id` in the draft.
fn params_of(form: Form, id: &str) -> Option<MathParams> {
    form.editor
        .draft
        .with(|f| match f.node(id).map(|n| &n.kind) {
            Some(NodeKind::Math(p)) => Some(p.clone()),
            _ => None,
        })
}

/// The math params of node `id` in `flow`.
fn math_mut<'a>(flow: &'a mut Flow, id: &str) -> Option<&'a mut MathParams> {
    flow.nodes
        .iter_mut()
        .find(|n| n.id == id)
        .and_then(|n| match &mut n.kind {
            NodeKind::Math(p) => Some(p),
            _ => None,
        })
}

/// Applies `mutate` to the math params of node `id`.
fn update(form: Form, id: &str, mutate: impl FnOnce(&mut MathParams)) {
    form.update(id, |n| {
        if let NodeKind::Math(ref mut p) = n.kind {
            mutate(p);
        }
    });
}

pub(super) fn math_fields(form: Form, id: String) -> impl IntoView {
    let t = use_flow_text();
    let lang = use_lang();
    let draft = form.editor.draft;
    let house = expect_context::<House>();

    // The preview follows the draft and the house's readings.
    let shown = Memo::new({
        let id = id.clone();
        move |_| {
            house.entities.with(|entities| {
                let value = |entity: &str| {
                    entities
                        .get(entity)
                        .and_then(|e| Scalar::of_state(&e.state.value))
                };
                draft.with(|f| match f.node(&id).map(|n| &n.kind) {
                    Some(NodeKind::Math(p)) => Some(preview(&id, p, f, &value)),
                    _ => None,
                })
            })
        }
    });
    // Save waits for a formula the hub would accept.
    if let Some(blocked) = use_context::<SaveBlocked>() {
        Effect::new(move |_| {
            blocked
                .0
                .set(matches!(shown.get(), Some(Preview::Error(_))));
        });
        on_cleanup(move || blocked.0.set(false));
    }

    // Rows are rebuilt when inputs come or go, not on every edit.
    let version = RwSignal::new(0_u32);
    let count = Signal::derive({
        let id = id.clone();
        move || params_of(form, &id).map_or(0, |p| p.inputs.len())
    });
    let rows = {
        let id = id.clone();
        move || {
            version.track();
            (0..count.get_untracked())
                .map(|i| input_row(form, id.clone(), i, version))
                .collect_view()
        }
    };
    let add_id = id.clone();
    let add = move |_: web_sys::MouseEvent| {
        form.editor.edit(|f| {
            if let Some(p) = math_mut(f, &add_id) {
                let taken: Vec<&str> = p.inputs.iter().map(|i| i.name.as_str()).collect();
                let name = input_name_for("input", &taken);
                p.inputs.push(MathInput {
                    name,
                    entity: String::new(),
                    average: None,
                });
            }
        });
        version.update(|v| *v += 1);
    };

    let formula_id = id.clone();
    let on_formula = move |ev: web_sys::Event| {
        let text = event_target_value(&ev);
        update(form, &formula_id, |p| p.formula = text);
    };
    let window_note = {
        let id = id.clone();
        move || {
            params_of(form, &id)
                .and_then(|p| p.inputs.into_iter().find_map(|i| i.average))
                .unwrap_or_default()
        }
    };
    let line = move || {
        shown.get().map(|preview| match preview {
            Preview::Error(reason) => {
                view! { <p class="node-editor__error math-line" role="alert">{reason}</p> }
                    .into_any()
            }
            Preview::Unknown(name) => view! {
                <p class="node-editor__hint math-line">{math_unknown(lang(), &name)}</p>
            }
            .into_any(),
            Preview::Value {
                value,
                averaged_now,
            } => view! {
                <p class="math-line math-line--value" role="status">
                    <span class="math-line__value">"= " {math_value(lang(), value)}</span>
                    {averaged_now
                        .then(|| {
                            view! {
                                <span class="node-editor__hint">
                                    {math_averaged_note(lang(), &window_note())}
                                </span>
                            }
                        })}
                </p>
            }
            .into_any(),
        })
    };

    let formula = Signal::derive({
        let id = id.clone();
        move || params_of(form, &id).map(|p| p.formula).unwrap_or_default()
    });
    let name_id = id.clone();
    let on_name = move |ev: web_sys::Event| {
        let text = event_target_value(&ev);
        update(form, &name_id, |p| p.name = text);
    };
    let result_name = Signal::derive({
        let id = id.clone();
        move || params_of(form, &id).map(|p| p.name).unwrap_or_default()
    });
    let save_to = Signal::derive({
        let id = id.clone();
        move || {
            params_of(form, &id)
                .and_then(|p| p.save_to)
                .filter(|e| !e.is_empty())
        }
    });
    let save_id = id.clone();
    let on_save_to = Callback::new(move |value: Option<String>| {
        update(form, &save_id, |p| p.save_to = value);
    });
    let name_hint = move || {
        let name = result_name.get();
        if is_valid_name(&name) {
            view! {
                <span class="node-editor__hint">
                    {t(FlowText::MathResultHint).replace("{name}", &format!("{{{name}}}"))}
                </span>
            }
            .into_any()
        } else {
            view! { <span class="node-editor__error">{t(FlowText::MathNameInvalid)}</span> }
                .into_any()
        }
    };

    view! {
        <p class="node-editor__hint">{move || t(FlowText::MathSub)}"."</p>
        <div class="node-editor__field math-inputs">
            <span class="node-editor__label">{move || t(FlowText::MathInputs)}</span>
            {rows}
            <button
                type="button"
                class="btn btn--ghost math-inputs__add"
                disabled=move || { count.get() >= MAX_INPUTS }
                on:click=add
            >
                {move || t(FlowText::MathAddInput)}
            </button>
            <span class="node-editor__hint">{move || t(FlowText::MathAverageNote)}</span>
        </div>
        <label class="node-editor__field">
            <span class="node-editor__label">{move || t(FlowText::MathFormula)}</span>
            <input
                type="text"
                class="field math-formula"
                spellcheck="false"
                autocomplete="off"
                prop:value=move || formula.get()
                on:input=on_formula
            />
            {line}
        </label>
        <label class="node-editor__field">
            <span class="node-editor__label">{move || t(FlowText::MathResultName)}</span>
            <input
                type="text"
                class="field math-name"
                spellcheck="false"
                autocomplete="off"
                prop:value=move || result_name.get()
                aria-invalid=move || (!is_valid_name(&result_name.get())).then_some("true")
                on:input=on_name
            />
            {name_hint}
        </label>
        <EntityPickField
            label=word(FlowText::MathSaveTo)
            value=save_to
            on_change=on_save_to
            pool=Some(keep(|e| e.kind == Kind::Number))
            pinned=Some(word(FlowText::MathNotSaved))
            compact=true
            class=Some("node-editor__field".to_owned())
        />
    }
}

/// One input row: its name, the entity it reads, the window it averages over and a remove button.
fn input_row(form: Form, id: String, index: usize, version: RwSignal<u32>) -> impl IntoView {
    let t = use_flow_text();
    let lang = use_lang();
    let house = expect_context::<House>();
    let current = move |id: &str| params_of(form, id).and_then(|p| p.inputs.get(index).cloned());
    let initial = current(&id).unwrap_or_else(|| MathInput {
        name: String::new(),
        entity: String::new(),
        average: None,
    });

    let name = Signal::derive({
        let id = id.clone();
        move || current(&id).map(|i| i.name).unwrap_or_default()
    });
    let entity = Signal::derive({
        let id = id.clone();
        move || current(&id).map(|i| i.entity).filter(|e| !e.is_empty())
    });
    let name_id = id.clone();
    let on_name = move |ev: web_sys::Event| {
        let text = event_target_value(&ev);
        update(form, &name_id, |p| {
            if let Some(input) = p.inputs.get_mut(index) {
                input.name = text;
            }
        });
    };
    // The first entity chosen for a row still named `input…` gives it a name drawn from the
    // entity (`Réseau` becomes `reseau`); later changes leave the name alone, since the
    // formula may use it.
    let entity_id = id.clone();
    let on_entity = Callback::new(move |value: Option<String>| {
        let title = value.as_ref().and_then(|e| {
            house
                .entities
                .with_untracked(|m| m.get(e).map(|e| entity_title(lang(), e)))
        });
        update(form, &entity_id, |p| {
            let others: Vec<String> = p
                .inputs
                .iter()
                .enumerate()
                .filter(|(i, _)| *i != index)
                .map(|(_, input)| input.name.clone())
                .collect();
            let Some(input) = p.inputs.get_mut(index) else {
                return;
            };
            let first_pick = input.entity.is_empty();
            input.entity = value.unwrap_or_default();
            if let (true, true, Some(title)) = (first_pick, is_placeholder_name(&input.name), title)
            {
                let taken: Vec<&str> = others.iter().map(String::as_str).collect();
                input.name = input_name_for(&title, &taken);
            }
        });
    });
    let window_id = id.clone();
    let on_window = move |ev: web_sys::Event| {
        let value = event_target_value(&ev);
        update(form, &window_id, |p| {
            if let Some(input) = p.inputs.get_mut(index) {
                input.average = (value != "now").then_some(value);
            }
        });
    };
    let remove_id = id;
    let remove = move |_: web_sys::MouseEvent| {
        form.editor.edit(|f| {
            if let Some(p) = math_mut(f, &remove_id).filter(|p| index < p.inputs.len()) {
                p.inputs.remove(index);
            }
        });
        version.update(|v| *v += 1);
    };

    let held = initial.average.clone();
    let extra = held
        .clone()
        .filter(|w| !WINDOWS.contains(&w.as_str()))
        .into_iter();
    let windows = WINDOWS.iter().map(|w| (*w).to_owned()).chain(extra);
    view! {
        <div class="math-input">
            <input
                type="text"
                class="field math-input__name"
                spellcheck="false"
                autocomplete="off"
                aria-label=move || t(FlowText::MathInputName)
                prop:value=name
                aria-invalid=move || (!is_valid_name(&name.get())).then_some("true")
                on:input=on_name
            />
            <EntityPickField
                label=word(FlowText::Entity)
                value=entity
                on_change=on_entity
                compact=true
                class=Some("math-input__entity".to_owned())
            />
            <select class="field math-input__window" on:change=on_window>
                <option value="now" selected=held.is_none()>
                    {move || t(FlowText::MathNow)}
                </option>
                {windows
                    .map(|w| {
                        let chosen = held.as_deref() == Some(w.as_str());
                        view! { <option value=w.clone() selected=chosen>{w.clone()}</option> }
                    })
                    .collect_view()}
            </select>
            <button
                type="button"
                class="btn btn--ghost btn--icon math-input__remove"
                aria-label=move || t(FlowText::MathRemoveInput)
                on:click=remove
            >
                "✕"
            </button>
        </div>
    }
}
