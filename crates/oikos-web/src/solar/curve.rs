//! The solar day curve as SVG path strings: forecast line and fill, the low-high band, the measured
//! line and area, the "now" line, the gridlines and the hour ticks. Pure geometry, no UI.

use chrono::{DateTime, Timelike, Utc};
use chrono_tz::Tz;
use oikos_api::SolarPeriod;

/// Paths and marks of one day's curve, in a `w` x `h` pixel box.
#[derive(Clone, Debug, PartialEq)]
pub struct Curve {
    pub forecast_area: String,
    pub forecast_line: String,
    pub band: String,
    /// The measured points as an open line, `M … L …`.
    pub measured_line: String,
    /// The area between the measured line and the bottom of the box.
    pub measured_area: String,
    pub now_x: Option<f64>,
    /// `y px` of the highest plotted value: where the "now" line starts.
    pub now_top: f64,
    /// `(y px, kW)` gridlines at round kW steps, at most 4.
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

/// The kW step between gridlines: the smallest round step that draws at most 4 lines strictly
/// below `max_kw`; 0.25 kW only when no larger step draws any line.
fn grid_step(max_kw: f64) -> f64 {
    let lines = |step: f64| {
        (1..=4)
            .take_while(|k| f64::from(*k) * step < max_kw)
            .count()
    };
    [0.5, 1.0, 2.0, 5.0, 10.0]
        .into_iter()
        .find(|s| (max_kw / s).ceil() <= 5.0 && lines(*s) > 0)
        .or_else(|| (lines(0.25) > 0).then_some(0.25))
        .unwrap_or(10.0)
}

/// `(x px, local hour)` of every house-local whole hour that is a multiple of 3 strictly inside
/// `span`, in a box `w` px wide. Real instants are walked, so a DST change moves no label.
pub fn hour_ticks(span: (u64, u64), zone: Tz, w: f64) -> Vec<(f64, u32)> {
    if span.1 <= span.0 {
        return Vec::new();
    }
    let width = (span.1 - span.0) as f64;
    let mut ticks = Vec::new();
    let mut t = (span.0 / 1800 + 1) * 1800;
    while t < span.1 && ticks.len() < 8 {
        if let Some(utc) = DateTime::<Utc>::from_timestamp(t as i64, 0) {
            let local = utc.with_timezone(&zone);
            if local.minute() == 0 && local.hour() % 3 == 0 {
                ticks.push((finite((t - span.0) as f64 / width * w), local.hour()));
            }
        }
        t += 1800;
    }
    ticks
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
        measured_line: String::new(),
        measured_area: String::new(),
        now_x: None,
        now_top: h,
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
            if start(p) >= now {
                return None;
            }
            // The period in progress is a few minutes' average: it ends at "now", not at its middle.
            let at = if p.end > now { now as f64 } else { mid(p) };
            Some((x_of(at), kw.max(0.0)))
        })
        .collect();

    let top = inside
        .iter()
        .flat_map(|p| [p.high, p.estimate])
        .chain(measured_pts.iter().map(|m| m.1))
        .fold(0.0_f64, f64::max);
    let max_kw = if top > 0.0 { top * 1.1 } else { 1.0 };
    let y_of = |kw: f64| finite(h - kw / max_kw * h).clamp(0.0, h);
    curve.now_top = y_of(top);

    if now >= span.0 && now <= span.1 {
        curve.now_x = Some(x_of(now as f64));
    }

    let step = grid_step(max_kw);
    curve.grid = (1..=4)
        .map(|k| f64::from(k) * step)
        .take_while(|kw| *kw < max_kw)
        .map(|kw| (y_of(kw), kw))
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
        curve.measured_line = format!("M {}", pts.join(" L "));
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
            &c.measured_line,
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
        assert!(c.grid.len() <= 4);
        clean(&c);
    }

    #[test]
    fn measured_area_stops_at_last_measured_point() {
        let day = vec![p(1800, 1.0, 0.5, 2.0), p(3600, 2.0, 1.0, 4.0)];
        let c = day_curve(&day, &[(1800, 1.0)], (0, 3600), 2000, 100.0, 40.0);
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
        assert_eq!(c.measured_line, "M 25.0 30.9");
        let none = day_curve(&day, &[], (0, 3600), 0, 100.0, 40.0);
        assert_eq!(none.measured_area, "");
        assert_eq!(none.measured_line, "");
    }

    #[test]
    fn period_in_progress_ends_at_now() {
        let day = vec![p(1800, 1.0, 0.5, 2.0), p(3600, 2.0, 1.0, 4.0)];
        let measured = [(1800, 1.0), (3600, 1.5)];
        let c = day_curve(&day, &measured, (0, 3600), 2700, 100.0, 40.0);
        // First period is over (midpoint 25), the second is in progress and ends at now (75).
        assert!(c.measured_line.starts_with("M 25.0"), "{}", c.measured_line);
        assert!(c.measured_line.contains("L 75.0"), "{}", c.measured_line);
        assert_eq!(c.now_x, Some(75.0));
        assert!(
            c.measured_area.ends_with("75.0 40.0 Z"),
            "{}",
            c.measured_area
        );
        // A period starting at or after now has no point.
        let c = day_curve(&day, &measured, (0, 3600), 1800, 100.0, 40.0);
        assert!(!c.measured_line.contains("75.0"), "{}", c.measured_line);
        assert!(!c.measured_line.contains("L"), "{}", c.measured_line);
        clean(&c);
    }

    #[test]
    fn now_line_starts_at_the_highest_value() {
        let day = vec![p(1800, 1.0, 0.5, 2.0), p(3600, 2.0, 1.0, 4.0)];
        let c = day_curve(&day, &[], (0, 3600), 1800, 100.0, 40.0);
        // max 4.4 kW: the 4.0 high sits at 40 - 40 * 4 / 4.4.
        assert!((c.now_top - 3.6364).abs() < 0.01, "{}", c.now_top);
        let c = day_curve(&day, &[(1800, 6.0)], (0, 3600), 3000, 100.0, 40.0);
        assert!(
            (c.now_top - 40.0 * (1.0 - 6.0 / 6.6)).abs() < 0.01,
            "{}",
            c.now_top
        );
    }

    fn grid_of(peak: f64) -> Vec<f64> {
        let day = vec![
            p(1800, peak / 2.0, 0.0, peak / 2.0),
            p(3600, peak, 0.0, peak),
        ];
        let c = day_curve(&day, &[], (0, 3600), 0, 100.0, 40.0);
        c.grid.iter().map(|g| g.1).collect()
    }

    #[test]
    fn grid_steps_fit_the_day() {
        // Weak day, max about 1.2 kW: 0.5 and 1.0.
        assert_eq!(grid_of(1.1), vec![0.5, 1.0]);
        // Strong day, max about 6.6 kW: at most 4 lines.
        let strong = grid_of(6.0);
        assert!(strong.len() >= 2 && strong.len() <= 4, "{strong:?}");
        assert_eq!(strong, vec![2.0, 4.0, 6.0]);
        // Very weak day: finer steps rather than no line.
        assert_eq!(grid_of(0.3), vec![0.25]);
        for peak in [0.3, 0.9, 2.0, 3.5, 5.0, 9.0, 15.0] {
            let g = grid_of(peak);
            assert!(!g.is_empty() && g.len() <= 4, "{peak}: {g:?}");
            assert!(g.iter().all(|kw| *kw < peak * 1.1));
        }
    }

    fn at(zone: Tz, y: i32, m: u32, d: u32, h: u32) -> u64 {
        use chrono::TimeZone;
        zone.with_ymd_and_hms(y, m, d, h, 0, 0).unwrap().timestamp() as u64
    }

    #[test]
    fn hour_ticks_are_multiples_of_three_inside_the_span() {
        let zone: Tz = "Europe/Paris".parse().unwrap();
        let span = (at(zone, 2026, 10, 24, 8), at(zone, 2026, 10, 24, 19));
        let ticks = hour_ticks(span, zone, 600.0);
        let hours: Vec<u32> = ticks.iter().map(|t| t.1).collect();
        assert_eq!(hours, vec![9, 12, 15, 18]);
        assert!((ticks[0].0 - 600.0 / 11.0).abs() < 0.01, "{ticks:?}");
        // A tick on the very edge of the span is not strictly inside.
        let edge = (at(zone, 2026, 10, 24, 9), at(zone, 2026, 10, 24, 18));
        let hours: Vec<u32> = hour_ticks(edge, zone, 600.0).iter().map(|t| t.1).collect();
        assert_eq!(hours, vec![12, 15]);
        assert!(hour_ticks((10, 10), zone, 600.0).is_empty());
    }

    #[test]
    fn hour_ticks_follow_local_time_across_dst() {
        let zone: Tz = "Europe/Paris".parse().unwrap();
        // 2026-10-25: clocks go back at 03:00; the day is 25 hours long.
        let span = (at(zone, 2026, 10, 25, 0), at(zone, 2026, 10, 26, 0));
        let hours: Vec<u32> = hour_ticks(span, zone, 600.0).iter().map(|t| t.1).collect();
        assert_eq!(hours, vec![3, 6, 9, 12, 15, 18, 21]);
        let spring = (at(zone, 2026, 3, 29, 6), at(zone, 2026, 3, 29, 20));
        let hours: Vec<u32> = hour_ticks(spring, zone, 600.0)
            .iter()
            .map(|t| t.1)
            .collect();
        assert_eq!(hours, vec![9, 12, 15, 18]);
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
