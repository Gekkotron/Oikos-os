//! Solar figures: when the card shows, which entities measure the production, and the sums and
//! resampling the card and page draw from. Pure, no UI.

use std::collections::BTreeMap;

use chrono::{Duration, NaiveDate, TimeZone};
use chrono_tz::Tz;
use oikos_api::{HistorySeries, SolarDay, SolarPeriod, SolarView};
use oikos_core::energy::{EnergyEntity, EnergyMeters, EnergyRole, EnergyUnits, PowerUnits};

/// Age, in seconds, past which the solar card is hidden (24 h).
pub const SOLAR_MAX_AGE_S: u64 = 86_400;

/// Whether the solar card shows: there is a forecast and it is at most 24 h old.
pub fn solar_card_visible(s: Option<&SolarView>, now_s: u64) -> bool {
    s.is_some_and(|s| now_s.saturating_sub(s.fetched_at) <= SOLAR_MAX_AGE_S)
}

/// Unix seconds of local midnight of `date` and of the next day, in `zone`.
pub fn day_bounds(zone: Tz, date: NaiveDate) -> (u64, u64) {
    let midnight = |d: NaiveDate| {
        let naive = d.and_hms_opt(0, 0, 0).unwrap_or_default();
        // A zone whose midnight falls in a clock gap resumes at the first valid instant after it.
        let local = (0..=3)
            .find_map(|h| {
                zone.from_local_datetime(&(naive + Duration::hours(h)))
                    .earliest()
            })
            .map(|t| t.timestamp())
            .unwrap_or_else(|| naive.and_utc().timestamp());
        local.max(0) as u64
    };
    (midnight(date), midnight(date + Duration::days(1)))
}

/// The local date of Unix second `t` in `zone`.
pub fn local_date(zone: Tz, t: u64) -> NaiveDate {
    zone.timestamp_opt(t as i64, 0)
        .single()
        .map(|d| d.date_naive())
        .unwrap_or_default()
}

/// A solar power entity and how to read it in kW.
#[derive(Clone, Debug, PartialEq)]
pub struct PowerSource {
    pub id: String,
    /// kW per unit of the entity's value (W is 0.001).
    pub kw_per_unit: f64,
    /// -1 when the sensor reports production as negative.
    pub sign: f64,
}

/// Where the measured production comes from: lifetime counters when there are any, else powers.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SolarSources {
    /// Counter entity ids, sorted and unique.
    pub counters: Vec<String>,
    pub powers: Vec<PowerSource>,
}

/// The solar entities among `all`: energy-unit ones are counters; power-unit ones are power
/// sources, and the counter they pair with (if any) is a counter too.
pub fn solar_sources(all: &[EnergyEntity]) -> SolarSources {
    let mut out = SolarSources::default();
    for e in all.iter().filter(|e| e.role == Some(EnergyRole::Solar)) {
        if EnergyUnits::factor(e.unit.as_deref()).is_some() {
            out.counters.push(e.id.clone());
        } else if let Some(f) = PowerUnits::factor(e.unit.as_deref()) {
            out.powers.push(PowerSource {
                id: e.id.clone(),
                kw_per_unit: f / 1000.0,
                sign: if e.invert { -1.0 } else { 1.0 },
            });
            if let Some(c) = EnergyMeters::meter_for(e, all) {
                out.counters.push(c.id.clone());
            }
        }
    }
    out.counters.sort();
    out.counters.dedup();
    out
}

/// Sums `sign x kwh[id]` over `ids`; `None` when any is missing or unknown.
fn sum_known<'a>(
    ids: impl Iterator<Item = (&'a str, f64)>,
    kwh: &BTreeMap<String, Option<f64>>,
) -> Option<f64> {
    let mut total = 0.0;
    for (id, sign) in ids {
        total += sign * (*kwh.get(id)?)?;
    }
    Some(total)
}

/// kWh produced since local midnight: the counters' sum when there are counters, else the power
/// sources' integrals; `None` when nothing is known (never a made-up zero).
pub fn measured_today(
    src: &SolarSources,
    counter_kwh: &BTreeMap<String, Option<f64>>,
    power_kwh: &BTreeMap<String, Option<f64>>,
) -> Option<f64> {
    let total = if !src.counters.is_empty() {
        sum_known(src.counters.iter().map(|c| (c.as_str(), 1.0)), counter_kwh)?
    } else if !src.powers.is_empty() {
        sum_known(
            src.powers.iter().map(|p| (p.id.as_str(), p.sign)),
            power_kwh,
        )?
    } else {
        return None;
    };
    Some(total.max(0.0))
}

/// The production right now, kW: the live solar power sensors summed; `None` without one.
pub fn measured_power_kw(all: &[EnergyEntity]) -> Option<f64> {
    let mut total: Option<f64> = None;
    for e in all
        .iter()
        .filter(|e| e.role == Some(EnergyRole::Solar) && e.available)
    {
        let (Some(f), Some(v)) = (PowerUnits::factor(e.unit.as_deref()), e.value) else {
            continue;
        };
        let sign = if e.invert { -1.0 } else { 1.0 };
        *total.get_or_insert(0.0) += v * f / 1000.0 * sign;
    }
    total.map(|t| t.max(0.0))
}

/// The measured curve on the forecast's grid: per period, the mean of each source's points in
/// `[end - minutes, end)` in kW, summed over the sources that have points. Periods outside
/// `(from, until)` or with no point are absent. Returns `(period end, kW)`, sorted by end.
pub fn resample(
    series: &[(PowerSource, HistorySeries)],
    periods: &[SolarPeriod],
    from: u64,
    until: u64,
) -> Vec<(u64, f64)> {
    let mut out: Vec<(u64, f64)> = periods
        .iter()
        .filter_map(|p| {
            let dur = u64::from(p.minutes) * 60;
            let start = p.end.saturating_sub(dur);
            if p.end <= from || start >= until {
                return None;
            }
            let mut total: Option<f64> = None;
            for (src, s) in series {
                let (sum, n) = s
                    .points
                    .iter()
                    .filter(|pt| pt.0 >= start as i64 && pt.0 < p.end as i64)
                    .fold((0.0, 0u32), |(sum, n), pt| (sum + pt.1, n + 1));
                if n > 0 {
                    *total.get_or_insert(0.0) += sum / f64::from(n) * src.kw_per_unit * src.sign;
                }
            }
            Some((p.end, total?.max(0.0)))
        })
        .collect();
    out.sort_by_key(|(end, _)| *end);
    out
}

/// The periods that start in `[from, to)`.
pub fn periods_in(periods: &[SolarPeriod], from: u64, to: u64) -> Vec<SolarPeriod> {
    periods
        .iter()
        .filter(|p| {
            let start = p.end.saturating_sub(u64::from(p.minutes) * 60);
            start >= from && start < to
        })
        .cloned()
        .collect()
}

/// kWh the forecast expected up to `now`; a period containing `now` counts in proportion.
pub fn forecast_so_far(periods: &[SolarPeriod], now: u64) -> f64 {
    periods
        .iter()
        .map(|p| {
            let dur = u64::from(p.minutes) * 60;
            let start = p.end.saturating_sub(dur);
            let done = now.saturating_sub(start).min(dur);
            // estimate is mean kW over the period, so kWh = kW x hours elapsed.
            p.estimate * (done as f64 / 3600.0)
        })
        .sum()
}

/// The forecast day dated `today + 1`.
pub fn tomorrow(s: &SolarView, today: NaiveDate) -> Option<&SolarDay> {
    let next = today + Duration::days(1);
    s.days
        .iter()
        .find(|d| NaiveDate::parse_from_str(&d.date, "%Y-%m-%d").ok() == Some(next))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono_tz::Europe::Paris;

    fn view(fetched_at: u64, days: &[&str]) -> SolarView {
        let days: Vec<_> = days
            .iter()
            .map(|d| serde_json::json!({"date": d, "estimate": 5.0, "low": 3.0, "high": 7.0}))
            .collect();
        serde_json::from_value(serde_json::json!({
            "fetched_at": fetched_at, "stale": false, "next_refresh_at": null,
            "calls_today": 1, "daily_limit": 10,
            "now": {"power": null, "remaining_today": null},
            "today": {"date": "2026-10-24", "estimate": null, "low": null, "high": null,
                      "peak": null, "best_hours": null},
            "days": days, "sites": [], "periods": []
        }))
        .expect("solar view")
    }

    fn ent(id: &str, unit: &str) -> EnergyEntity {
        EnergyEntity {
            id: id.to_string(),
            device_id: None,
            name: id.to_string(),
            label: None,
            unit: Some(unit.to_string()),
            value: None,
            available: true,
            role: None,
            invert: false,
            order: None,
            icon: None,
            meter: None,
        }
    }

    fn solar(id: &str, unit: &str) -> EnergyEntity {
        EnergyEntity {
            role: Some(EnergyRole::Solar),
            ..ent(id, unit)
        }
    }

    fn period(end: u64, minutes: u32, estimate: f64) -> SolarPeriod {
        SolarPeriod {
            end,
            minutes,
            estimate,
            low: 0.0,
            high: 0.0,
        }
    }

    fn src(id: &str, kw_per_unit: f64, sign: f64) -> PowerSource {
        PowerSource {
            id: id.to_string(),
            kw_per_unit,
            sign,
        }
    }

    fn series(points: &[(i64, f64)]) -> HistorySeries {
        HistorySeries {
            window: 300,
            points: points.iter().map(|&(t, v)| (t, v, v, v)).collect(),
        }
    }

    fn map(items: &[(&str, Option<f64>)]) -> BTreeMap<String, Option<f64>> {
        items.iter().map(|(k, v)| (k.to_string(), *v)).collect()
    }

    fn date(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    #[test]
    fn card_hidden_when_missing_or_older_than_a_day() {
        let now = 1_000_000;
        assert!(!solar_card_visible(None, now));
        assert!(solar_card_visible(Some(&view(now - 86_400, &[])), now));
        assert!(!solar_card_visible(Some(&view(now - 86_401, &[])), now));
    }

    #[test]
    fn day_bounds_on_dst_change() {
        let (from, to) = day_bounds(Paris, date(2026, 10, 25));
        assert_eq!(to - from, 25 * 3600);
        let (from, to) = day_bounds(Paris, date(2026, 3, 29));
        assert_eq!(to - from, 23 * 3600);
        assert_eq!(local_date(Paris, from), date(2026, 3, 29));
        assert_eq!(local_date(Paris, from - 1), date(2026, 3, 28));
    }

    #[test]
    fn sources_pair_power_with_its_counter() {
        let on_device = |e: EnergyEntity, label: &str| EnergyEntity {
            device_id: Some("inv".into()),
            label: Some(label.into()),
            ..e
        };
        let all = vec![
            on_device(solar("p", "W"), "Power"),
            on_device(ent("e", "kWh"), "Energy"),
            ent("other", "kWh"),
        ];
        assert_eq!(
            solar_sources(&all),
            SolarSources {
                counters: vec!["e".into()],
                powers: vec![src("p", 0.001, 1.0)],
            }
        );
    }

    #[test]
    fn energy_unit_solar_entity_is_a_counter() {
        let s = solar_sources(&[solar("c", "Wh")]);
        assert_eq!(s.counters, vec!["c".to_string()]);
        assert!(s.powers.is_empty());
    }

    #[test]
    fn inverted_power_source_is_positive() {
        let all = [EnergyEntity {
            invert: true,
            value: Some(-800.0),
            ..solar("p", "W")
        }];
        let s = solar_sources(&all);
        assert_eq!(s.powers, vec![src("p", 0.001, -1.0)]);
        let kw = measured_power_kw(&all).unwrap();
        assert!((kw - 0.8).abs() < 1e-9);
        let periods = [period(1000 + 1800, 30, 1.0)];
        let r = resample(
            &[(s.powers[0].clone(), series(&[(1000, -1500.0)]))],
            &periods,
            0,
            10_000,
        );
        assert_eq!(r.len(), 1);
        assert!((r[0].1 - 1.5).abs() < 1e-9);
    }

    #[test]
    fn measured_today_prefers_counters() {
        let s = SolarSources {
            counters: vec!["e".into()],
            powers: vec![src("p", 0.001, 1.0)],
        };
        let v = measured_today(&s, &map(&[("e", Some(4.2))]), &map(&[("p", Some(9.0))]));
        assert_eq!(v, Some(4.2));
    }

    #[test]
    fn measured_today_none_when_a_counter_is_missing() {
        let s = SolarSources {
            counters: vec!["e".into()],
            powers: vec![],
        };
        assert_eq!(measured_today(&s, &map(&[("e", None)]), &map(&[])), None);
        assert_eq!(measured_today(&s, &map(&[]), &map(&[])), None);
    }

    #[test]
    fn measured_today_from_power_when_no_counter() {
        let s = SolarSources {
            counters: vec![],
            powers: vec![src("p", 0.001, -1.0)],
        };
        assert_eq!(
            measured_today(&s, &map(&[]), &map(&[("p", Some(-3.0))])),
            Some(3.0)
        );
        assert_eq!(
            measured_today(&SolarSources::default(), &map(&[]), &map(&[])),
            None
        );
    }

    #[test]
    fn resample_means_points_per_period_and_sums_sources() {
        let periods = [
            period(1000 + 1800, 30, 1.0),
            period(1000 + 3600, 30, 1.0),
            period(1000 + 5400, 30, 1.0),
        ];
        let a = (
            src("a", 0.001, 1.0),
            series(&[(1000, 1000.0), (1900, 2000.0)]),
        );
        let b = (src("b", 0.001, 1.0), series(&[(1500, 500.0)]));
        let r = resample(&[a.clone(), b.clone()], &periods, 0, 100_000);
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].0, 2800);
        assert!((r[0].1 - 2.0).abs() < 1e-9);
        // Periods ending at or before `from` or starting at or after `until` are absent.
        assert!(resample(&[a.clone(), b.clone()], &periods, 2800, 100_000).is_empty());
        assert!(resample(&[a, b], &periods, 0, 1000).is_empty());
    }

    #[test]
    fn forecast_so_far_prorates_current_period() {
        let periods = [period(1800, 30, 2.0), period(3600, 30, 2.0)];
        assert!((forecast_so_far(&periods, 2700) - 1.5).abs() < 1e-9);
        assert_eq!(periods_in(&periods, 1800, 3600).len(), 1);
    }

    #[test]
    fn tomorrow_is_matched_by_date() {
        let s = view(0, &["2026-10-25", "2026-10-26"]);
        assert_eq!(
            tomorrow(&s, date(2026, 10, 24)).map(|d| d.date.as_str()),
            Some("2026-10-25")
        );
        assert!(tomorrow(&view(0, &["2026-10-26"]), date(2026, 10, 24)).is_none());
    }
}
