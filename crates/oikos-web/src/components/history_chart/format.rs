//! Tick labels, tooltip time strings, and state-axis labels for the history chart.
//!
//! Times are rendered in the house timezone (Europe/Paris). DST is computed by hand (last Sunday
//! of March at 01:00 UTC → +02:00; last Sunday of October at 01:00 UTC → +01:00), so no new
//! dependency is pulled into the wasm bundle.

use oikos_api::HistoryRange;
use time::{Date, Month, OffsetDateTime, PrimitiveDateTime, Time, UtcOffset, Weekday};

/// Europe/Paris UTC offset in seconds at `unix` time.
pub fn paris_offset(unix: i64) -> i32 {
    let Ok(dt) = OffsetDateTime::from_unix_timestamp(unix) else {
        return 3600;
    };
    let year = dt.year();
    let dst_start = last_sunday_01_utc(year, Month::March);
    let dst_end = last_sunday_01_utc(year, Month::October);
    if (dst_start..dst_end).contains(&unix) {
        7200
    } else {
        3600
    }
}

fn last_sunday_01_utc(year: i32, month: Month) -> i64 {
    let last = month.length(year);
    let mut d = Date::from_calendar_date(year, month, last).unwrap();
    while d.weekday() != Weekday::Sunday {
        d = d.previous_day().unwrap();
    }
    PrimitiveDateTime::new(d, Time::from_hms(1, 0, 0).unwrap())
        .assume_utc()
        .unix_timestamp()
}

/// `unix` as an `OffsetDateTime` in the house zone.
fn local(unix: i64) -> OffsetDateTime {
    let off = paris_offset(unix);
    let offset = UtcOffset::from_whole_seconds(off).unwrap_or(UtcOffset::UTC);
    OffsetDateTime::from_unix_timestamp(unix)
        .unwrap_or(OffsetDateTime::UNIX_EPOCH)
        .to_offset(offset)
}

/// X-axis tick labels for a range ending at `now_secs`: a list of `(position in 0..=1, label)`.
/// `now` is always the final label at position `1.0`. On the day and month axes, whose ticks
/// are instants, a tick closer to `now` than [`NOW_GAP`] is dropped so the two labels do not run
/// into each other; week and year labels name whole slots and are kept.
pub fn tick_labels(range: HistoryRange, now_secs: i64) -> Vec<(f64, String)> {
    let mut out = match range {
        HistoryRange::Day => day_ticks(now_secs),
        HistoryRange::Week => return week_ticks(now_secs),
        HistoryRange::Month => month_ticks(now_secs),
        HistoryRange::Year => return year_ticks(now_secs),
    };
    let last = out.len().saturating_sub(1);
    let mut i = 0;
    out.retain(|(pos, _)| {
        let keep = i == last || *pos <= 1.0 - NOW_GAP;
        i += 1;
        keep
    });
    out
}

/// The smallest gap, as a fraction of the axis, between a tick and the final `now` label.
const NOW_GAP: f64 = 0.12;

fn day_ticks(now_secs: i64) -> Vec<(f64, String)> {
    let start = now_secs - 86_400;
    let mut out: Vec<(f64, String)> = Vec::new();
    // Each candidate wall-clock hour at 00/06/12/18 in the window. Compute the offset at each
    // candidate's own local date so DST transitions don't misplace a candidate by one hour
    // (reviewer finding #9).
    for h in [0u8, 6, 12, 18] {
        for days_back in 0..=1i64 {
            let probe = now_secs - days_back * 86_400;
            let dt = local(probe);
            let offset =
                UtcOffset::from_whole_seconds(paris_offset(probe)).unwrap_or(UtcOffset::UTC);
            let t = PrimitiveDateTime::new(dt.date(), Time::from_hms(h, 0, 0).unwrap())
                .assume_offset(offset)
                .unix_timestamp();
            if (start..now_secs).contains(&t) {
                let pos = (t - start) as f64 / 86_400.0;
                let label = format!("{h:02}:00");
                if !out.iter().any(|(_, l)| *l == label) {
                    out.push((pos, label));
                }
            }
        }
    }
    out.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    out.push((1.0, "now".into()));
    out
}

fn week_ticks(now_secs: i64) -> Vec<(f64, String)> {
    // One label per day in the 7-day window. The last tick is "now".
    let mut out = Vec::new();
    for i in 0..7 {
        let t = now_secs - (6 - i) as i64 * 86_400;
        let pos = i as f64 / 6.0;
        let dt = local(t);
        let name = short_weekday(dt.weekday());
        if i == 6 {
            out.push((1.0, "now".into()));
        } else {
            out.push((pos, name.to_owned()));
        }
    }
    out
}

fn month_ticks(now_secs: i64) -> Vec<(f64, String)> {
    // Fixed day-of-month labels within the 30-day window.
    let start = now_secs - 30 * 86_400;
    let mut out = Vec::new();
    for day in [1u8, 8, 15, 22] {
        // Try the two most recent occurrences of this day-of-month.
        for back in 0..=1 {
            let probe = now_secs - back * 30 * 86_400 / 2;
            let dt = local(probe);
            let year = dt.year();
            let month = dt.month();
            if let Ok(d) = Date::from_calendar_date(year, month, day) {
                let t = PrimitiveDateTime::new(d, Time::MIDNIGHT)
                    .assume_offset(dt.offset())
                    .unix_timestamp();
                if (start..now_secs).contains(&t) {
                    let pos = (t - start) as f64 / (30.0 * 86_400.0);
                    out.push((pos, format!("{day:02}")));
                    break;
                }
            }
        }
    }
    out.push((1.0, "now".into()));
    out
}

fn year_ticks(now_secs: i64) -> Vec<(f64, String)> {
    // Twelve month labels ending at the month that contains `now`, walking back one month per
    // slot. Earlier code labelled every slot Jan..Dec regardless of the window, so October's
    // data was read as if it were January (reviewer finding #7).
    let end_month = local(now_secs).month() as i32;
    let mut out = Vec::new();
    for i in 0..12i32 {
        let m_num = ((end_month - 12 + i).rem_euclid(12) + 1) as u8;
        let month = Month::try_from(m_num).unwrap_or(Month::January);
        let pos = (i as f64 + 0.5) / 12.0;
        out.push((pos, short_month(month).to_owned()));
    }
    out.push((1.0, "now".into()));
    out
}

fn short_weekday(w: Weekday) -> &'static str {
    match w {
        Weekday::Monday => "Mon",
        Weekday::Tuesday => "Tue",
        Weekday::Wednesday => "Wed",
        Weekday::Thursday => "Thu",
        Weekday::Friday => "Fri",
        Weekday::Saturday => "Sat",
        Weekday::Sunday => "Sun",
    }
}

fn short_month(m: Month) -> &'static str {
    match m {
        Month::January => "Jan",
        Month::February => "Feb",
        Month::March => "Mar",
        Month::April => "Apr",
        Month::May => "May",
        Month::June => "Jun",
        Month::July => "Jul",
        Month::August => "Aug",
        Month::September => "Sep",
        Month::October => "Oct",
        Month::November => "Nov",
        Month::December => "Dec",
    }
}

/// Local time for a tooltip. Relative on 24h ("14:20"), absolute from 7d up ("Tue 06 Oct 14:20"
/// through 30d, "Oct 2026" for 1y).
pub fn tooltip_time(range: HistoryRange, t_secs: i64, _now_secs: i64) -> String {
    let dt = local(t_secs);
    match range {
        HistoryRange::Day => format!("{:02}:{:02}", dt.hour(), dt.minute()),
        HistoryRange::Week | HistoryRange::Month => format!(
            "{} {:02} {} {:02}:{:02}",
            short_weekday(dt.weekday()),
            dt.day(),
            short_month(dt.month()),
            dt.hour(),
            dt.minute(),
        ),
        HistoryRange::Year => format!("{} {}", short_month(dt.month()), dt.year()),
    }
}

/// For discrete-kind entities, the Y-axis label pair that stands in for numeric min/max. `None`
/// when the entity is a numeric sensor and should keep its numeric labels.
///
/// `kind` is the debug-rendered variant of `oikos_core::model::Kind` (e.g. `"OnOff"`,
/// `"Light"`, `"BinarySensor"`, `"Cover"`). Case-sensitive.
pub fn state_axis_labels(_unit: Option<&str>, kind: &str) -> Option<(String, String)> {
    match kind {
        "OnOff" | "Light" | "Switch" | "BinarySensor" => Some(("off".into(), "on".into())),
        "Cover" => Some(("0".into(), "100".into())),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oikos_api::HistoryRange;

    #[test]
    fn paris_offset_is_two_hours_in_summer_one_in_winter() {
        // 2026-06-01 12:00 UTC → CEST (+02:00)
        let summer = OffsetDateTime::from_unix_timestamp(1_780_660_800).unwrap();
        assert_eq!(paris_offset(summer.unix_timestamp()), 7200);
        // 2026-12-01 12:00 UTC → CET (+01:00)
        let winter = OffsetDateTime::from_unix_timestamp(1_796_558_400).unwrap();
        assert_eq!(paris_offset(winter.unix_timestamp()), 3600);
    }

    #[test]
    fn a_tick_next_to_now_is_dropped() {
        // Every quarter hour of a day, so some "now" falls just after a 6-hourly tick.
        for step in 0..96 {
            let now = 1_792_497_600 + step * 900;
            let out = tick_labels(HistoryRange::Day, now);
            let (last, rest) = out.split_last().unwrap();
            assert_eq!(last.1, "now");
            assert!(rest.iter().all(|(p, _)| *p <= 1.0 - NOW_GAP), "{out:?}");
        }
    }

    #[test]
    fn tick_labels_for_24h_uses_6h_steps() {
        // 17:00 on 2026-10-20 Europe/Paris: every 6-hourly tick is clear of "now".
        let labels: Vec<String> = tick_labels(HistoryRange::Day, 1_792_497_600 + 3 * 3_600)
            .into_iter()
            .map(|(_, s)| s)
            .collect();
        assert!(labels.iter().any(|l| l == "00:00"));
        assert!(labels.iter().any(|l| l == "06:00"));
        assert!(labels.iter().any(|l| l == "12:00"));
        assert!(labels.iter().any(|l| l == "18:00"));
        assert_eq!(labels.last().unwrap(), "now");
    }

    #[test]
    fn tick_labels_for_7d_has_7_entries_ending_with_now() {
        let out = tick_labels(HistoryRange::Week, 1_792_497_600);
        assert_eq!(out.len(), 7);
        assert_eq!(out.last().unwrap().1, "now");
    }

    #[test]
    fn tick_labels_for_1y_end_at_current_month_not_december() {
        // 2026-10-20 12:00 UTC → October in Europe/Paris. Reviewer finding #7.
        let out = tick_labels(HistoryRange::Year, 1_792_497_600);
        // Last entry is "now", penultimate is the current month.
        assert_eq!(out.last().unwrap().1, "now");
        assert_eq!(out[out.len() - 2].1, "Oct");
        // 11 months before current month = November of the prior year.
        assert_eq!(out[0].1, "Nov");
    }

    #[test]
    fn day_ticks_still_produce_four_hour_labels_on_a_dst_day() {
        // 2026-10-26 10:00 UTC — the day after the autumn DST transition (Oct 25).
        let now = 1_792_015_200 + 86_400;
        let out = tick_labels(HistoryRange::Day, now);
        let hour_labels: Vec<&String> = out
            .iter()
            .map(|(_, s)| s)
            .filter(|s| s.ends_with(":00") && !s.starts_with(|c: char| c.is_alphabetic()))
            .collect();
        // At most one of each of 00/06/12/18 must appear (deduped) and all four should fit a
        // 24-hour window.
        assert!(hour_labels.len() >= 3 && hour_labels.len() <= 4);
        for h in ["00:00", "06:00", "12:00", "18:00"] {
            assert_eq!(hour_labels.iter().filter(|l| **l == h).count(), 1);
        }
    }

    #[test]
    fn tooltip_time_is_relative_on_24h_and_absolute_from_7d() {
        let now = 1_792_497_600;
        let t = now - 3_600;
        let d = tooltip_time(HistoryRange::Day, t, now);
        assert!(d.len() == 5 && d.chars().nth(2) == Some(':'));
        let w = tooltip_time(HistoryRange::Week, t, now);
        assert!(w.len() > 10 && w.contains(':'));
        let y = tooltip_time(HistoryRange::Year, t, now);
        // Year tooltip looks like "Oct 2026"
        assert!(y.contains("20"));
        assert!(!y.contains(':'));
    }

    #[test]
    fn state_axis_labels_pair_each_kind() {
        assert_eq!(
            state_axis_labels(None, "OnOff"),
            Some(("off".into(), "on".into()))
        );
        assert_eq!(
            state_axis_labels(None, "Light"),
            Some(("off".into(), "on".into()))
        );
        assert_eq!(
            state_axis_labels(None, "BinarySensor"),
            Some(("off".into(), "on".into()))
        );
        assert_eq!(
            state_axis_labels(None, "Cover"),
            Some(("0".into(), "100".into()))
        );
        assert_eq!(state_axis_labels(Some("°C"), "Sensor"), None);
    }
}
