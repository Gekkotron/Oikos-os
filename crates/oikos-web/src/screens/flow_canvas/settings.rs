//! The settings of one card: its name and the fields of its kind (the schedule picker, the
//! flow switch, raw JSON for values and commands), and a button that deletes the card. Edits
//! write into the draft, one undo step per field change.

use leptos::prelude::*;
use oikos_core::flow::{Missing, Node, NodeKind, Op};

use super::Editor;
use super::graph::Selection;
use crate::flow_edit::remove_node;
use crate::house::House;
use crate::screens::flow_schedule::ScheduleField;

/// How the form writes into the draft. The first write after the user starts a change (focus
/// on a field, a click, a committed `change`) records an undo step; the keystrokes that follow
/// in the same field join it.
#[derive(Clone, Copy)]
struct Form {
    editor: Editor,
    armed: StoredValue<bool>,
}

impl Form {
    /// Applies `mutate` to node `id`; a write that changes nothing is dropped.
    fn update(self, id: &str, mutate: impl FnOnce(&mut Node)) {
        let draft = self.editor.draft;
        let Some(mut node) = draft.with_untracked(|f| f.node(id).cloned()) else {
            return;
        };
        mutate(&mut node);
        if draft.with_untracked(|f| f.node(id) == Some(&node)) {
            return;
        }
        if self.armed.get_value() {
            self.armed.set_value(false);
            self.editor
                .history
                .update_value(|h| h.record(draft.get_untracked()));
        }
        draft.update(|f| {
            if let Some(n) = f.nodes.iter_mut().find(|n| n.id == node.id) {
                *n = node;
            }
        });
    }

    /// The next write starts a new undo step.
    fn arm(self) {
        self.armed.set_value(true);
    }
}

fn op_str(op: Op) -> &'static str {
    match op {
        Op::Eq => "eq",
        Op::Ne => "ne",
        Op::Gt => "gt",
        Op::Lt => "lt",
        Op::Gte => "gte",
        Op::Lte => "lte",
        Op::IsDark => "is_dark",
    }
}

fn op_from(s: &str) -> Option<Op> {
    match s {
        "eq" => Some(Op::Eq),
        "ne" => Some(Op::Ne),
        "gt" => Some(Op::Gt),
        "lt" => Some(Op::Lt),
        "gte" => Some(Op::Gte),
        "lte" => Some(Op::Lte),
        "is_dark" => Some(Op::IsDark),
        _ => None,
    }
}

/// A text field whose value is a JSON scalar. The user types raw JSON (`true`, `21.5`, `"ON"`);
/// `None` means absent (an empty textbox clears an optional field). On a parse error the field
/// shows a hint and does not touch `write`.
fn json_text(initial: &serde_json::Value) -> String {
    match initial {
        serde_json::Value::String(s) => format!("\"{s}\""),
        other => other.to_string(),
    }
}

#[component]
fn JsonField(
    label: String,
    initial: serde_json::Value,
    optional: bool,
    write: Callback<Option<serde_json::Value>>,
) -> impl IntoView {
    let raw = RwSignal::new(json_text(&initial));
    let error = RwSignal::new(None::<String>);
    let on_input = move |ev: web_sys::Event| {
        let text = event_target_value(&ev);
        raw.set(text.clone());
        let trimmed = text.trim();
        if trimmed.is_empty() {
            if optional {
                error.set(None);
                write.run(None);
            } else {
                error.set(Some("required".into()));
            }
            return;
        }
        match serde_json::from_str::<serde_json::Value>(trimmed) {
            Ok(value) => {
                error.set(None);
                write.run(Some(value));
            }
            Err(e) => error.set(Some(e.to_string())),
        }
    };
    view! {
        <label class="node-editor__field">
            <span class="node-editor__label">{label}</span>
            <input
                type="text"
                class="field"
                spellcheck="false"
                autocomplete="off"
                prop:value=move || raw.get()
                on:input=on_input
                aria-invalid=move || error.with(|e| e.is_some()).then_some("true")
            />
            {move || error.get().map(|e| view! {
                <span class="node-editor__error">{e}</span>
            })}
        </label>
    }
}

#[component]
fn TextField(
    label: String,
    initial: String,
    placeholder: String,
    optional: bool,
    write: Callback<Option<String>>,
) -> impl IntoView {
    let raw = RwSignal::new(initial);
    let on_input = move |ev: web_sys::Event| {
        let text = event_target_value(&ev);
        raw.set(text.clone());
        let trimmed = text.trim();
        if trimmed.is_empty() {
            if optional {
                write.run(None);
            }
        } else {
            write.run(Some(trimmed.to_owned()));
        }
    };
    view! {
        <label class="node-editor__field">
            <span class="node-editor__label">{label}</span>
            <input
                type="text"
                class="field"
                placeholder=placeholder
                spellcheck="false"
                autocomplete="off"
                prop:value=move || raw.get()
                on:input=on_input
            />
        </label>
    }
}

/// The settings of the selected card, or nothing when no card is selected. The form is built
/// from the draft when the selection changes or undo/redo replace the draft.
#[component]
pub fn NodeEditor(editor: Editor, selected: RwSignal<Option<Selection>>) -> impl IntoView {
    let node_id = move || match selected.get() {
        Some(Selection::Node(id)) => Some(id),
        _ => None,
    };
    view! {
        <Show when=move || node_id().is_some()>
            {move || {
                editor.restored.track();
                let Some(id) = node_id() else { return ().into_any(); };
                let Some(node) = editor.draft.with_untracked(|f| f.node(&id).cloned()) else {
                    return ().into_any();
                };
                let form = Form {
                    editor,
                    armed: StoredValue::new(false),
                };
                node_editor_body(form, selected, id, node).into_any()
            }}
        </Show>
    }
}

fn node_editor_body(
    form: Form,
    selected: RwSignal<Option<Selection>>,
    id: String,
    node: Node,
) -> impl IntoView {
    let kind_label = match &node.kind {
        NodeKind::TriggerState(_) => "Trigger",
        NodeKind::TriggerSchedule(_) => "Schedule",
        NodeKind::TriggerSun(_) => "Sun",
        NodeKind::If(_) => "If",
        NodeKind::Delay(_) => "Delay",
        NodeKind::Command(_) => "Command",
        NodeKind::FlowEnable(_) => "Flow switch",
    };
    let id_display = id.clone();
    let close = move |_: web_sys::MouseEvent| selected.set(None);
    let initial_name = node.name.clone().unwrap_or_default();
    let name_id = id.clone();
    let name_write = Callback::new(move |value: Option<String>| {
        form.update(&name_id, |n| n.name = value);
    });
    let delete_id = id.clone();
    let delete = move |_: web_sys::MouseEvent| {
        form.editor.edit(|f| remove_node(f, &delete_id));
        selected.set(None);
    };
    let kind_fields = kind_fields(form, id.clone(), node.kind);
    view! {
        <section
            class="node-editor"
            aria-label="Node editor"
            on:focusin=move |_| form.arm()
            on:click=move |_| form.arm()
            on:change=move |_| form.arm()
        >
            <header class="node-editor__header">
                <div class="node-editor__title">
                    <span class="chip">{kind_label}</span>
                    <code class="node-editor__id">{id_display}</code>
                </div>
                <button
                    type="button"
                    class="btn btn--ghost"
                    aria-label="Close editor"
                    on:click=close
                >
                    "Close"
                </button>
            </header>
            <TextField
                label="Name".into()
                initial=initial_name
                placeholder="(auto)".into()
                optional=true
                write=name_write
            />
            {kind_fields}
            <footer class="node-editor__footer">
                <button type="button" class="btn btn--ghost node-editor__delete" on:click=delete>
                    "Delete card"
                </button>
                <span class="node-editor__keys">
                    <kbd>"⌫"</kbd>
                    " delete · "
                    <kbd>"⌘D"</kbd>
                    " duplicate"
                </span>
            </footer>
        </section>
    }
}

/// The select value of an `if`'s missing-value choice; absent reads as `fail`.
fn missing_str(missing: Option<Missing>) -> &'static str {
    match missing {
        None | Some(Missing::Fail) => "fail",
        Some(Missing::True) => "true",
        Some(Missing::False) => "false",
    }
}

/// The choice behind a select value; `fail` is the default so it is stored as absent.
fn missing_from(value: &str) -> Option<Missing> {
    match value {
        "true" => Some(Missing::True),
        "false" => Some(Missing::False),
        _ => None,
    }
}

/// The `flow.enable` editor: which flow it switches ("This flow" or another by name) and
/// whether it enables or disables it.
fn flow_enable_fields(
    form: Form,
    id: String,
    p: oikos_core::flow::FlowEnableParams,
) -> impl IntoView {
    let house = expect_context::<House>();
    let own_id = form.editor.draft.with_untracked(|f| f.id.clone());
    let others: Vec<(String, String)> = house.flows.with_untracked(|flows| {
        flows
            .iter()
            .filter(|f| f.definition.id != own_id)
            .map(|f| (f.definition.id.clone(), f.definition.name.clone()))
            .collect()
    });
    let target = p.flow.clone();
    let known = target
        .as_ref()
        .is_none_or(|t| others.iter().any(|(id, _)| id == t));
    let target_id = id.clone();
    let on_target = move |ev: web_sys::Event| {
        let value = event_target_value(&ev);
        form.update(&target_id, |n| {
            if let NodeKind::FlowEnable(ref mut f) = n.kind {
                f.flow = (!value.is_empty()).then_some(value);
            }
        });
    };
    let state_id = id;
    let on_state = move |ev: web_sys::Event| {
        let enabled = event_target_value(&ev) == "enable";
        form.update(&state_id, |n| {
            if let NodeKind::FlowEnable(ref mut f) = n.kind {
                f.enabled = enabled;
            }
        });
    };
    let enabled = p.enabled;
    view! {
        <label class="node-editor__field">
            <span class="node-editor__label">"Flow"</span>
            <select class="field" on:change=on_target>
                <option value="" selected=target.is_none()>"This flow"</option>
                {(!known)
                    .then(|| {
                        let gone = target.clone().unwrap_or_default();
                        view! { <option value=gone.clone() selected=true>{gone.clone()}</option> }
                    })}
                {others
                    .into_iter()
                    .map(|(id, name)| {
                        let selected = target.as_deref() == Some(id.as_str());
                        view! { <option value=id selected=selected>{name}</option> }
                    })
                    .collect_view()}
            </select>
        </label>
        <label class="node-editor__field">
            <span class="node-editor__label">"Action"</span>
            <select class="field" on:change=on_state>
                <option value="enable" selected=enabled>"Enable"</option>
                <option value="disable" selected=!enabled>"Disable"</option>
            </select>
        </label>
    }
}

fn kind_fields(form: Form, id: String, kind: NodeKind) -> impl IntoView {
    match kind {
        NodeKind::TriggerState(p) => {
            let entity_id = id.clone();
            let entity_write = Callback::new(move |value: Option<String>| {
                form.update(&entity_id, |n| {
                    if let NodeKind::TriggerState(ref mut t) = n.kind {
                        t.entity = value.unwrap_or_default();
                    }
                });
            });
            let to_id = id.clone();
            let to_write = Callback::new(move |value: Option<serde_json::Value>| {
                form.update(&to_id, |n| {
                    if let NodeKind::TriggerState(ref mut t) = n.kind {
                        t.to = value;
                    }
                });
            });
            let from_id = id;
            let from_write = Callback::new(move |value: Option<serde_json::Value>| {
                form.update(&from_id, |n| {
                    if let NodeKind::TriggerState(ref mut t) = n.kind {
                        t.from = value;
                    }
                });
            });
            view! {
                <TextField
                    label="Entity".into()
                    initial=p.entity
                    placeholder="entity.id".into()
                    optional=false
                    write=entity_write
                />
                <JsonField
                    label="Fires when it becomes (JSON, optional)".into()
                    initial=p.to.unwrap_or(serde_json::Value::Null)
                    optional=true
                    write=to_write
                />
                <JsonField
                    label="Only after (previous, JSON, optional)".into()
                    initial=p.from.unwrap_or(serde_json::Value::Null)
                    optional=true
                    write=from_write
                />
            }
            .into_any()
        }
        NodeKind::If(p) => {
            let op_id = id.clone();
            let on_op = move |ev: web_sys::Event| {
                if let Some(op) = op_from(&event_target_value(&ev)) {
                    form.update(&op_id, |n| {
                        if let NodeKind::If(ref mut i) = n.kind {
                            i.op = op;
                        }
                    });
                }
            };
            let current_op = p.op;
            let value_id = id.clone();
            let value_write = Callback::new(move |value: Option<serde_json::Value>| {
                if let Some(v) = value {
                    form.update(&value_id, |n| {
                        if let NodeKind::If(ref mut i) = n.kind {
                            i.value = Some(v);
                        }
                    });
                }
            });
            let missing_id = id.clone();
            let current_missing = p.missing;
            let on_missing = move |ev: web_sys::Event| {
                let missing = missing_from(&event_target_value(&ev));
                form.update(&missing_id, |n| {
                    if let NodeKind::If(ref mut i) = n.kind {
                        i.missing = missing;
                    }
                });
            };
            let entity_id = id;
            let entity_write = Callback::new(move |value: Option<String>| {
                form.update(&entity_id, |n| {
                    if let NodeKind::If(ref mut i) = n.kind {
                        i.entity = value;
                    }
                });
            });
            view! {
                <label class="node-editor__field">
                    <span class="node-editor__label">"Compare"</span>
                    <select class="field" on:change=on_op>
                        {["eq", "ne", "gt", "lt", "gte", "lte"]
                            .into_iter()
                            .map(|o| {
                                let selected = op_str(current_op) == o;
                                view! {
                                    <option value=o selected=selected>{o}</option>
                                }
                            })
                            .collect_view()}
                    </select>
                </label>
                <JsonField
                    label="Value (JSON)".into()
                    initial=p.value.unwrap_or(serde_json::Value::Null)
                    optional=false
                    write=value_write
                />
                <TextField
                    label="Entity (defaults to trigger value)".into()
                    initial=p.entity.unwrap_or_default()
                    placeholder="entity.id".into()
                    optional=true
                    write=entity_write
                />
                <label class="node-editor__field">
                    <span class="node-editor__label">"When the value is unknown"</span>
                    <select class="field" on:change=on_missing>
                        {[
                            ("fail", "Fail"),
                            ("true", "Take Yes"),
                            ("false", "Take No"),
                        ]
                            .into_iter()
                            .map(|(value, label)| {
                                let selected = missing_str(current_missing) == value;
                                view! {
                                    <option value=value selected=selected>{label}</option>
                                }
                            })
                            .collect_view()}
                    </select>
                </label>
            }
            .into_any()
        }
        NodeKind::TriggerSchedule(p) => {
            let write = Callback::new(move |cron: String| {
                form.update(&id, |n| {
                    if let NodeKind::TriggerSchedule(ref mut s) = n.kind {
                        s.cron = cron;
                    }
                });
            });
            view! { <ScheduleField initial=p.cron write=write /> }.into_any()
        }
        NodeKind::TriggerSun(_) => ().into_any(),
        NodeKind::FlowEnable(p) => flow_enable_fields(form, id, p).into_any(),
        NodeKind::Delay(p) => {
            let for_id = id;
            let for_write = Callback::new(move |value: Option<String>| {
                form.update(&for_id, |n| {
                    if let NodeKind::Delay(ref mut d) = n.kind {
                        d.for_ = value.unwrap_or_default();
                    }
                });
            });
            view! {
                <TextField
                    label="Duration".into()
                    initial=p.for_
                    placeholder="5m, 1h30m, 45s".into()
                    optional=false
                    write=for_write
                />
            }
            .into_any()
        }
        NodeKind::Command(p) => {
            let entity_id = id.clone();
            let entity_write = Callback::new(move |value: Option<String>| {
                form.update(&entity_id, |n| {
                    if let NodeKind::Command(ref mut c) = n.kind {
                        c.entity = value.unwrap_or_default();
                    }
                });
            });
            let cmd_id = id;
            let command_json = serde_json::to_value(&p.command).unwrap_or(serde_json::Value::Null);
            let cmd_write = Callback::new(move |value: Option<serde_json::Value>| {
                if let Some(v) = value
                    && let Ok(cmd) = serde_json::from_value(v)
                {
                    form.update(&cmd_id, |n| {
                        if let NodeKind::Command(ref mut c) = n.kind {
                            c.command = cmd;
                        }
                    });
                }
            });
            view! {
                <TextField
                    label="Entity".into()
                    initial=p.entity
                    placeholder="entity.id".into()
                    optional=false
                    write=entity_write
                />
                <JsonField
                    label=r#"Command (JSON, e.g. {"type":"turn_on"})"#.into()
                    initial=command_json
                    optional=false
                    write=cmd_write
                />
            }
            .into_any()
        }
    }
}
