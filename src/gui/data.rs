use std::{collections::BTreeSet, num::NonZeroU32, sync::mpsc::Sender, time::Duration};

use anyhow::{Result, bail};
use chrono::{DateTime, Datelike, NaiveDate, Utc};
use eframe::egui::Context;

use super::style::number;
use crate::history::{HistorySnapshot, MonthlyAverage, SteamChartsClient};
use crate::{Game, GameQuery, NameMatch, PlayerSnapshot, SteamClient, match_name};

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
    pub updated_at: DateTime<Utc>,
    pub demo: bool,
}

pub fn spawn(
    id: u64,
    query: String,
    selected: Option<Game>,
    sender: Sender<WorkerMessage>,
    ctx: Context,
) {
    // Tutte le richieste bloccanti restano fuori dal thread della finestra.
    std::thread::spawn(move || {
        let result = load(query, selected).map_err(|error| format!("{error:#}"));
        let _ = sender.send(WorkerMessage { id, result });
        ctx.request_repaint();
    });
}

fn load(query: String, selected: Option<Game>) -> Result<Loaded> {
    let steam = SteamClient::new(Duration::from_secs(15))?;
    let (appid, known_name) = if let Some(game) = selected {
        (game.appid, Some(game.name))
    } else {
        match GameQuery::parse(&query)? {
            GameQuery::AppId(appid) => (appid, None),
            GameQuery::Name(name) => match match_name(&name, steam.search(&name)?) {
                NameMatch::Found(game) => (game.appid, Some(game.name)),
                NameMatch::Ambiguous(games) => return Ok(Loaded::Candidates(games)),
                NameMatch::NotFound => bail!("Nessun gioco trovato per «{name}»."),
            },
        }
    };
    let mut warnings = Vec::new();
    let current = match steam.snapshot(appid, known_name.clone()) {
        Ok(value) => Some(value),
        Err(error) => {
            warnings.push(format!("Conteggio attuale non disponibile: {error:#}"));
            None
        }
    };
    let history = match SteamChartsClient::new(Duration::from_secs(15))
        .and_then(|client| client.history(appid))
    {
        Ok(value) => {
            warnings.extend(value.warnings.clone());
            Some(value)
        }
        Err(error) => {
            warnings.push(format!("Storico non disponibile: {error:#}"));
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
        updated_at: Utc::now(),
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
            updated_at: Utc::now(),
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
                    "In questo momento",
                    "Media oggi ~782.410",
                    "Dati dimostrativi: nessuna richiesta di rete.",
                ),
                week: Metric::new(
                    Some(814_343.0),
                    true,
                    "Ultimi 7 giorni",
                    "Media · copertura 99%",
                    "Dati dimostrativi.",
                ),
                month: Metric::new(
                    Some(737_670.0 + (month.month() as f64 - Utc::now().month() as f64) * 4513.0),
                    true,
                    "",
                    "Media · mese selezionato",
                    "Dati dimostrativi.",
                ),
                year: Metric::new(
                    Some(990_170.0 + (year - Utc::now().year()) as f64 * 9852.0),
                    true,
                    "",
                    "Media provvisoria",
                    "Dati dimostrativi.",
                ),
            };
        }
        let history = self.history.as_ref();
        let today = history.and_then(|h| h.today.average_players);
        let live = Metric::new(
            self.current.as_ref().map(|value| value.player_count as f64),
            false,
            "In questo momento",
            today
                .map(|value| format!("Media oggi ~{}", number(value)))
                .unwrap_or_else(|| "Media oggi non disponibile".to_owned()),
            format!(
                "Giocatori contemporanei: dato attuale da Steam. La media di oggi usa campioni SteamCharts da mezzanotte UTC. Copertura: {:.1}%.",
                history.map_or(0.0, |h| h.today.coverage_percent)
            ),
        );
        let week = Metric::new(
            history.and_then(|h| h.last_7_days.average_players),
            true,
            "Ultimi 7 giorni",
            history
                .map(|h| format!("Copertura {:.1}%", h.last_7_days.coverage_percent))
                .unwrap_or_else(|| "Storico non disponibile".to_owned()),
            "Stima sui campioni orari degli ultimi 7 giorni. I buchi non sono considerati zero.",
        );
        let monthly = if let Some(h) = history {
            if month == h.current_month.starts_at.date_naive() {
                Metric::new(
                    h.current_month.average_players,
                    true,
                    "",
                    format!(
                        "In corso · copertura {:.1}%",
                        h.current_month.coverage_percent
                    ),
                    "Media del mese corrente, dal primo del mese UTC, sul tempo coperto dai campioni. Non equivale agli ultimi 30 giorni.",
                )
            } else {
                Metric::new(
                    h.month(month).map(|row| row.players.average_players),
                    false,
                    "",
                    "Media pubblicata",
                    "Media mensile pubblicata da SteamCharts.",
                )
            }
        } else {
            Metric::new(
                None,
                false,
                "",
                "Storico non disponibile",
                "La fonte storica non ha risposto.",
            )
        };
        let annual = if let Some(h) = history {
            if year == Utc::now().year() {
                let (average, count) = provisional_year(&h.months, year);
                Metric::new(
                    average,
                    true,
                    "",
                    format!("Provvisoria · {count} mesi conclusi"),
                    "Anno in corso: media ponderata per i giorni dei soli mesi conclusi disponibili. Non include il mese corrente e non e una media annuale completa.",
                )
            } else {
                let average = h.year(year);
                Metric::new(
                    average.average_players,
                    true,
                    "",
                    format!("{} / 12 mesi disponibili", average.months_available),
                    "Stima annuale ponderata per i giorni dei mesi. Richiede tutti i 12 mesi.",
                )
            }
        } else {
            Metric::new(
                None,
                false,
                "",
                "Storico non disponibile",
                "La fonte storica non ha risposto.",
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
