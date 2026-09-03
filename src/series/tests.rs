use super::*;
use crate::history::{MonthlyAverage, PublishedAverage, Sample, SampleAverage};

fn at(text: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(text)
        .unwrap()
        .with_timezone(&Utc)
}
fn history() -> HistorySnapshot {
    let now = at("2026-09-03T12:00:00Z");
    let empty = || SampleAverage {
        starts_at: now,
        ends_at: now,
        average_players: None,
        sample_count: 0,
        coverage_percent: 0.0,
        method: "test",
    };
    HistorySnapshot {
        source: "SteamCharts",
        source_url: String::new(),
        retrieved_at: now,
        source_updated_at: None,
        latest_sample_at: None,
        last_30_days: PublishedAverage {
            average_players: 100.0,
            peak_players: 1000,
        },
        months: [(1, 10.0), (2, 20.0), (4, 40.0), (8, 80.0)]
            .into_iter()
            .map(|(month, value)| MonthlyAverage {
                month: NaiveDate::from_ymd_opt(2026, month, 1).unwrap(),
                players: PublishedAverage {
                    average_players: value,
                    peak_players: 999_999,
                },
            })
            .collect(),
        today: empty(),
        last_7_days: empty(),
        current_month: empty(),
        warnings: vec![],
        samples: [
            ("2026-09-02T00:05:00Z", 100),
            ("2026-09-02T01:05:00Z", 200),
            ("2026-09-02T08:05:00Z", 300),
        ]
        .into_iter()
        .map(|(time, players)| Sample {
            at: at(time),
            players,
        })
        .collect(),
        cache_state: Default::default(),
    }
}
fn series(h: Option<&HistorySnapshot>, range: ChartRange, month: u32) -> Series {
    Series::build(
        h,
        range,
        NaiveDate::from_ymd_opt(2026, month, 1).unwrap(),
        2026,
        at("2026-09-03T12:00:00Z"),
    )
}

#[test]
fn hourly_series_preserves_real_timestamps_and_does_not_bridge_gaps() {
    let h = history();
    let s = series(Some(&h), ChartRange::Hours, 9);
    assert_eq!(s.points.len(), 3);
    assert_eq!(s.points[0].at, h.samples[0].at);
    assert_eq!(s.points[0].value, 100.0);
    assert!(s.connects(&s.points[0], &s.points[1]));
    assert!(!s.connects(&s.points[1], &s.points[2]));
    assert!(s.points.last().unwrap().at < s.end); // no extension to now
}

#[test]
fn yearly_chart_uses_means_not_peaks_and_keeps_missing_months_empty() {
    let h = history();
    let s = series(Some(&h), ChartRange::Year, 9);
    assert_eq!(s.kind, SeriesKind::Monthly);
    assert_eq!(
        s.points.iter().map(|p| p.value).collect::<Vec<_>>(),
        [10.0, 20.0, 40.0, 80.0]
    );
    assert!(s.connects(&s.points[0], &s.points[1]));
    assert!(!s.connects(&s.points[1], &s.points[2]));
}

#[test]
fn old_month_has_a_single_published_summary_not_a_fabricated_curve() {
    let h = history();
    let s = series(Some(&h), ChartRange::Month, 8);
    assert_eq!(s.kind, SeriesKind::MonthSummary);
    assert_eq!(s.points.len(), 1);
    assert_eq!(s.points[0].value, 80.0);
    assert!(series(None, ChartRange::Month, 8).points.is_empty());
}
