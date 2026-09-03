//! Statistiche su richiesta da SteamCharts: nessun database o processo in background.
//!
//! Gli endpoint pubblici non costituiscono un'API stabile. La tabella contiene medie
//! mensili, mentre il grafico mescola campioni recenti e picchi storici: questi ultimi
//! non devono mai essere usati per stimare una media.

use std::{collections::HashSet, io::Read, num::NonZeroU32, time::Duration};

use anyhow::{Context, Result, bail, ensure};
use chrono::{DateTime, Datelike, NaiveDate, NaiveTime, Utc};
use reqwest::blocking::Client;
use scraper::{ElementRef, Html, Selector};
use serde::Serialize;

const MAX_RESPONSE_BYTES: u64 = 2 * 1024 * 1024;
const MIN_SAMPLE_GAP_SECONDS: i64 = 30 * 60;
const MAX_SAMPLE_GAP_SECONDS: i64 = 90 * 60;

#[derive(Debug, Clone, Serialize)]
pub struct PublishedAverage {
    pub average_players: f64,
    pub peak_players: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct MonthlyAverage {
    /// Primo giorno del mese, ad esempio 2026-08-01.
    pub month: NaiveDate,
    #[serde(flatten)]
    pub players: PublishedAverage,
}

#[derive(Debug, Serialize)]
pub struct SampleAverage {
    pub starts_at: DateTime<Utc>,
    pub ends_at: DateTime<Utc>,
    pub average_players: Option<f64>,
    pub sample_count: usize,
    /// Percentuale dell'intervallo richiesta coperta da coppie di campioni orari.
    pub coverage_percent: f64,
    pub method: &'static str,
}

#[derive(Debug, Serialize)]
pub struct YearAverage {
    pub year: i32,
    pub average_players: Option<f64>,
    pub months_available: usize,
    pub method: &'static str,
}

#[derive(Debug, Serialize)]
pub struct HistorySnapshot {
    pub source: &'static str,
    pub source_url: String,
    pub retrieved_at: DateTime<Utc>,
    /// Timestamp pubblicato nella pagina; non viene sostituito dall'ora del download.
    pub source_updated_at: Option<DateTime<Utc>>,
    pub latest_sample_at: Option<DateTime<Utc>>,
    pub last_30_days: PublishedAverage,
    pub months: Vec<MonthlyAverage>,
    pub today: SampleAverage,
    pub last_7_days: SampleAverage,
    pub current_month: SampleAverage,
    pub warnings: Vec<String>,
}

impl HistorySnapshot {
    pub fn month(&self, month: NaiveDate) -> Option<&MonthlyAverage> {
        self.months.iter().find(|row| row.month == month)
    }

    /// La media annuale e una stima ponderata per i giorni dei 12 mesi pubblicati.
    /// Non si spaccia un anno incompleto per un anno intero.
    pub fn year(&self, year: i32) -> YearAverage {
        let rows: Vec<_> = self
            .months
            .iter()
            .filter(|row| row.month.year() == year)
            .collect();
        let mut days = 0.0;
        let mut total = 0.0;
        for row in &rows {
            let weight = (next_month(row.month) - row.month).num_days() as f64;
            days += weight;
            total += row.players.average_players * weight;
        }
        YearAverage {
            year,
            average_players: (rows.len() == 12).then(|| total / days),
            months_available: rows.len(),
            method: "calendar_day_weighted_monthly_means_estimate",
        }
    }
}

pub struct SteamChartsClient {
    http: Client,
    base_url: String,
}

impl SteamChartsClient {
    pub fn new(timeout: Duration) -> Result<Self> {
        Ok(Self {
            http: Client::builder()
                .user_agent(concat!(
                    "SteamCounter/",
                    env!("CARGO_PKG_VERSION"),
                    " (personal CLI)"
                ))
                .timeout(timeout)
                .connect_timeout(timeout.min(Duration::from_secs(10)))
                .build()
                .context("Impossibile inizializzare la connessione a SteamCharts")?,
            base_url: "https://steamcharts.com".to_owned(),
        })
    }

    pub fn history(&self, appid: NonZeroU32) -> Result<HistorySnapshot> {
        let url = format!("{}/app/{appid}", self.base_url);
        let html = self.get(&url)?;
        let fetched = Utc::now();
        let mut history = parse_page(&html, appid, fetched)?;
        let chart = self
            .get(&format!("{url}/chart-data.json"))
            .and_then(|body| parse_samples(&body, fetched));
        match chart {
            Ok(points) => {
                history.latest_sample_at = points.last().map(|point| point.at);
                history.today = sample_average(&points, history.today.starts_at, fetched);
                history.last_7_days = sample_average(&points, history.last_7_days.starts_at, fetched);
                history.current_month = sample_average(&points, history.current_month.starts_at, fetched);
                if points.last().is_none_or(|point| fetched - point.at > chrono::Duration::hours(3)) {
                    history.warnings.push("I campioni orari sono assenti o risalgono a oltre tre ore fa.".to_owned());
                }
            }
            Err(error) => history.warnings.push(format!(
                "Medie dai campioni orari non disponibili: {error:#}. Le medie mensili pubblicate restano consultabili."
            )),
        }
        Ok(history)
    }

    fn get(&self, url: &str) -> Result<String> {
        let response = self
            .http
            .get(url)
            .send()
            .map_err(|error| anyhow::Error::new(error.without_url()))
            .context("Connessione a SteamCharts non riuscita")?;
        match response.status().as_u16() {
            403 => bail!("SteamCharts non consente questa richiesta (HTTP 403)"),
            404 => bail!("SteamCharts non ha dati per questo gioco (HTTP 404)"),
            429 => bail!("SteamCharts richiede di attendere prima di riprovare (HTTP 429)"),
            _ => (),
        }
        let response = response
            .error_for_status()
            .map_err(|error| anyhow::Error::new(error.without_url()))
            .context("SteamCharts ha restituito un errore HTTP")?;
        ensure!(
            response
                .content_length()
                .is_none_or(|len| len <= MAX_RESPONSE_BYTES),
            "Risposta SteamCharts troppo grande (limite 2 MiB)"
        );
        let mut bytes = Vec::new();
        response
            .take(MAX_RESPONSE_BYTES + 1)
            .read_to_end(&mut bytes)
            .context("Lettura della risposta SteamCharts non riuscita")?;
        ensure!(
            bytes.len() as u64 <= MAX_RESPONSE_BYTES,
            "Risposta SteamCharts troppo grande (limite 2 MiB)"
        );
        String::from_utf8(bytes).context("Risposta SteamCharts non valida: atteso testo UTF-8")
    }
}

pub fn parse_month(input: &str) -> Result<NaiveDate> {
    ensure!(
        input.len() == 7 && input.as_bytes()[4] == b'-',
        "Usa un mese nel formato YYYY-MM, per esempio 2026-08"
    );
    let date = NaiveDate::parse_from_str(&format!("{input}-01"), "%Y-%m-%d")
        .context("Mese non valido: usa YYYY-MM")?;
    ensure!(
        (2012..=9998).contains(&date.year()),
        "L'anno deve essere compreso tra 2012 e 9998"
    );
    Ok(date)
}

fn next_month(date: NaiveDate) -> NaiveDate {
    let (year, month) = if date.month() == 12 {
        (date.year() + 1, 1)
    } else {
        (date.year(), date.month() + 1)
    };
    NaiveDate::from_ymd_opt(year, month, 1).expect("mese valido")
}

fn midnight(date: NaiveDate) -> DateTime<Utc> {
    date.and_time(NaiveTime::MIN).and_utc()
}

fn selector(css: &str) -> Selector {
    Selector::parse(css).expect("selettore CSS costante valido")
}

fn text(element: ElementRef<'_>) -> String {
    element
        .text()
        .collect::<Vec<_>>()
        .join(" ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn parse_page(html: &str, appid: NonZeroU32, now: DateTime<Utc>) -> Result<HistorySnapshot> {
    let document = Html::parse_document(html);
    let table_selector = selector("table.common-table");
    let header_selector = selector("thead th");
    let table = document
        .select(&table_selector)
        .find(|table| {
            let headers: Vec<_> = table.select(&header_selector).map(text).collect();
            headers == ["Month", "Avg. Players", "Gain", "% Gain", "Peak Players"]
        })
        .context("Tabella delle medie SteamCharts assente o formato modificato")?;
    let mut last_30_days = None;
    let mut months = Vec::new();
    let mut seen_months = HashSet::new();
    for row in table.select(&selector("tbody tr")) {
        let cells: Vec<_> = row.select(&selector("td")).map(text).collect();
        ensure!(
            cells.len() == 5,
            "Riga SteamCharts incompleta o incompatibile"
        );
        let average = cells[1]
            .parse::<f64>()
            .context("Media SteamCharts non numerica")?;
        ensure!(
            average.is_finite() && average >= 0.0,
            "Media SteamCharts non valida"
        );
        let peak = cells[4]
            .parse::<u64>()
            .context("Picco SteamCharts non numerico")?;
        ensure!(
            average <= peak as f64,
            "Media SteamCharts superiore al picco: dati incompatibili"
        );
        let players = PublishedAverage {
            average_players: average,
            peak_players: peak,
        };
        if cells[0] == "Last 30 Days" {
            ensure!(last_30_days.is_none(), "Riga ultimi 30 giorni duplicata");
            last_30_days = Some(players);
        } else {
            let month = NaiveDate::parse_from_str(&format!("1 {}", cells[0]), "%d %B %Y")
                .context("Etichetta del mese SteamCharts non riconosciuta")?;
            ensure!(
                month.year() >= 2012 && month.year() < 9999,
                "Anno SteamCharts non valido"
            );
            ensure!(
                next_month(month) <= now.date_naive(),
                "SteamCharts contiene un mese non ancora completato"
            );
            ensure!(seen_months.insert(month), "Mese SteamCharts duplicato");
            months.push(MonthlyAverage { month, players });
        }
    }
    months.sort_by_key(|row| std::cmp::Reverse(row.month));
    let source_updated_at = document
        .select(&selector("#app-heading abbr.timeago"))
        .next()
        .and_then(|element| element.value().attr("title"))
        .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
        .map(|value| value.with_timezone(&Utc));
    let today = midnight(now.date_naive());
    let month_start = midnight(
        NaiveDate::from_ymd_opt(now.year(), now.month(), 1).expect("data corrente valida"),
    );
    Ok(HistorySnapshot {
        source: "SteamCharts",
        source_url: format!("https://steamcharts.com/app/{appid}"),
        retrieved_at: now,
        source_updated_at,
        latest_sample_at: None,
        last_30_days: last_30_days
            .context("Media degli ultimi 30 giorni assente in SteamCharts")?,
        months,
        today: sample_average(&[], today, now),
        last_7_days: sample_average(&[], now - chrono::Duration::days(7), now),
        current_month: sample_average(&[], month_start, now),
        warnings: Vec::new(),
    })
}

#[derive(Debug)]
struct Sample {
    at: DateTime<Utc>,
    players: u64,
}

fn parse_samples(json: &str, now: DateTime<Utc>) -> Result<Vec<Sample>> {
    let data: Vec<(i64, u64)> =
        serde_json::from_str(json).context("Formato del grafico SteamCharts non riconosciuto")?;
    let cutoff = now - chrono::Duration::days(30);
    let mut previous = None;
    let mut points = Vec::new();
    for (millis, players) in data {
        let at =
            DateTime::from_timestamp_millis(millis).context("Timestamp SteamCharts non valido")?;
        ensure!(
            previous.is_none_or(|prev| at > prev),
            "Campioni SteamCharts duplicati o non ordinati"
        );
        ensure!(
            at <= now + chrono::Duration::minutes(5),
            "Campione SteamCharts nel futuro"
        );
        previous = Some(at);
        // I vecchi picchi giornalieri e mensili osservati sono etichettati alle 00:00 UTC.
        // Scartiamo anche un eventuale campione reale a quell'ora: e ambiguo.
        if at >= cutoff && at.time() != NaiveTime::MIN {
            points.push(Sample { at, players });
        }
    }
    Ok(points)
}

fn sample_average(points: &[Sample], start: DateTime<Utc>, end: DateTime<Utc>) -> SampleAverage {
    let mut area = 0.0;
    let mut covered = 0.0;
    let mut used = HashSet::new();
    for (index, pair) in points.windows(2).enumerate() {
        let gap = (pair[1].at - pair[0].at).num_seconds();
        // Accettiamo solo intervalli compatibili con campioni orari. Non riempiamo buchi.
        if !(MIN_SAMPLE_GAP_SECONDS..=MAX_SAMPLE_GAP_SECONDS).contains(&gap) {
            continue;
        }
        let left = start.max(pair[0].at);
        let right = end.min(pair[1].at);
        if left >= right {
            continue;
        }
        let value_at = |at: DateTime<Utc>| {
            let fraction = (at - pair[0].at).num_seconds() as f64 / gap as f64;
            pair[0].players as f64 + (pair[1].players as f64 - pair[0].players as f64) * fraction
        };
        let seconds = (right - left).num_seconds() as f64;
        area += (value_at(left) + value_at(right)) / 2.0 * seconds;
        covered += seconds;
        used.insert(index);
        used.insert(index + 1);
    }
    let duration = (end - start).num_seconds().max(0) as f64;
    SampleAverage {
        starts_at: start,
        ends_at: end,
        average_players: (covered > 0.0).then(|| area / covered),
        sample_count: used.len(),
        coverage_percent: if duration > 0.0 {
            (covered / duration * 100.0).min(100.0)
        } else {
            0.0
        },
        method: "time_weighted_hourly_samples_estimate",
    }
}

#[cfg(test)]
mod tests;
