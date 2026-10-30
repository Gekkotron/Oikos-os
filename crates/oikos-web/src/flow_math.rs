//! Pure helpers of the math card's settings drawer: the live preview of the formula and the
//! name proposed for a new input.

use std::cell::RefCell;

use oikos_core::flow::{
    Flow, FlowError, MathParams, Scalar, check_math_card, is_valid_name, parse_formula,
};

use crate::flow_edit::fold_accents;

/// What the drawer shows under the formula.
#[derive(Clone, Debug, PartialEq)]
pub enum Preview {
    /// The formula's value with the readings as they are now. `averaged_now` is true when an
    /// input averages over a window: the preview reads its current value, not the average.
    Value { value: f64, averaged_now: bool },
    /// The preview cannot be computed here: the name of the reading it lacks (an input with no
    /// value, another card's result, `trigger`).
    Unknown(String),
    /// Why the card would be refused on save.
    Error(String),
}

/// The preview of math card `node_id` (with `params`, a draft of it) in `flow`. Errors come from
/// [`check_math_card`], the rules the hub applies on save, so the editor and the hub agree.
pub fn preview(
    node_id: &str,
    params: &MathParams,
    flow: &Flow,
    value: &dyn Fn(&str) -> Option<Scalar>,
) -> Preview {
    if let Err(e) = check_math_card(node_id, params, flow) {
        return Preview::Error(match e {
            FlowError::BadMath { reason, .. } => reason,
            other => other.to_string(),
        });
    }
    let formula = match parse_formula(&params.formula) {
        Ok(f) => f,
        Err(e) => return Preview::Error(e.reason),
    };
    let lacking = RefCell::new(None::<String>);
    let read =
        |name: &str| -> Result<f64, String> {
            let number = params.inputs.iter().find(|i| i.name == name).and_then(|i| {
                match value(&i.entity) {
                    Some(Scalar::Number(n)) => Some(n),
                    Some(Scalar::Bool(b)) => Some(f64::from(u8::from(b))),
                    _ => None,
                }
            });
            number.ok_or_else(|| {
                // The reason is dropped: a lacking reading shows as `Unknown(name)`.
                lacking.borrow_mut().get_or_insert_with(|| name.to_owned());
                name.to_owned()
            })
        };
    let result = formula.eval(&read);
    if let Some(name) = lacking.into_inner() {
        return Preview::Unknown(name);
    }
    match result {
        Ok(value) => Preview::Value {
            value,
            averaged_now: params.inputs.iter().any(|i| i.average.is_some()),
        },
        Err(reason) => Preview::Error(reason),
    }
}

/// Whether `name` is one a new input row proposes before an entity is chosen (`input`,
/// `input2`…), so choosing the entity may replace it with a name drawn from the entity.
pub fn is_placeholder_name(name: &str) -> bool {
    name.strip_prefix("input")
        .is_some_and(|rest| rest.chars().all(|c| c.is_ascii_digit()))
}

/// The name proposed for an input reading an entity called `label`: lowercase, accents folded,
/// other characters as `_`, at most 32 characters, starting with a letter (`v_` otherwise), and
/// numbered `2`, `3`… until it is valid and not in `taken`.
pub fn input_name_for(label: &str, taken: &[&str]) -> String {
    let mut slug = String::new();
    for c in fold_accents(label).chars() {
        if c.is_ascii_alphanumeric() {
            slug.push(c);
        } else if !slug.ends_with('_') {
            slug.push('_');
        }
    }
    let slug = slug.trim_matches('_');
    let base = if slug.starts_with(|c: char| c.is_ascii_lowercase()) {
        slug.to_owned()
    } else {
        format!("v_{slug}")
    };
    let base: String = base.chars().take(32).collect();
    let free = |name: &str| is_valid_name(name) && !taken.contains(&name);
    if free(&base) {
        return base;
    }
    (2..)
        .map(|n| {
            let suffix = n.to_string();
            let keep: String = base.chars().take(32 - suffix.len()).collect();
            format!("{keep}{suffix}")
        })
        .find(|name| free(name))
        .expect("an unbounded range always yields a free name")
}

#[cfg(test)]
mod tests {
    use oikos_core::flow::{Flow, MathInput, MathParams, Node, NodeKind, Scalar};

    use super::*;

    fn input(name: &str, entity: &str, average: Option<&str>) -> MathInput {
        MathInput {
            name: name.into(),
            entity: entity.into(),
            average: average.map(str::to_owned),
        }
    }

    fn params(formula: &str, inputs: Vec<MathInput>) -> MathParams {
        MathParams {
            inputs,
            formula: formula.into(),
            name: "desired".into(),
            save_to: None,
            extra: Default::default(),
        }
    }

    /// A flow holding `params` as card `m`, and optionally a second card named `other`.
    fn flow_of(params: &MathParams, other: Option<&str>) -> Flow {
        let node = |id: &str, p: MathParams| Node {
            id: id.into(),
            name: None,
            position: None,
            kind: NodeKind::Math(p),
            extra: Default::default(),
        };
        let mut nodes = vec![node("m", params.clone())];
        if let Some(name) = other {
            nodes.push(node(
                "n",
                MathParams {
                    name: name.into(),
                    formula: "1".into(),
                    ..Default::default()
                },
            ));
        }
        Flow {
            id: "f".into(),
            name: "F".into(),
            nodes,
            ..Flow::default()
        }
    }

    fn values(name: &str) -> Option<Scalar> {
        match name {
            "sensor.grid" => Some(Scalar::Number(1330.0)),
            "sensor.heater" => Some(Scalar::Number(5.0)),
            _ => None,
        }
    }

    fn run(p: &MathParams, other: Option<&str>) -> Preview {
        preview("m", p, &flow_of(p, other), &values)
    }

    #[test]
    fn preview_computes_with_current_values() {
        let p = params(
            "clamp(heater - 100 - grid, 0, 1650)",
            vec![
                input("grid", "sensor.grid", None),
                input("heater", "sensor.heater", None),
            ],
        );
        assert_eq!(
            run(&p, None),
            Preview::Value {
                value: 0.0,
                averaged_now: false
            }
        );
    }

    #[test]
    fn preview_reports_errors_and_unknowns() {
        let grid = || input("grid", "sensor.grid", None);
        let p = params("gird + 1", vec![grid()]);
        assert_eq!(run(&p, None), Preview::Error("unknown name `gird`".into()));

        let p = params("desired2 + 1", vec![grid()]);
        assert_eq!(
            run(&p, Some("desired2")),
            Preview::Unknown("desired2".into())
        );

        let p = params("grid + 1", vec![input("grid", "sensor.missing", None)]);
        assert_eq!(run(&p, None), Preview::Unknown("grid".into()));

        let p = params("1 / 0", vec![]);
        assert!(matches!(run(&p, None), Preview::Error(_)));
    }

    #[test]
    fn preview_marks_averages() {
        let p = params("grid", vec![input("grid", "sensor.grid", Some("1m"))]);
        assert_eq!(
            run(&p, None),
            Preview::Value {
                value: 1330.0,
                averaged_now: true
            }
        );
    }

    #[test]
    fn input_names() {
        assert_eq!(input_name_for("Réseau", &[]), "reseau");
        assert_eq!(input_name_for("Chauffe eau", &[]), "chauffe_eau");
        assert_eq!(input_name_for("2nd floor", &[]), "v_2nd_floor");
        assert_eq!(input_name_for("Réseau", &["reseau"]), "reseau2");
        assert_eq!(input_name_for("Min", &[]), "min2");
        assert_eq!(input_name_for("Réseau", &["reseau", "reseau2"]), "reseau3");
        assert_eq!(input_name_for("  !! ", &[]), "v_");
        let long = input_name_for(&"a".repeat(50), &[]);
        assert_eq!(long.len(), 32);
        assert!(is_placeholder_name("input") && is_placeholder_name("input12"));
        assert!(!is_placeholder_name("inputs") && !is_placeholder_name("grid"));
    }
}
