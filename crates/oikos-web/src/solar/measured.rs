//! What the house measured of today's production, shared with the solar card and page.

use leptos::prelude::*;

/// Measured production today, provided as context by whoever loads it.
#[derive(Clone, Copy)]
pub struct SolarMeasured {
    /// kWh since local midnight; `None` while unknown (never a made-up zero).
    pub today_kwh: RwSignal<Option<f64>>,
    /// `(period end, kW)` of the measured curve.
    pub periods: RwSignal<Vec<(u64, f64)>>,
}
