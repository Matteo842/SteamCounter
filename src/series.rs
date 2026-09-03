// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Matteo842
// See LICENSE in the project root for the full terms.

//! Chart data preserves source timestamps and gaps. Monthly means are not hourly counts.
use crate::history::HistorySnapshot;
use chrono::{DateTime, Datelike, Duration, NaiveDate, Utc};

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum ChartRange {
    Hours,
    Week,
    Month,
    Year,
}

impl ChartRange {
    pub fn label(self) -> &'static str {
        match self {
            Self::Hours => "48h",
            Self::Week => "1w",
            Self::Month => "1m",
            Self::Year => "1y",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum SeriesKind {
    Hourly,
    Monthly,
    MonthSummary,
}

#[derive(Clone, Debug)]
pub struct Point {
    pub at: DateTime<Utc>,
    pub value: f64,
}

pub struct Series {
    pub kind: SeriesKind,
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
    pub points: Vec<Point>,
    pub note: String,
}

impl Series {
    pub fn build(
        history: Option<&HistorySnapshot>,
        range: ChartRange,
        month: NaiveDate,
        year: i32,
        now: DateTime<Utc>,
    ) -> Self {
        let month_start = midnight(month);
        let month_end = midnight(month.checked_add_months(chrono::Months::new(1)).unwrap());
        let (start, end) = match range {
            ChartRange::Hours => (now - Duration::hours(48), now),
            ChartRange::Week => (now - Duration::days(7), now),
            ChartRange::Month => (
                month_start,
                month_end.min(now).max(month_start + Duration::seconds(1)),
            ),
            ChartRange::Year => (
                midnight(NaiveDate::from_ymd_opt(year, 1, 1).unwrap()),
                midnight(NaiveDate::from_ymd_opt(year + 1, 1, 1).unwrap()).min(now),
            ),
        };
        let mut series = Self {
            kind: if range == ChartRange::Year {
                SeriesKind::Monthly
            } else {
                SeriesKind::Hourly
            },
            start,
            end,
            points: Vec::new(),
            note: String::new(),
        };
        let Some(history) = history else {
            series.note = "History is unavailable. Try again later.".to_owned();
            return series;
        };
        if range == ChartRange::Year {
            series.points = history
                .months
                .iter()
                .filter(|row| row.month.year() == year)
                .map(|row| {
                    let start = midnight(row.month);
                    let next = midnight(
                        row.month
                            .checked_add_months(chrono::Months::new(1))
                            .unwrap(),
                    );
                    Point {
                        at: start + (next - start) / 2,
                        value: row.players.average_players,
                    }
                })
                .collect();
            series.points.sort_by_key(|point| point.at);
            series.note =
                "Published monthly averages · completed months only · SteamCharts".to_owned();
        } else {
            series.points = history
                .samples
                .iter()
                .filter(|point| point.at >= start && point.at <= end)
                .map(|point| Point {
                    at: point.at,
                    value: point.players as f64,
                })
                .collect();
            series.note = "Hourly player counts · gaps are left empty · times in UTC · SteamCharts"
                .to_owned();
            if range == ChartRange::Month
                && series.points.is_empty()
                && let Some(row) = history.month(month)
            {
                series.kind = SeriesKind::MonthSummary;
                series.end = month_end;
                series.points.push(Point {
                    at: month_start + (month_end - month_start) / 2,
                    value: row.players.average_players,
                });
                series.note = "Published monthly average · hourly detail is no longer available · SteamCharts".to_owned();
            }
            if series.points.is_empty() {
                series.note =
                    "No hourly samples for this period. SteamCharts provides about 30 recent days."
                        .to_owned();
            }
        }
        series
    }

    pub fn connects(&self, left: &Point, right: &Point) -> bool {
        match self.kind {
            SeriesKind::Hourly => (30 * 60..=90 * 60).contains(&(right.at - left.at).num_seconds()),
            SeriesKind::Monthly => {
                let left_index = left.at.year() * 12 + left.at.month0() as i32;
                let right_index = right.at.year() * 12 + right.at.month0() as i32;
                right_index - left_index == 1
            }
            SeriesKind::MonthSummary => false,
        }
    }
    pub fn label(&self) -> &'static str {
        match self.kind {
            SeriesKind::Hourly => "Hourly players",
            SeriesKind::Monthly | SeriesKind::MonthSummary => "Monthly averages",
        }
    }
}

fn midnight(date: NaiveDate) -> DateTime<Utc> {
    date.and_hms_opt(0, 0, 0).unwrap().and_utc()
}

#[cfg(test)]
mod tests;
