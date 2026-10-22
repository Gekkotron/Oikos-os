use chrono::{Datelike, Duration, NaiveDate, NaiveDateTime, Weekday};
use oikos_climate::decide::DeviceCaps;
use oikos_climate::doc::{Block, Day, Evidence, HhMm, Mode, ModeRules, Rules, Setpoint, Source};
use oikos_climate::learn::{
    DeviceHistory, EngineCommand, LearnInput, LearnOutcome, SLOTS, Slot, TargetSample, diff,
    differs, fingerprint, learn, timeline,
};

const ENTITY: &str = "climate.room";

/// A Monday.
fn monday() -> NaiveDate {
    let d = NaiveDate::from_ymd_opt(2026, 10, 5).unwrap();
    assert_eq!(d.weekday(), Weekday::Mon);
    d
}

fn at(date: NaiveDate, hhmm: &str) -> NaiveDateTime {
    let (h, m) = hhmm.split_once(':').unwrap();
    date.and_hms_opt(h.parse().unwrap(), m.parse().unwrap(), 0)
        .unwrap()
}

/// One day of heat targets: `(time, target)` pairs.
fn day_of(date: NaiveDate, points: &[(&str, f64)]) -> Vec<TargetSample> {
    points
        .iter()
        .map(|(t, v)| TargetSample {
            at: at(date, t),
            target: Some(*v),
            mode: Some("heat".into()),
        })
        .collect()
}

/// One day where `None` means the device is off.
fn day_with(date: NaiveDate, mode: &str, points: &[(&str, Option<f64>)]) -> Vec<TargetSample> {
    points
        .iter()
        .map(|(t, v)| TargetSample {
            at: at(date, t),
            target: *v,
            mode: Some(v.map_or("off", |_| mode).into()),
        })
        .collect()
}

fn history(days: u32, f: impl Fn(NaiveDate) -> Vec<TargetSample>) -> Vec<DeviceHistory> {
    vec![DeviceHistory {
        entity: ENTITY.into(),
        samples: (0..days)
            .flat_map(|i| f(monday() + Duration::days(i64::from(i))))
            .collect(),
    }]
}

fn caps() -> DeviceCaps {
    DeviceCaps {
        hvac_modes: vec!["heat".into(), "cool".into(), "off".into()],
        min_temp: None,
        max_temp: None,
        temp_step: Some(0.5),
    }
}

fn input<'a>(
    devices: &'a [DeviceHistory],
    engine: &'a [EngineCommand],
    caps: &'a DeviceCaps,
    days: u32,
) -> LearnInput<'a> {
    LearnInput {
        mode: Mode::Heat,
        devices,
        engine,
        approved: None,
        first_day: monday(),
        days,
        min_days: 10,
        support: 0.6,
        frost: 7.0,
        caps,
    }
}

fn is_weekday(d: NaiveDate) -> bool {
    !matches!(d.weekday(), Weekday::Sat | Weekday::Sun)
}

/// 17 °C, 20.5 °C from 07:00 to 22:30 on weekdays, flat 17 °C on weekends.
fn standard_day(d: NaiveDate) -> Vec<TargetSample> {
    if is_weekday(d) {
        day_of(d, &[("00:00", 17.0), ("07:00", 20.5), ("22:30", 17.0)])
    } else {
        day_of(d, &[("00:00", 17.0)])
    }
}

fn rules_of(outcome: LearnOutcome) -> ModeRules {
    match outcome {
        LearnOutcome::Rules(r) => r,
        other => panic!("expected rules, got {other:?}"),
    }
}

fn weekdays() -> Vec<Day> {
    vec![Day::Mon, Day::Tue, Day::Wed, Day::Thu, Day::Fri]
}

fn block(days: &[Day], from: u16, to: u16, sp: f64) -> Block {
    Block {
        days: days.to_vec(),
        from: HhMm(from),
        to: HhMm(to),
        setpoint: Setpoint::Temp(sp),
        evidence: None,
    }
}

fn weekday_rules() -> ModeRules {
    ModeRules {
        base: Setpoint::Temp(17.0),
        blocks: vec![block(&weekdays(), 420, 1350, 20.5)],
    }
}

fn heat(m: ModeRules) -> Rules {
    Rules {
        heat: Some(m),
        cool: None,
    }
}

fn strip(mut m: ModeRules) -> ModeRules {
    for b in &mut m.blocks {
        b.evidence = None;
    }
    m
}

fn cmd(date: NaiveDate, hhmm: &str, sp: f64, source: Source) -> EngineCommand {
    EngineCommand {
        entity: ENTITY.into(),
        at: at(date, hhmm),
        setpoint: Some(sp),
        mode: "heat".into(),
        source,
    }
}

#[test]
fn not_enough_days() {
    let h = history(6, standard_day);
    let c = caps();
    assert_eq!(
        learn(&input(&h, &[], &c, 6)),
        LearnOutcome::NotEnough {
            days: 6,
            needed: 10
        }
    );
}

#[test]
fn learns_weekday_block() {
    let h = history(28, standard_day);
    let c = caps();
    let rules = rules_of(learn(&input(&h, &[], &c, 28)));
    assert_eq!(rules.base, Setpoint::Temp(17.0));
    assert_eq!(rules.blocks.len(), 1);
    let b = &rules.blocks[0];
    assert_eq!(b.days, weekdays());
    assert_eq!((b.from, b.to), (HhMm(420), HhMm(1350)));
    assert_eq!(b.setpoint, Setpoint::Temp(20.5));
    assert_eq!(b.evidence, Some(Evidence { agree: 20, of: 20 }));
    assert!(heat(rules).validate().is_ok());
}

#[test]
fn noisy_day_lowers_evidence_not_block() {
    let flat = [
        NaiveDate::from_ymd_opt(2026, 10, 7).unwrap(),
        NaiveDate::from_ymd_opt(2026, 10, 14).unwrap(),
        NaiveDate::from_ymd_opt(2026, 10, 21).unwrap(),
    ];
    let h = history(28, |d| {
        if flat.contains(&d) {
            day_of(d, &[("00:00", 17.0)])
        } else {
            standard_day(d)
        }
    });
    let c = caps();
    let rules = rules_of(learn(&input(&h, &[], &c, 28)));
    assert_eq!(rules.blocks.len(), 1);
    assert_eq!(
        rules.blocks[0].evidence,
        Some(Evidence { agree: 17, of: 20 })
    );
}

#[test]
fn weak_pattern_dropped() {
    let h = history(28, |d| {
        if is_weekday(d) && d < monday() + Duration::days(14) {
            standard_day(d)
        } else {
            day_of(d, &[("00:00", 17.0)])
        }
    });
    let c = caps();
    let rules = rules_of(learn(&input(&h, &[], &c, 28)));
    assert!(rules.blocks.is_empty(), "{rules:?}");
    assert_eq!(rules.base, Setpoint::Temp(17.0));
}

#[test]
fn off_at_night_learns_off_base() {
    let h = history(14, |d| {
        day_with(
            d,
            "heat",
            &[("00:00", None), ("06:00", Some(20.0)), ("23:00", None)],
        )
    });
    let c = caps();
    let rules = rules_of(learn(&input(&h, &[], &c, 14)));
    assert_eq!(rules.base, Setpoint::Off);
    assert_eq!(rules.blocks.len(), 1);
    let b = &rules.blocks[0];
    assert_eq!(b.days.len(), 7);
    assert_eq!((b.from, b.to), (HhMm(360), HhMm(1380)));
    assert_eq!(b.setpoint, Setpoint::Temp(20.0));
    assert_eq!(b.evidence, Some(Evidence { agree: 14, of: 14 }));
    assert!(heat(rules).validate().is_ok());
}

#[test]
fn cool_mode_blocks_below_base() {
    let h = history(14, |d| {
        day_with(
            d,
            "cool",
            &[
                ("00:00", Some(28.0)),
                ("08:00", Some(24.0)),
                ("20:00", Some(28.0)),
            ],
        )
    });
    let c = caps();
    let mut inp = input(&h, &[], &c, 14);
    inp.mode = Mode::Cool;
    let rules = rules_of(learn(&inp));
    assert_eq!(rules.base, Setpoint::Temp(28.0));
    assert_eq!(rules.blocks.len(), 1);
    let b = &rules.blocks[0];
    assert_eq!(b.days.len(), 7);
    assert_eq!((b.from, b.to), (HhMm(480), HhMm(1200)));
    assert_eq!(b.setpoint, Setpoint::Temp(24.0));
}

#[test]
fn engine_schedule_slots_count_as_agreement() {
    let h = history(28, standard_day);
    let c = caps();
    let engine: Vec<EngineCommand> = (0..28)
        .map(|i| monday() + Duration::days(i))
        .flat_map(|d| {
            if is_weekday(d) {
                vec![
                    cmd(d, "07:00", 20.5, Source::Schedule),
                    cmd(d, "22:30", 17.0, Source::Schedule),
                ]
            } else {
                vec![cmd(d, "00:00", 17.0, Source::Schedule)]
            }
        })
        .collect();
    let approved = weekday_rules();
    let mut inp = input(&h, &engine, &c, 28);
    inp.approved = Some(&approved);
    let learned = rules_of(learn(&inp));
    assert_eq!(strip(learned.clone()), approved);
    assert!(!differs(&approved, &learned));
}

#[test]
fn engine_away_slots_are_no_data() {
    let h = history(28, |d| {
        if is_weekday(d) {
            day_of(
                d,
                &[
                    ("00:00", 17.0),
                    ("07:00", 20.5),
                    ("19:00", 16.0),
                    ("22:30", 17.0),
                ],
            )
        } else {
            day_of(d, &[("00:00", 17.0)])
        }
    });
    let c = caps();
    let engine: Vec<EngineCommand> = (0..28)
        .map(|i| monday() + Duration::days(i))
        .filter(|d| is_weekday(*d))
        .flat_map(|d| {
            vec![
                cmd(d, "07:00", 20.5, Source::Schedule),
                cmd(d, "19:00", 16.0, Source::Away),
                cmd(d, "22:30", 17.0, Source::Schedule),
            ]
        })
        .collect();
    let approved = weekday_rules();
    let mut inp = input(&h, &engine, &c, 28);
    inp.approved = Some(&approved);
    let tl = timeline(&inp);
    assert_eq!(tl[0].1[19 * 4], Slot::None);
    assert_eq!(tl[0].1[12 * 4], Slot::Temp(20.5));
    assert_eq!(tl[0].1[23 * 4], Slot::Temp(17.0));
    let learned = rules_of(learn(&inp));
    assert!(!differs(&approved, &learned), "{learned:?}");
}

#[test]
fn single_hand_change_does_not_change_rules() {
    let odd = NaiveDate::from_ymd_opt(2026, 10, 14).unwrap();
    let h = history(28, |d| {
        if d == odd {
            day_of(d, &[("00:00", 17.0), ("07:00", 22.0), ("22:30", 17.0)])
        } else {
            standard_day(d)
        }
    });
    let c = caps();
    let learned = rules_of(learn(&input(&h, &[], &c, 28)));
    assert!(!differs(&weekday_rules(), &learned), "{learned:?}");
}

#[test]
fn repeated_hand_changes_propose_change() {
    let early_until = monday() + Duration::days(21);
    let h = history(28, |d| {
        if is_weekday(d) && d < early_until {
            day_of(d, &[("00:00", 17.0), ("06:30", 20.5), ("22:30", 17.0)])
        } else {
            standard_day(d)
        }
    });
    let c = caps();
    let learned = rules_of(learn(&input(&h, &[], &c, 28)));
    let approved = weekday_rules();
    assert!(differs(&approved, &learned), "{learned:?}");
    let lines = diff(Some(&heat(approved)), &heat(learned));
    assert!(
        lines.iter().any(|l| l.contains("07:00 → 06:30")),
        "{lines:?}"
    );
}

#[test]
fn heat_floor_at_frost() {
    let h = history(14, |d| {
        day_of(d, &[("00:00", 5.0), ("08:00", 20.0), ("20:00", 5.0)])
    });
    let c = caps();
    let rules = rules_of(learn(&input(&h, &[], &c, 14)));
    assert_eq!(rules.base, Setpoint::Temp(7.0));
    assert_eq!(rules.blocks[0].setpoint, Setpoint::Temp(20.0));
}

#[test]
fn deterministic() {
    let h = history(28, standard_day);
    let c = caps();
    let inp = input(&h, &[], &c, 28);
    assert_eq!(learn(&inp), learn(&inp));
    assert_eq!(timeline(&inp).len(), 28);
    assert_eq!(timeline(&inp)[0].1.len(), SLOTS);
}

#[test]
fn fingerprint_ignores_evidence() {
    let a = heat(weekday_rules());
    let mut b = a.clone();
    b.heat.as_mut().unwrap().blocks[0].evidence = Some(Evidence { agree: 3, of: 4 });
    assert_eq!(fingerprint(&a), fingerprint(&b));
    assert_eq!(fingerprint(&a).len(), 16);
    assert!(
        fingerprint(&a)
            .chars()
            .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c))
    );
    b.heat.as_mut().unwrap().blocks[0].setpoint = Setpoint::Temp(21.0);
    assert_ne!(fingerprint(&a), fingerprint(&b));
}

#[test]
fn diff_lines() {
    let old = heat(ModeRules {
        base: Setpoint::Temp(17.0),
        blocks: vec![
            block(&weekdays(), 420, 1350, 20.5),
            block(&[Day::Sat], 540, 720, 21.0),
        ],
    });
    let new = heat(ModeRules {
        base: Setpoint::Temp(16.5),
        blocks: vec![
            block(&weekdays(), 390, 1350, 20.5),
            Block {
                evidence: Some(Evidence { agree: 5, of: 6 }),
                ..block(&[Day::Sat, Day::Sun], 480, 540, 19.0)
            },
            block(&[Day::Mon, Day::Wed], 0, 60, 18.0),
        ],
    });
    assert_eq!(
        diff(Some(&old), &new),
        vec![
            "Base: 17.0 °C → 16.5 °C",
            "Changed: weekdays start 07:00 → 06:30",
            "Removed: Sat 09:00–12:00 at 21.0 °C",
            "New: weekends 08:00–09:00 at 19.0 °C (5 of 6 days)",
            "New: Mon, Wed 00:00–01:00 at 18.0 °C",
        ]
    );
    assert!(diff(Some(&old), &old).is_empty());

    let first = Rules {
        heat: Some(ModeRules {
            base: Setpoint::Temp(17.0),
            blocks: vec![Block {
                evidence: Some(Evidence { agree: 17, of: 20 }),
                ..block(&weekdays(), 420, 1350, 20.5)
            }],
        }),
        cool: None,
    };
    assert_eq!(
        diff(None, &first),
        vec![
            "Base: 17.0 °C",
            "New: weekdays 07:00–22:30 at 20.5 °C (17 of 20 days)"
        ]
    );
    let all = heat(ModeRules {
        base: Setpoint::Off,
        blocks: vec![block(
            &[
                Day::Mon,
                Day::Tue,
                Day::Wed,
                Day::Thu,
                Day::Fri,
                Day::Sat,
                Day::Sun,
            ],
            360,
            1380,
            20.0,
        )],
    });
    assert!(diff(None, &all).contains(&"New: every day 06:00–23:00 at 20.0 °C".to_string()));
}
