//! The Energy screen's maths, ported rule for rule from the Android app's pure Kotlin: units,
//! meter pairing and the flow builder. No I/O; the caller maps its entities to `EnergyEntity`.

mod units;

pub use units::*;
