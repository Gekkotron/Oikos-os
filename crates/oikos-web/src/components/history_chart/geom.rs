//! SVG viewBox math for the history chart: auto-scale, step-line path, daily bars.

/// A y-axis extent (data space).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct YScale {
    pub min: f64,
    pub max: f64,
}

/// SVG path strings for the step-line chart.
#[derive(Clone, Debug, PartialEq)]
pub struct StepPath {
    /// `d` attribute for the mean step line.
    pub d: String,
    /// `d` attribute for the min/max shaded band (closed polygon).
    pub band_d: String,
}

/// One daily bar in viewBox coordinates, with its data for the tooltip.
#[derive(Clone, Debug, PartialEq)]
pub struct Bar {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    pub t: i64,
    pub kwh: f64,
}

/// Auto-scale from the mean column with 10% headroom top and bottom. Falls back to `0..1` on
/// empty input, and expands to a 1-unit range when every value is the same (so a flat line
/// doesn't collapse against a zero-height viewBox).
pub fn y_scale(points: &[(i64, f64, f64, f64)]) -> YScale {
    if points.is_empty() {
        return YScale { min: 0.0, max: 1.0 };
    }
    let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
    for p in points {
        if p.1 < lo {
            lo = p.1;
        }
        if p.1 > hi {
            hi = p.1;
        }
    }
    let range = hi - lo;
    if range == 0.0 {
        return YScale {
            min: lo - 0.5,
            max: hi + 0.5,
        };
    }
    let headroom = range * 0.1;
    YScale {
        min: lo - headroom,
        max: hi + headroom,
    }
}

/// Build the step-line path and its min/max band in viewBox coordinates, with x spanning the
/// window `t_start..t_end` (bucket starts before it clamp to the left edge). Each value is held
/// until the next point, and the last one until `t_end`. Empty input yields empty strings.
pub fn step_path(
    points: &[(i64, f64, f64, f64)],
    scale: YScale,
    t_start: i64,
    t_end: i64,
    view_w: f64,
    view_h: f64,
) -> StepPath {
    if points.is_empty() {
        return StepPath {
            d: String::new(),
            band_d: String::new(),
        };
    }
    let t_span = (t_end - t_start).max(1) as f64;
    let y_span = (scale.max - scale.min).max(f64::EPSILON);
    let x_of = |t: i64| ((t - t_start) as f64 / t_span).clamp(0.0, 1.0) * view_w;
    let y_of = |v: f64| view_h - (v - scale.min) / y_span * view_h;
    let x_end = view_w;

    let (x0, y0) = (x_of(points[0].0), y_of(points[0].1));
    let mut d = format!("M{x0},{y0}");
    for p in &points[1..] {
        let x = x_of(p.0);
        let y = y_of(p.1);
        d.push('H');
        d.push_str(&format!("{x}"));
        d.push('V');
        d.push_str(&format!("{y}"));
    }
    d.push_str(&format!("H{x_end}"));

    // Band: step forward along max values to the window end, then step back along min values.
    let mut band = String::new();
    let ymax0 = y_of(points[0].3);
    band.push_str(&format!("M{x0},{ymax0}"));
    for p in &points[1..] {
        let x = x_of(p.0);
        let y_max = y_of(p.3);
        band.push('H');
        band.push_str(&format!("{x}"));
        band.push('V');
        band.push_str(&format!("{y_max}"));
    }
    let y_min_last = y_of(points[points.len() - 1].2);
    band.push_str(&format!("H{x_end}V{y_min_last}"));
    for i in (1..points.len()).rev() {
        let x = x_of(points[i].0);
        let y_min_prev = y_of(points[i - 1].2);
        band.push('H');
        band.push_str(&format!("{x}"));
        band.push('V');
        band.push_str(&format!("{y_min_prev}"));
    }
    band.push_str(&format!("H{x0}"));
    band.push('Z');

    StepPath { d, band_d: band }
}

/// Lay bars evenly across `view_w`, scaling heights to the maximum kWh. Each slot carries one
/// bar at 80% of the slot width. Returns an empty `Vec` on empty input.
pub fn daily_bars(days: &[(i64, f64)], view_w: f64, view_h: f64) -> Vec<Bar> {
    if days.is_empty() {
        return Vec::new();
    }
    let slot = view_w / days.len() as f64;
    let max = days
        .iter()
        .map(|d| d.1)
        .fold(0.0f64, f64::max)
        .max(f64::EPSILON);
    let bar_w = slot * 0.8;
    days.iter()
        .enumerate()
        .map(|(i, &(t, kwh))| {
            let h = (kwh / max) * view_h;
            Bar {
                x: i as f64 * slot,
                y: view_h - h,
                w: bar_w,
                h,
                t,
                kwh,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn y_scale_adds_ten_percent_headroom() {
        let p = vec![(0, 20.0, 20.0, 20.0), (1, 30.0, 30.0, 30.0)];
        let s = y_scale(&p);
        assert!((s.min - 19.0).abs() < 1e-6);
        assert!((s.max - 31.0).abs() < 1e-6);
    }

    #[test]
    fn y_scale_falls_back_to_unit_range_on_empty() {
        let s = y_scale(&[]);
        assert_eq!((s.min, s.max), (0.0, 1.0));
    }

    #[test]
    fn y_scale_expands_a_flat_series() {
        let p = vec![(0, 5.0, 5.0, 5.0), (1, 5.0, 5.0, 5.0)];
        let s = y_scale(&p);
        assert!((s.min - 4.5).abs() < 1e-6);
        assert!((s.max - 5.5).abs() < 1e-6);
    }

    #[test]
    fn step_path_produces_horizontal_then_vertical_segments() {
        let p = vec![(0, 0.0, 0.0, 0.0), (10, 1.0, 1.0, 1.0)];
        let s = YScale { min: 0.0, max: 1.0 };
        let out = step_path(&p, s, 0, 10, 100.0, 50.0);
        assert!(out.d.contains("M0,50"));
        assert!(out.d.contains("H") && out.d.contains("V"));
    }

    #[test]
    fn step_path_holds_a_single_point_until_the_window_end() {
        let p = vec![(50, 0.5, 0.0, 1.0)];
        let s = YScale { min: 0.0, max: 1.0 };
        let out = step_path(&p, s, 0, 100, 100.0, 50.0);
        assert_eq!(out.d, "M50,25H100");
        assert_eq!(out.band_d, "M50,0H100V50H50Z");
    }

    #[test]
    fn step_path_places_short_history_at_the_right_of_the_window() {
        let p = vec![(80, 0.0, 0.0, 0.0), (90, 1.0, 1.0, 1.0)];
        let s = YScale { min: 0.0, max: 1.0 };
        let out = step_path(&p, s, 0, 100, 100.0, 50.0);
        assert_eq!(out.d, "M80,50H90V0H100");
    }

    #[test]
    fn step_path_clamps_a_bucket_starting_before_the_window() {
        let p = vec![(-20, 0.0, 0.0, 0.0)];
        let s = YScale { min: 0.0, max: 1.0 };
        let out = step_path(&p, s, 0, 100, 100.0, 50.0);
        assert_eq!(out.d, "M0,50H100");
    }

    #[test]
    fn step_path_band_steps_back_along_min_without_diagonals() {
        let p = vec![(0, 0.5, 0.0, 1.0), (50, 0.5, 0.25, 0.75)];
        let s = YScale { min: 0.0, max: 1.0 };
        let out = step_path(&p, s, 0, 100, 100.0, 100.0);
        assert!(!out.band_d.contains('L'));
        assert_eq!(out.band_d, "M0,0H50V25H100V75H50V100H0Z");
    }

    #[test]
    fn step_path_empty_input_is_empty() {
        let out = step_path(&[], YScale { min: 0.0, max: 1.0 }, 0, 100, 100.0, 50.0);
        assert_eq!(out.d, "");
        assert_eq!(out.band_d, "");
    }

    #[test]
    fn daily_bars_lay_out_evenly_and_scale_to_max() {
        let days = vec![(0, 2.0), (86_400, 4.0), (172_800, 1.0)];
        let bars = daily_bars(&days, 300.0, 100.0);
        assert_eq!(bars.len(), 3);
        let w_each = 100.0;
        assert!((bars[0].x - 0.0).abs() < 1e-6);
        assert!((bars[1].x - 100.0).abs() < 1e-6);
        assert!((bars[1].h - 100.0).abs() < 1e-6);
        assert!((bars[0].h - 50.0).abs() < 1e-6);
        assert!((bars[0].w - w_each * 0.8).abs() < w_each * 0.21);
    }

    #[test]
    fn daily_bars_empty_input_is_empty() {
        assert!(daily_bars(&[], 300.0, 100.0).is_empty());
    }
}
