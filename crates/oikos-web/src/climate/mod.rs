//! The Climate screen's parts: the zone card, the settings sheet and the pure text helpers.

pub mod entity_field;
pub mod proposal;
pub mod settings_sheet;
pub mod timeline;
pub mod zone_card;

use chrono::{DateTime, Datelike, Timelike};
use chrono_tz::Tz;
use oikos_climate::doc::{Day, Decision, Mode, Rules, Setpoint, Source};
use oikos_climate::view::{DecisionRow, LearnStatus};

/// What the learner's state says, in one line.
pub fn learn_status_text(status: &LearnStatus) -> String {
    match status {
        LearnStatus::Off => "Learning is off".into(),
        LearnStatus::HistoryOff => "History is off: rules can't be learned".into(),
        LearnStatus::NotEnough { days, needed } => {
            format!("Learning: {days} of {needed} days so far")
        }
        LearnStatus::UpToDate => "Rules up to date".into(),
        LearnStatus::Proposed => "New rules to review".into(),
        LearnStatus::Rejected => "Last proposal rejected".into(),
    }
}

/// How old a reading is: "just now", "10 min ago", "2 h ago", "3 d ago".
pub fn age_text(secs: u64) -> String {
    match secs {
        0..60 => "just now".into(),
        60..3600 => format!("{} min ago", secs / 60),
        3600..86_400 => format!("{} h ago", secs / 3600),
        _ => format!("{} d ago", secs / 86_400),
    }
}

/// A temperature without a trailing ".0": `21`, `20.5`.
pub fn degrees_text(t: f64) -> String {
    let s = format!("{t:.1}");
    format!("{}°", s.strip_suffix(".0").unwrap_or(&s))
}

pub fn setpoint_text(sp: Option<Setpoint>) -> String {
    match sp {
        Some(Setpoint::Temp(t)) => degrees_text(t),
        Some(Setpoint::Off) => "Off".into(),
        None => "—".into(),
    }
}

trait ShortDay {
    fn short(self) -> &'static str;
}

impl ShortDay for Day {
    fn short(self) -> &'static str {
        match self {
            Day::Mon => "Mon",
            Day::Tue => "Tue",
            Day::Wed => "Wed",
            Day::Thu => "Thu",
            Day::Fri => "Fri",
            Day::Sat => "Sat",
            Day::Sun => "Sun",
        }
    }
}

pub fn mode_word(mode: Option<Mode>) -> &'static str {
    match mode {
        Some(Mode::Heat) => "Heat",
        Some(Mode::Cool) => "Cool",
        None => "Idle",
    }
}

pub fn source_word(source: Source) -> &'static str {
    match source {
        Source::Safety => "Safety",
        Source::Override => "Override",
        Source::Holiday => "Holiday",
        Source::Away => "Away",
        Source::Schedule => "Schedule",
        Source::Idle => "Idle",
    }
}

/// The house zone from its IANA name; UTC when the name is unknown.
pub fn parse_zone(name: &str) -> Tz {
    name.parse().unwrap_or(Tz::UTC)
}

fn local(unix: u64, tz: Tz) -> Option<DateTime<Tz>> {
    DateTime::from_timestamp(i64::try_from(unix).ok()?, 0).map(|t| t.with_timezone(&tz))
}

/// Unix seconds as "HH:MM" in `tz`.
pub fn clock_text(unix: u64, tz: Tz) -> String {
    local(unix, tz).map_or_else(
        || "—".into(),
        |t| format!("{:02}:{:02}", t.hour(), t.minute()),
    )
}

/// When a hold ends, in `tz`: "07:00" today, "tomorrow 07:00", else "Mon 07:00".
pub fn until_text(until: u64, now: u64, tz: Tz) -> String {
    let (Some(end), Some(start)) = (local(until, tz), local(now, tz)) else {
        return "—".into();
    };
    let clock = format!("{:02}:{:02}", end.hour(), end.minute());
    match (end.date_naive() - start.date_naive()).num_days() {
        0 => clock,
        1 => format!("tomorrow {clock}"),
        _ => format!("{} {clock}", Day::from_chrono(end.weekday()).short()),
    }
}

/// The weekday and minute of the day at `now` in `tz`.
pub fn week_minute(now: u64, tz: Tz) -> (Day, u16) {
    local(now, tz).map_or((Day::Mon, 0), |t| {
        (
            Day::from_chrono(t.weekday()),
            (t.hour() * 60 + t.minute()) as u16,
        )
    })
}

/// One decision as a list line: "14:05 · Schedule · Heat 21° · reason".
pub fn decision_line(d: &Decision, tz: Tz) -> String {
    format!(
        "{} · {} · {} {} · {}",
        clock_text(d.at, tz),
        source_word(d.source),
        mode_word(d.mode),
        setpoint_text(d.setpoint),
        d.reason
    )
}

/// An older history row as a list line.
pub fn row_line(r: &DecisionRow, tz: Tz) -> String {
    let setpoint = r.setpoint.map_or_else(|| "—".to_owned(), degrees_text);
    format!(
        "{} · {} · {} {} · {}",
        clock_text(r.at, tz),
        crate::format::word(&r.source),
        crate::format::word(&r.mode),
        setpoint,
        r.reason
    )
}

/// Minutes from `(today, minute)` until the schedule of `mode` next changes level: the nearest
/// block start or end within the coming week. `None` without blocks for the mode.
pub fn minutes_to_next_change(rules: &Rules, mode: Mode, today: Day, minute: u16) -> Option<u32> {
    let blocks = &rules.get(mode)?.blocks;
    let today_ix = Day::ALL.iter().position(|d| *d == today)? as u32;
    let now = u32::from(minute);
    let mut best: Option<u32> = None;
    for offset in 0..=7u32 {
        let day = Day::ALL[((today_ix + offset) % 7) as usize];
        for block in blocks.iter().filter(|b| b.days.contains(&day)) {
            for edge in [block.from.0, block.to.0] {
                let at = offset * 1440 + u32::from(edge);
                if at > now {
                    let delta = at - now;
                    best = Some(best.map_or(delta, |b| b.min(delta)));
                }
            }
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;
    use oikos_climate::doc::{Block, HhMm, ModeRules};

    #[test]
    fn learn_status_text_reads_each_state() {
        assert_eq!(
            learn_status_text(&LearnStatus::NotEnough {
                days: 6,
                needed: 10
            }),
            "Learning: 6 of 10 days so far"
        );
        assert_eq!(
            learn_status_text(&LearnStatus::HistoryOff),
            "History is off: rules can't be learned"
        );
        assert_eq!(
            learn_status_text(&LearnStatus::Proposed),
            "New rules to review"
        );
        assert_eq!(
            learn_status_text(&LearnStatus::UpToDate),
            "Rules up to date"
        );
    }

    #[test]
    fn age_text_picks_the_unit() {
        assert_eq!(age_text(45), "just now");
        assert_eq!(age_text(600), "10 min ago");
        assert_eq!(age_text(7200), "2 h ago");
        assert_eq!(age_text(3 * 86_400), "3 d ago");
    }

    fn at(y: i32, mo: u32, d: u32, h: u32, mi: u32) -> u64 {
        use chrono::TimeZone;
        chrono::Utc
            .with_ymd_and_hms(y, mo, d, h, mi, 0)
            .unwrap()
            .timestamp() as u64
    }

    #[test]
    fn clock_and_week_follow_the_house_zone() {
        let paris = parse_zone("Europe/Paris");
        // Monday 2026-01-05 23:30 UTC is Tuesday 00:30 in Paris (UTC+1).
        let now = at(2026, 1, 5, 23, 30);
        assert_eq!(clock_text(now, paris), "00:30");
        assert_eq!(clock_text(now, Tz::UTC), "23:30");
        assert_eq!(week_minute(now, paris), (Day::Tue, 30));
        assert_eq!(week_minute(now, Tz::UTC), (Day::Mon, 23 * 60 + 30));
        assert_eq!(parse_zone("nowhere"), Tz::UTC);
    }

    #[test]
    fn until_text_names_the_day_when_not_today() {
        let paris = parse_zone("Europe/Paris");
        let now = at(2026, 1, 5, 10, 0); // Monday 11:00 in Paris
        assert_eq!(until_text(at(2026, 1, 5, 12, 0), now, paris), "13:00");
        assert_eq!(
            until_text(at(2026, 1, 6, 6, 0), now, paris),
            "tomorrow 07:00"
        );
        assert_eq!(until_text(at(2026, 1, 8, 6, 0), now, paris), "Thu 07:00");
        // 23:30 UTC is already tomorrow in Paris.
        assert_eq!(
            until_text(at(2026, 1, 5, 23, 30), now, paris),
            "tomorrow 00:30"
        );
    }

    #[test]
    fn degrees_drop_a_trailing_zero() {
        assert_eq!(degrees_text(21.0), "21°");
        assert_eq!(degrees_text(20.5), "20.5°");
        assert_eq!(setpoint_text(Some(Setpoint::Off)), "Off");
    }

    fn rules(blocks: Vec<Block>) -> Rules {
        Rules {
            heat: Some(ModeRules {
                base: Setpoint::Temp(17.0),
                blocks,
            }),
            cool: None,
        }
    }

    fn block(days: &[Day], from: &str, to: &str) -> Block {
        Block {
            days: days.to_vec(),
            from: HhMm::parse(from).unwrap(),
            to: HhMm::parse(to).unwrap(),
            setpoint: Setpoint::Temp(21.0),
            evidence: None,
        }
    }

    #[test]
    fn next_change_is_the_nearest_block_edge() {
        let r = rules(vec![block(&[Day::Mon, Day::Tue], "07:00", "09:00")]);
        // Monday 06:00: the block starts in an hour.
        assert_eq!(
            minutes_to_next_change(&r, Mode::Heat, Day::Mon, 360),
            Some(60)
        );
        // Monday 08:00: it ends in an hour.
        assert_eq!(
            minutes_to_next_change(&r, Mode::Heat, Day::Mon, 480),
            Some(60)
        );
        // Monday 10:00: next is Tuesday 07:00.
        assert_eq!(
            minutes_to_next_change(&r, Mode::Heat, Day::Mon, 600),
            Some(1260)
        );
        // Wednesday: wraps to next Monday 07:00.
        assert_eq!(
            minutes_to_next_change(&r, Mode::Heat, Day::Wed, 0),
            Some(5 * 1440 + 420)
        );
        assert_eq!(minutes_to_next_change(&r, Mode::Cool, Day::Mon, 0), None);
        assert_eq!(
            minutes_to_next_change(&rules(vec![]), Mode::Heat, Day::Mon, 0),
            None
        );
    }
}
