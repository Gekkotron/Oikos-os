//! The solar day curve as SVG path strings: forecast line and fill, the low-high band, the measured
//! area and the "now" line. Pure geometry, no UI.

use oikos_api::SolarPeriod;

/// Paths and marks of one day's curve, in a `w` x `h` pixel box.
#[derive(Clone, Debug, PartialEq)]
pub struct Curve {
    pub forecast_area: String,
    pub forecast_line: String,
    pub band: String,
    pub measured_area: String,
    pub now_x: Option<f64>,
    /// `(y px, kW)` gridlines at whole kW, at most 4.
    pub grid: Vec<(f64, f64)>,
}

fn start(p: &SolarPeriod) -> u64 {
    p.end.saturating_sub(p.minutes as u64 * 60)
}

fn producing(p: &SolarPeriod) -> bool {
    p.estimate > 0.0 || p.high > 0.0
}

/// Start of the first producing period to the end of the last one; `None` without production.
pub fn producing_span(day: &[SolarPeriod]) -> Option<(u64, u64)> {
    let first = day.iter().find(|p| producing(p))?;
    let last = day.iter().rfind(|p| producing(p))?;
    Some((start(first), last.end))
}

fn mid(p: &SolarPeriod) -> f64 {
    p.end as f64 - p.minutes as f64 * 30.0
}

fn pt(x: f64, y: f64) -> String {
    format!("{x:.1} {y:.1}")
}

fn finite(v: f64) -> f64 {
    if v.is_finite() { v } else { 0.0 }
}

/// The curve of one day's periods over `span`, with the measured points `(period end, kW)`.
pub fn day_curve(
    day: &[SolarPeriod],
    measured: &[(u64, f64)],
    span: (u64, u64),
    now: u64,
    w: f64,
    h: f64,
) -> Curve {
    let mut curve = Curve {
        forecast_area: String::new(),
        forecast_line: String::new(),
        band: String::new(),
        measured_area: String::new(),
        now_x: None,
        grid: Vec::new(),
    };
    if span.1 <= span.0 {
        return curve;
    }
    let width = (span.1 - span.0) as f64;
    let x_of = |t: f64| finite((t - span.0 as f64) / width * w).clamp(0.0, w);
    let inside: Vec<&SolarPeriod> = day
        .iter()
        .filter(|p| start(p) >= span.0 && p.end <= span.1)
        .collect();

    let measured_pts: Vec<(f64, f64)> = measured
        .iter()
        .filter_map(|&(end, kw)| {
            let p = inside.iter().find(|p| p.end == end)?;
            Some((x_of(mid(p)), kw.max(0.0)))
        })
        .collect();

    let top = inside
        .iter()
        .flat_map(|p| [p.high, p.estimate])
        .chain(measured_pts.iter().map(|m| m.1))
        .fold(0.0_f64, f64::max);
    let max_kw = if top > 0.0 { top * 1.1 } else { 1.0 };
    let y_of = |kw: f64| finite(h - kw / max_kw * h).clamp(0.0, h);

    if now >= span.0 && now <= span.1 {
        curve.now_x = Some(x_of(now as f64));
    }

    let whole = max_kw.floor() as u32;
    let step = whole.div_ceil(4).max(1);
    curve.grid = (1..=whole)
        .step_by(step as usize)
        .take(4)
        .map(|k| (y_of(k as f64), k as f64))
        .collect();

    if inside.is_empty() {
        return curve;
    }

    let line: Vec<String> = inside
        .iter()
        .map(|p| pt(x_of(mid(p)), y_of(p.estimate)))
        .collect();
    curve.forecast_line = format!("M {}", line.join(" L "));
    curve.forecast_area = format!(
        "M {} L {} L {} Z",
        pt(x_of(mid(inside[0])), h),
        line.join(" L "),
        pt(x_of(mid(inside[inside.len() - 1])), h)
    );
    let up: Vec<String> = inside
        .iter()
        .map(|p| pt(x_of(mid(p)), y_of(p.high)))
        .collect();
    let down: Vec<String> = inside
        .iter()
        .rev()
        .map(|p| pt(x_of(mid(p)), y_of(p.low)))
        .collect();
    curve.band = format!("M {} L {} Z", up.join(" L "), down.join(" L "));

    if let (Some(first), Some(last)) = (measured_pts.first(), measured_pts.last()) {
        let pts: Vec<String> = measured_pts.iter().map(|m| pt(m.0, y_of(m.1))).collect();
        curve.measured_area = format!(
            "M {} L {} L {} Z",
            pt(first.0, h),
            pts.join(" L "),
            pt(last.0, h)
        );
    }
    curve
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(end: u64, estimate: f64, low: f64, high: f64) -> SolarPeriod {
        SolarPeriod {
            end,
            minutes: 30,
            estimate,
            low,
            high,
        }
    }

    fn clean(c: &Curve) {
        for s in [
            &c.forecast_area,
            &c.forecast_line,
            &c.band,
            &c.measured_area,
        ] {
            assert!(!s.contains("NaN") && !s.contains("inf"), "{s}");
        }
    }

    #[test]
    fn span_skips_dark_periods() {
        let day = vec![
            p(1800, 0.0, 0.0, 0.0),
            p(3600, 0.5, 0.2, 0.8),
            p(5400, 1.0, 0.5, 1.5),
            p(7200, 0.0, 0.0, 0.3),
            p(9000, 0.0, 0.0, 0.0),
        ];
        assert_eq!(producing_span(&day), Some((1800, 7200)));
    }

    #[test]
    fn span_none_without_production() {
        assert_eq!(producing_span(&[p(1800, 0.0, 0.0, 0.0)]), None);
        assert_eq!(producing_span(&[]), None);
    }

    #[test]
    fn points_sit_at_period_midpoints_and_scale_to_max() {
        let day = vec![p(1800, 1.0, 0.5, 2.0), p(3600, 2.0, 1.0, 4.0)];
        let c = day_curve(&day, &[], (0, 3600), 0, 100.0, 40.0);
        assert!(c.forecast_line.starts_with("M 25"), "{}", c.forecast_line);
        // max 4.4 kW: the 4.0 high sits at 40 - 40 * 4 / 4.4.
        assert!(c.band.contains("75.0 3.6"), "{}", c.band);
        for s in [&c.forecast_area, &c.forecast_line, &c.band] {
            for pair in s
                .replace(['M', 'L', 'Z'], " ")
                .split_whitespace()
                .collect::<Vec<_>>()
                .chunks(2)
            {
                let y: f64 = pair[1].parse().unwrap();
                assert!((0.0..=40.0).contains(&y), "{s}");
            }
        }
        assert!(c.grid.len() <= 4 && c.grid.iter().all(|g| g.1.fract() == 0.0));
        clean(&c);
    }

    #[test]
    fn measured_area_stops_at_last_measured_point() {
        let day = vec![p(1800, 1.0, 0.5, 2.0), p(3600, 2.0, 1.0, 4.0)];
        let c = day_curve(&day, &[(1800, 1.0)], (0, 3600), 0, 100.0, 40.0);
        assert!(
            c.measured_area.starts_with("M 25.0 40.0"),
            "{}",
            c.measured_area
        );
        assert!(
            c.measured_area.ends_with("25.0 40.0 Z"),
            "{}",
            c.measured_area
        );
        assert_eq!(c.measured_area.matches("25.0").count(), 3);
        assert!(!c.measured_area.contains("75.0"));
        let none = day_curve(&day, &[], (0, 3600), 0, 100.0, 40.0);
        assert_eq!(none.measured_area, "");
    }

    #[test]
    fn now_line_inside_span() {
        let day = vec![p(1800, 1.0, 0.5, 2.0), p(3600, 2.0, 1.0, 4.0)];
        let c = day_curve(&day, &[], (0, 3600), 1800, 100.0, 40.0);
        assert_eq!(c.now_x, Some(50.0));
    }

    #[test]
    fn no_now_line_after_last_producing_period() {
        let day = vec![p(1800, 1.0, 0.5, 2.0), p(3600, 2.0, 1.0, 4.0)];
        assert_eq!(
            day_curve(&day, &[], (0, 3600), 4000, 100.0, 40.0).now_x,
            None
        );
        assert_eq!(
            day_curve(&day, &[], (1000, 3600), 500, 100.0, 40.0).now_x,
            None
        );
    }

    #[test]
    fn empty_curve_is_safe() {
        let c = day_curve(&[], &[], (0, 0), 0, 100.0, 40.0);
        assert_eq!(c.forecast_area, "");
        assert_eq!(c.forecast_line, "");
        assert_eq!(c.band, "");
        assert_eq!(c.measured_area, "");
        assert_eq!(c.now_x, None);
        clean(&c);
        let zeros = vec![p(1800, 0.0, 0.0, 0.0)];
        let c = day_curve(&zeros, &[(1800, 0.0)], (0, 1800), 900, 100.0, 40.0);
        clean(&c);
    }
}
