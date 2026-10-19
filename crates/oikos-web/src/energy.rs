//! Pure presentation of the Energy tab: entity views → energy inputs, diagram geometry, labels.

use oikos_api::EntityView;
use oikos_core::model::StateValue;

/// True when the solar roles together produce at least 10 W, so the Energy tab glyph lights up.
/// `W` and `kW` only until the core energy units land (Task 8 swaps in `PowerUnits`).
pub fn any_solar_active<'a>(entities: impl Iterator<Item = &'a EntityView>) -> bool {
    let watts: f64 = entities
        .filter(|e| e.ui.energy_role.as_deref() == Some("solar"))
        .filter_map(|e| {
            match (
                &e.state.value,
                e.unit.as_deref().map(str::to_ascii_lowercase),
            ) {
                (StateValue::Number(n), Some(u)) if u == "w" => Some(*n),
                (StateValue::Number(n), Some(u)) if u == "kw" => Some(*n * 1000.0),
                _ => None,
            }
        })
        .sum();
    watts >= 10.0
}
