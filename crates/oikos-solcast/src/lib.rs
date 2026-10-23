//! Solcast rooftop-site forecasts: parsing, day figures, the call budget and a small client.

#![forbid(unsafe_code)]

pub mod forecast;
pub mod period;

pub use period::{ParseError, Period, parse_forecasts};
