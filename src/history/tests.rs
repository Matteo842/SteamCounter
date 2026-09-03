use super::*;

const PAGE: &str = r#"
<div id="app-heading"><abbr class="timeago" title="2026-09-03T11:00:00Z"></abbr></div>
<table class="common-table"><thead><tr>
<th>Month</th><th>Avg. Players</th><th>Gain</th><th>% Gain</th><th>Peak Players</th>
</tr></thead><tbody>
<tr><td class="month-cell">Last 30 Days</td><td>120.50</td><td>-2</td><td>-1%</td><td>300</td></tr>
<tr><td>August 2026</td><td>125.25</td><td>+2</td><td>+1%</td><td>310</td></tr>
<tr><td><span>July 2026</span></td><td>123.00</td><td>-</td><td>-</td><td>305</td></tr>
</tbody></table>"#;

fn at(value: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(value)
        .unwrap()
        .with_timezone(&Utc)
}

fn report() -> HistorySnapshot {
    parse_page(
        PAGE,
        NonZeroU32::new(730).unwrap(),
        at("2026-09-03T12:00:00Z"),
    )
    .unwrap()
}

fn sample(value: &str, players: u64) -> Sample {
    Sample {
        at: at(value),
        players,
    }
}

#[test]
fn published_averages_preserve_periods_and_source_timestamps() {
    let history = report();
    assert_eq!(history.last_30_days.average_players, 120.50);
    assert_eq!(
        history
            .month(parse_month("2026-08").unwrap())
            .unwrap()
            .players
            .average_players,
        125.25
    );
    assert!(history.month(parse_month("2026-09").unwrap()).is_none());
    assert_eq!(history.current_month.starts_at, at("2026-09-01T00:00:00Z"));
    assert_eq!(history.source_updated_at, Some(at("2026-09-03T11:00:00Z")));
    assert!(history.today.average_players.is_none());
    let json = serde_json::to_value(history).unwrap();
    assert!(json["today"]["average_players"].is_null());
    assert_eq!(json["months"][0]["month"], "2026-08-01");
}

#[test]
fn malformed_or_changed_tables_are_errors_not_zeroes() {
    for html in [
        "<html>Login required</html>".to_owned(),
        PAGE.replace("Avg. Players", "Different metric"),
        PAGE.replace("120.50", "NaN"),
        PAGE.replace("120.50", "-1"),
        PAGE.replace("120.50", "999"),
        PAGE.replace("Last 30 Days", "September 2026"),
        PAGE.replace("July 2026", "August 2026"),
        PAGE.replace("<td>300</td>", ""),
    ] {
        assert!(
            parse_page(
                &html,
                NonZeroU32::new(730).unwrap(),
                at("2026-09-03T12:00:00Z")
            )
            .is_err(),
            "{html}"
        );
    }
    let zero = PAGE
        .replace("120.50", "0.00")
        .replace("<td>300</td>", "<td>0</td>");
    assert_eq!(
        parse_page(
            &zero,
            NonZeroU32::new(730).unwrap(),
            at("2026-09-03T12:00:00Z")
        )
        .unwrap()
        .last_30_days
        .average_players,
        0.0
    );
}

#[test]
fn annual_average_weights_leap_year_days_and_requires_all_months() {
    let mut history = report();
    history.months = (1..=12)
        .map(|month| MonthlyAverage {
            month: NaiveDate::from_ymd_opt(2024, month, 1).unwrap(),
            players: PublishedAverage {
                average_players: if month == 2 { 100.0 } else { 0.0 },
                peak_players: 100,
            },
        })
        .collect();
    assert!((history.year(2024).average_players.unwrap() - 100.0 * 29.0 / 366.0).abs() < 0.0001);
    history.months.pop();
    assert_eq!(history.year(2024).months_available, 11);
    assert!(history.year(2024).average_players.is_none());
}

#[test]
fn graph_monthly_and_daily_peaks_are_excluded_from_averages() {
    let json = serde_json::to_string(&[
        (at("2026-08-01T00:00:00Z").timestamp_millis(), 900_000),
        (at("2026-09-02T00:00:00Z").timestamp_millis(), 800_000),
        (at("2026-09-02T00:05:00Z").timestamp_millis(), 100),
        (at("2026-09-02T01:05:00Z").timestamp_millis(), 200),
    ])
    .unwrap();
    let points = parse_samples(&json, at("2026-09-03T12:00:00Z")).unwrap();
    assert_eq!(points.len(), 2);
    let estimate = sample_average(
        &points,
        at("2026-09-02T00:05:00Z"),
        at("2026-09-02T01:05:00Z"),
    );
    assert_eq!(estimate.average_players, Some(150.0));
    assert_eq!(estimate.coverage_percent, 100.0);
}

#[test]
fn averages_clip_boundaries_and_weight_time() {
    let points = vec![
        sample("2026-09-02T00:10:00Z", 0),
        sample("2026-09-02T01:10:00Z", 100),
        sample("2026-09-02T02:10:00Z", 300),
    ];
    let estimate = sample_average(
        &points,
        at("2026-09-02T00:40:00Z"),
        at("2026-09-02T01:40:00Z"),
    );
    assert_eq!(estimate.average_players, Some(112.5));
    assert_eq!(estimate.sample_count, 3);
    assert_eq!(estimate.coverage_percent, 100.0);
}

#[test]
fn gaps_and_unobserved_edges_are_not_filled() {
    let points = vec![
        sample("2026-09-02T00:10:00Z", 100),
        sample("2026-09-02T01:10:00Z", 100),
        sample("2026-09-02T10:10:00Z", 1000),
        sample("2026-09-02T11:10:00Z", 1000),
    ];
    let estimate = sample_average(
        &points,
        at("2026-09-02T00:00:00Z"),
        at("2026-09-02T12:00:00Z"),
    );
    assert_eq!(estimate.average_players, Some(550.0));
    assert!((estimate.coverage_percent - 100.0 * 2.0 / 12.0).abs() < 0.001);
    let sparse = vec![
        sample("2026-09-01T08:00:00Z", 1000),
        sample("2026-09-02T08:00:00Z", 1000),
    ];
    assert!(
        sample_average(
            &sparse,
            at("2026-09-01T00:00:00Z"),
            at("2026-09-03T00:00:00Z")
        )
        .average_players
        .is_none()
    );
}

#[test]
fn invalid_chart_data_is_not_used() {
    let now = at("2026-09-03T12:00:00Z");
    let millis = now.timestamp_millis();
    for json in [
        "<html>Blocked</html>".to_owned(),
        format!("[[{millis},100],[{millis},200]]"),
        format!("[[{millis},100],[{},200]]", millis - 3600000),
        format!("[[{millis},null]]"),
        format!("[[{},100]]", millis + 3600000),
    ] {
        assert!(parse_samples(&json, now).is_err(), "{json}");
    }
}

#[test]
fn chart_failure_preserves_published_monthly_averages() {
    let (steam, server) = crate::tests::fixture(vec![
        ("/app/730", 200, PAGE),
        ("/app/730/chart-data.json", 503, "unavailable"),
    ]);
    let client = SteamChartsClient {
        http: steam.http.clone(),
        base_url: steam.players_url.trim_end_matches("/players").to_owned(),
    };
    let result = client.history(NonZeroU32::new(730).unwrap()).unwrap();
    assert_eq!(result.last_30_days.average_players, 120.5);
    assert!(result.today.average_players.is_none());
    assert_eq!(result.warnings.len(), 1);
    server.join().unwrap();
}

#[test]
fn blocked_or_missing_source_reports_an_error() {
    for (status, message) in [(403, "403"), (404, "404"), (429, "429")] {
        let (steam, server) = crate::tests::fixture(vec![("/app/730", status, "error")]);
        let client = SteamChartsClient {
            http: steam.http.clone(),
            base_url: steam.players_url.trim_end_matches("/players").to_owned(),
        };
        assert!(
            client
                .history(NonZeroU32::new(730).unwrap())
                .unwrap_err()
                .to_string()
                .contains(message)
        );
        server.join().unwrap();
    }
}
