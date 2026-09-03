use std::{
    collections::{BTreeSet, HashMap},
    num::NonZeroU32,
    sync::{Arc, Mutex, mpsc::Sender},
    time::{Duration, Instant},
};

use anyhow::{Result, bail};
use chrono::{Datelike, NaiveDate, Utc};
use eframe::egui::Context;

use super::style::number;
use crate::{Game, GameQuery, NameMatch, PlayerSnapshot, SteamClient, match_name};
use crate::{
    cache::HistoryCache,
    history::{HistorySnapshot, MonthlyAverage, SteamChartsClient},
};

pub struct WorkerMessage {
    pub id: u64,
    pub result: Result<Loaded, String>,
}

pub enum Loaded {
    Dashboard(Box<DashboardData>),
    Candidates(Vec<Game>),
}

pub struct DashboardData {
    pub appid: NonZeroU32,
    pub name: String,
    pub current: Option<PlayerSnapshot>,
    pub history: Option<HistorySnapshot>,
    pub warnings: Vec<String>,
    pub demo: bool,
}

#[derive(Default)]
pub struct Session {
    searches: HashMap<String, (Instant, Vec<Game>)>,
    live: HashMap<NonZeroU32, PlayerSnapshot>,
}

impl Session {
    fn search(&mut self, steam: &SteamClient, query: &str) -> Result<Vec<Game>> {
        let key = query.trim().to_lowercase();
        if let Some((at, games)) = self.searches.get(&key)
            && at.elapsed() < Duration::from_secs(24 * 3600)
        {
            return Ok(games.clone());
        }
        let games = steam.search(query)?;
        if self.searches.len() >= 128 {
            self.searches.clear();
        }
        self.searches.insert(key, (Instant::now(), games.clone()));
        Ok(games)
    }
    fn snapshot(
        &mut self,
        steam: &SteamClient,
        appid: NonZeroU32,
        name: Option<String>,
    ) -> Result<PlayerSnapshot> {
        if let Some(snapshot) = self.live.get(&appid)
            && Utc::now().signed_duration_since(snapshot.checked_at) < chrono::Duration::minutes(1)
        {
            return Ok(snapshot.clone());
        }
        let name = name.or_else(|| {
            self.live
                .get(&appid)
                .and_then(|snapshot| snapshot.name.clone())
        });
        let snapshot = steam.snapshot(appid, name)?;
        if self.live.len() >= 128 {
            self.live.clear();
        }
        self.live.insert(appid, snapshot.clone());
        Ok(snapshot)
    }
}

pub fn spawn(
    id: u64,
    query: String,
    selected: Option<Game>,
    sender: Sender<WorkerMessage>,
    ctx: Context,
    cache: HistoryCache,
    session: Arc<Mutex<Session>>,
) {
    // Tutte le richieste bloccanti restano fuori dal thread della finestra.
    std::thread::spawn(move || {
        let result = session
            .lock()
            .map_err(|_| {
                anyhow::anyhow!("The request worker is unavailable. Restart SteamCounter.")
            })
            .and_then(|mut session| load(query, selected, cache, &mut session))
            .map_err(|error| format!("{error:#}"));
        let _ = sender.send(WorkerMessage { id, result });
        ctx.request_repaint();
    });
}

fn load(
    query: String,
    selected: Option<Game>,
    cache: HistoryCache,
    session: &mut Session,
) -> Result<Loaded> {
    let steam = SteamClient::new(Duration::from_secs(15))?;
    let (appid, known_name) = if let Some(game) = selected {
        (game.appid, Some(game.name))
    } else {
        match GameQuery::parse(&query)? {
            GameQuery::AppId(appid) => (appid, None),
            GameQuery::Name(name) => match match_name(&name, session.search(&steam, &name)?) {
                NameMatch::Found(game) => (game.appid, Some(game.name)),
                NameMatch::Ambiguous(games) => return Ok(Loaded::Candidates(games)),
                NameMatch::NotFound => bail!("No game found for “{name}”."),
            },
        }
    };
    let mut warnings = Vec::new();
    let current = match session.snapshot(&steam, appid, known_name.clone()) {
        Ok(value) => Some(value),
        Err(error) => {
            warnings.push(format!("Current count unavailable: {error:#}"));
            None
        }
    };
    let history = match SteamChartsClient::new(Duration::from_secs(15))
        .and_then(|client| client.history_cached(appid, &cache))
    {
        Ok(value) => {
            warnings.extend(value.warnings.clone());
            Some(value)
        }
        Err(error) => {
            warnings.push(format!("History unavailable: {error:#}"));
            None
        }
    };
    if current.is_none() && history.is_none() {
        bail!("{}", warnings.join("\n"));
    }
    let name = current
        .as_ref()
        .and_then(|snapshot| snapshot.name.clone())
        .or(known_name)
        .unwrap_or_else(|| format!("Steam App {appid}"));
    Ok(Loaded::Dashboard(Box::new(DashboardData {
        appid,
        name,
        current,
        history,
        warnings,
        demo: false,
    })))
}

pub struct Metric {
    pub value: String,
    pub period: String,
    pub note: String,
    pub detail: String,
}

impl Metric {
    fn new(
        value: Option<f64>,
        estimated: bool,
        period: impl Into<String>,
        note: impl Into<String>,
        detail: impl Into<String>,
    ) -> Self {
        Self {
            value: value
                .map(|value| format!("{}{}", if estimated { "~" } else { "" }, number(value)))
                .unwrap_or_else(|| "—".to_owned()),
            period: period.into(),
            note: note.into(),
            detail: detail.into(),
        }
    }
}

pub struct Metrics {
    pub live: Metric,
    pub week: Metric,
    pub month: Metric,
    pub year: Metric,
}

impl DashboardData {
    pub fn demo() -> Self {
        Self {
            appid: NonZeroU32::new(730).unwrap(),
            name: "Counter-Strike 2".to_owned(),
            current: None,
            history: None,
            warnings: Vec::new(),
            demo: true,
        }
    }

    pub fn months(&self) -> Vec<NaiveDate> {
        let now = Utc::now();
        let current = NaiveDate::from_ymd_opt(now.year(), now.month(), 1).unwrap();
        let mut months = BTreeSet::from([current]);
        if let Some(history) = &self.history {
            months.extend(history.months.iter().map(|month| month.month));
        }
        if self.demo {
            for offset in 1..12 {
                months.insert(
                    current
                        .checked_sub_months(chrono::Months::new(offset))
                        .unwrap(),
                );
            }
        }
        months.into_iter().rev().collect()
    }

    pub fn years(&self) -> Vec<i32> {
        let now = Utc::now().year();
        let mut years = BTreeSet::from([now]);
        if let Some(history) = &self.history {
            years.extend(history.months.iter().map(|month| month.month.year()));
        }
        if self.demo {
            years.extend((now - 3)..now);
        }
        years.into_iter().rev().collect()
    }

    pub fn metrics(&self, month: NaiveDate, year: i32) -> Metrics {
        if self.demo {
            return Metrics {
                live: Metric::new(
                    Some(1_093_369.0),
                    false,
                    "Right now",
                    "Today avg ~782,410",
                    "Example data: no network requests.",
                ),
                week: Metric::new(
                    Some(814_343.0),
                    true,
                    "Last 7 days",
                    "Average · 99% coverage",
                    "Example data.",
                ),
                month: Metric::new(
                    Some(737_670.0 + (month.month() as f64 - Utc::now().month() as f64) * 4513.0),
                    true,
                    "",
                    "Selected month average",
                    "Example data.",
                ),
                year: Metric::new(
                    Some(990_170.0 + (year - Utc::now().year()) as f64 * 9852.0),
                    true,
                    "",
                    "Provisional average",
                    "Example data.",
                ),
            };
        }
        let history = self.history.as_ref();
        let today = history.and_then(|h| h.today.average_players);
        let live = Metric::new(
            self.current.as_ref().map(|value| value.player_count as f64),
            false,
            "Right now",
            today
                .map(|value| format!("Today avg ~{}", number(value)))
                .unwrap_or_else(|| "Today avg unavailable".to_owned()),
            format!(
                "Concurrent players: latest count from Steam, reused for up to 60 seconds. Today's average uses SteamCharts samples since midnight UTC. Coverage: {:.1}%.",
                history.map_or(0.0, |h| h.today.coverage_percent)
            ),
        );
        let week = Metric::new(
            history.and_then(|h| h.last_7_days.average_players),
            true,
            "Last 7 days",
            history
                .map(|h| format!("Coverage {:.1}%", h.last_7_days.coverage_percent))
                .unwrap_or_else(|| "History unavailable".to_owned()),
            "Estimate from hourly samples over the last 7 days. Missing intervals are not treated as zero.",
        );
        let monthly = if let Some(h) = history {
            if month == h.current_month.starts_at.date_naive() {
                Metric::new(
                    h.current_month.average_players,
                    true,
                    "",
                    format!(
                        "In progress · {:.1}% coverage",
                        h.current_month.coverage_percent
                    ),
                    "Average since the first of this month UTC, over time covered by samples. This is not the last 30 days.",
                )
            } else {
                Metric::new(
                    h.month(month).map(|row| row.players.average_players),
                    false,
                    "",
                    "Published average",
                    "Monthly average published by SteamCharts.",
                )
            }
        } else {
            Metric::new(
                None,
                false,
                "",
                "History unavailable",
                "The historical data source did not respond.",
            )
        };
        let annual = if let Some(h) = history {
            if year == Utc::now().year() {
                let (average, count) = provisional_year(&h.months, year);
                Metric::new(
                    average,
                    true,
                    "",
                    format!("Provisional · {count} full months"),
                    "Current year: day-weighted average of available completed months. Excludes the current month; this is not a full-year average.",
                )
            } else {
                let average = h.year(year);
                Metric::new(
                    average.average_players,
                    true,
                    "",
                    format!("{} / 12 months available", average.months_available),
                    "Yearly estimate weighted by calendar days. Requires all 12 months.",
                )
            }
        } else {
            Metric::new(
                None,
                false,
                "",
                "History unavailable",
                "The historical data source did not respond.",
            )
        };
        Metrics {
            live,
            week,
            month: monthly,
            year: annual,
        }
    }
}

fn provisional_year(months: &[MonthlyAverage], year: i32) -> (Option<f64>, usize) {
    let mut days = 0.0;
    let mut total = 0.0;
    let mut count = 0;
    for row in months.iter().filter(|row| row.month.year() == year) {
        let next = row
            .month
            .checked_add_months(chrono::Months::new(1))
            .unwrap();
        let weight = (next - row.month).num_days() as f64;
        days += weight;
        total += weight * row.players.average_players;
        count += 1;
    }
    ((days > 0.0).then(|| total / days), count)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::history::PublishedAverage;

    #[test]
    fn provisional_year_is_weighted_and_never_invents_missing_months() {
        let rows = [(2024, 1, 100.0), (2024, 2, 300.0), (2023, 12, 5000.0)]
            .into_iter()
            .map(|(year, month, average)| MonthlyAverage {
                month: NaiveDate::from_ymd_opt(year, month, 1).unwrap(),
                players: PublishedAverage {
                    average_players: average,
                    peak_players: 6000,
                },
            })
            .collect::<Vec<_>>();
        let (average, months) = provisional_year(&rows, 2024);
        assert_eq!(months, 2);
        assert!((average.unwrap() - (100.0 * 31.0 + 300.0 * 29.0) / 60.0).abs() < 0.001);
        assert_eq!(provisional_year(&rows, 2022), (None, 0));
    }
}
