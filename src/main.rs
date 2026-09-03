// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Matteo842
// See LICENSE in the project root for the full terms.

use std::{
    fmt::Write as _,
    io::{self, Write},
    process::ExitCode,
    time::Duration,
};

use anyhow::{Result, bail};
use chrono::NaiveDate;
use clap::Parser;
use serde::Serialize;
use steamcounter::cache::{CacheState, HistoryCache, Settings};
use steamcounter::history::{
    HistorySnapshot, MonthlyAverage, SampleAverage, SteamChartsClient, YearAverage, parse_month,
};
use steamcounter::{Game, GameQuery, NameMatch, PlayerSnapshot, SteamClient, match_name};

#[derive(Parser)]
#[command(
    name = "steamcounter",
    version,
    about = "Show current Steam player counts. No API key required.",
    after_help = "Examples:\n  steamcounter 730\n  steamcounter elden ring --stats\n  steamcounter 730 --month 2026-08 --year 2025\n  steamcounter \"Counter-Strike 2\" --stats --json\n  steamcounter --search portal\n\nUse the game's AppID, not a user's SteamID."
)]
struct Cli {
    /// Game name or Steam AppID
    #[arg(required = true, num_args = 1.., value_name = "GAME")]
    game: Vec<String>,

    /// List Store results and AppIDs without requesting player counts
    #[arg(long)]
    search: bool,

    /// Print JSON for use in scripts
    #[arg(long)]
    json: bool,

    /// Add SteamCharts averages and history, fetched on demand
    #[arg(long, conflicts_with = "search")]
    stats: bool,

    /// Published average for a completed month, YYYY-MM (includes --stats)
    #[arg(long, value_name = "YYYY-MM", value_parser = parse_month, conflicts_with = "search")]
    month: Option<NaiveDate>,

    /// Yearly estimate from 12 published months (includes --stats)
    #[arg(long, value_name = "YYYY", value_parser = clap::value_parser!(i32).range(2012..=9998), conflicts_with = "search")]
    year: Option<i32>,

    /// Maximum timeout for each request, in seconds (1-120)
    #[arg(long, default_value_t = 15, value_parser = clap::value_parser!(u64).range(1..=120))]
    timeout: u64,
    /// Reuse local history for one hour (overrides the GUI setting for this run)
    #[arg(long, conflicts_with = "no_cache")]
    cache: bool,
    /// Skip local history storage for this run
    #[arg(long)]
    no_cache: bool,
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            if error
                .downcast_ref::<io::Error>()
                .is_some_and(|err| err.kind() == io::ErrorKind::BrokenPipe)
            {
                return ExitCode::SUCCESS;
            }
            eprintln!("Error: {error:#}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> Result<()> {
    let input = cli.game.join(" ");
    // Valida prima di inizializzare la rete; --search consente anche titoli numerici.
    let query = if cli.search {
        if input.trim().is_empty() {
            bail!("Enter a name to search.");
        }
        GameQuery::Name(input.trim().to_owned())
    } else {
        GameQuery::parse(&input)?
    };
    let steam = SteamClient::new(Duration::from_secs(cli.timeout))?;
    let mut output = io::stdout().lock();

    if cli.search {
        let games = steam.search(&input)?;
        if cli.json {
            writeln!(output, "{}", serde_json::to_string_pretty(&games)?)?;
        } else if games.is_empty() {
            writeln!(
                output,
                "No results for \"{input}\". Try another name or the AppID."
            )?;
        } else {
            writeln!(output, "{}", format_games(&games))?;
            writeln!(output, "\nFor player counts: steamcounter <APPID>")?;
        }
        return Ok(());
    }

    let (appid, name) = match query {
        GameQuery::AppId(appid) => (appid, None),
        GameQuery::Name(name) => match match_name(&name, steam.search(&name)?) {
            NameMatch::Found(game) => (game.appid, Some(game.name)),
            NameMatch::NotFound => {
                bail!("No results for \"{name}\". Try the full game name or its AppID.")
            }
            NameMatch::Ambiguous(games) => bail!(
                "Multiple results for \"{name}\":\n\n{}\n\nChoose a game using: steamcounter <APPID>",
                format_games(&games)
            ),
        },
    };
    if cli.stats || cli.month.is_some() || cli.year.is_some() {
        return show_stats(&mut output, &steam, appid, name, &cli);
    }
    let snapshot = steam.snapshot(appid, name)?;
    if cli.json {
        writeln!(output, "{}", serde_json::to_string_pretty(&snapshot)?)?;
    } else {
        write_current(&mut output, &snapshot)?;
    }
    Ok(())
}

#[derive(Serialize)]
struct StatsOutput {
    appid: std::num::NonZeroU32,
    current: Option<PlayerSnapshot>,
    history: Option<HistorySnapshot>,
    selected_month: Option<MonthlyAverage>,
    selected_year: Option<YearAverage>,
    warnings: Vec<String>,
}

fn show_stats(
    output: &mut impl Write,
    steam: &SteamClient,
    appid: std::num::NonZeroU32,
    name: Option<String>,
    cli: &Cli,
) -> Result<()> {
    let mut report = StatsOutput {
        appid,
        current: None,
        history: None,
        selected_month: None,
        selected_year: None,
        warnings: Vec::new(),
    };
    match steam.snapshot(appid, name) {
        Ok(current) => report.current = Some(current),
        Err(error) => report
            .warnings
            .push(format!("Current Steam count unavailable: {error:#}")),
    }
    let enabled = if cli.no_cache {
        false
    } else if cli.cache {
        true
    } else {
        match Settings::load() {
            Ok(settings) => settings.cache_enabled,
            Err(error) => {
                report.warnings.push(format!("{error:#}"));
                false
            }
        }
    };
    let cache = HistoryCache::new(enabled)?;
    match SteamChartsClient::new(Duration::from_secs(cli.timeout))
        .and_then(|client| client.history_cached(appid, &cache))
    {
        Ok(history) => {
            if let Some(month) = cli.month {
                report.selected_month = history.month(month).cloned();
                if report.selected_month.is_none() {
                    report.warnings.push(format!("Published average for {} unavailable. For this month, see the estimate and its coverage.", month.format("%Y-%m")));
                }
            }
            if let Some(year) = cli.year {
                let average = history.year(year);
                if average.average_players.is_none() {
                    report.warnings.push(format!(
                        "Yearly average for {year} unavailable: {}/12 completed months available.",
                        average.months_available
                    ));
                }
                report.selected_year = Some(average);
            }
            report.warnings.extend(history.warnings.iter().cloned());
            report.history = Some(history);
        }
        Err(error) => report
            .warnings
            .push(format!("SteamCharts statistics unavailable: {error:#}")),
    }
    if report.current.is_none() && report.history.is_none() {
        bail!("No data source available. {}", report.warnings.join("\n"));
    }
    if cli.json {
        writeln!(output, "{}", serde_json::to_string_pretty(&report)?)?;
        return Ok(());
    }
    if let Some(current) = &report.current {
        write_current(output, current)?;
    } else {
        writeln!(output, "AppID: {appid}\nCurrent players: unavailable")?;
    }
    if let Some(history) = &report.history {
        writeln!(output, "\nAverage concurrent players - SteamCharts")?;
        write_estimate(output, "Today (UTC, as of this request)", &history.today)?;
        write_estimate(output, "Last 7 days", &history.last_7_days)?;
        write_estimate(
            output,
            &format!(
                "Current month ({})",
                history.current_month.starts_at.format("%Y-%m")
            ),
            &history.current_month,
        )?;
        writeln!(
            output,
            "Last 30 days: {} (published average)",
            format_average(history.last_30_days.average_players)
        )?;
        if let Some(month) = report
            .selected_month
            .as_ref()
            .or_else(|| history.months.first())
        {
            writeln!(
                output,
                "Month {}: {} (published average)",
                month.month.format("%Y-%m"),
                format_average(month.players.average_players)
            )?;
        }
        if let Some(year) = &report.selected_year
            && let Some(average) = year.average_players
        {
            writeln!(
                output,
                "Year {}: ~{} (weighted estimate from 12 months)",
                year.year,
                format_average(average)
            )?;
        }
        writeln!(
            output,
            "\n~ = estimate from available data; coverage excludes missing intervals."
        )?;
        if let Some(at) = history.latest_sample_at {
            writeln!(
                output,
                "Latest hourly sample (UTC): {}",
                at.format("%Y-%m-%d %H:%M:%S")
            )?;
        }
        if let Some(at) = history.source_updated_at {
            writeln!(
                output,
                "Update reported by source (UTC): {}",
                at.format("%Y-%m-%d %H:%M:%S")
            )?;
        }
        writeln!(output, "Source: {}", history.source_url)?;
        if history.cache_state != CacheState::Network {
            writeln!(
                output,
                "History: {} cache, fetched {} UTC",
                if history.cache_state == CacheState::Stale {
                    "stale"
                } else {
                    "fresh"
                },
                history.retrieved_at.format("%Y-%m-%d %H:%M")
            )?;
        }
    }
    for warning in report.warnings {
        eprintln!("Warning: {warning}");
    }
    Ok(())
}

fn write_current(output: &mut impl Write, snapshot: &PlayerSnapshot) -> Result<()> {
    writeln!(
        output,
        "{}",
        snapshot.name.as_deref().unwrap_or("Name unavailable")
    )?;
    writeln!(output, "AppID:           {}", snapshot.appid)?;
    writeln!(
        output,
        "Current players: {}",
        format_count(snapshot.player_count)
    )?;
    writeln!(
        output,
        "Checked (UTC):    {}",
        snapshot.checked_at.format("%Y-%m-%d %H:%M:%S")
    )?;
    Ok(())
}

fn write_estimate(output: &mut impl Write, label: &str, average: &SampleAverage) -> Result<()> {
    if let Some(value) = average.average_players {
        writeln!(
            output,
            "{label}: ~{} ({:.1}% coverage, {} samples)",
            format_average(value),
            average.coverage_percent,
            average.sample_count
        )?;
    } else {
        writeln!(output, "{label}: unavailable (insufficient hourly samples)")?;
    }
    Ok(())
}

fn format_average(value: f64) -> String {
    let number = format!("{value:.2}");
    let (integer, decimal) = number
        .split_once('.')
        .expect("media finita con due decimali");
    format!("{}.{decimal}", group_digits(integer))
}

fn format_games(games: &[Game]) -> String {
    let mut output = format!("{:<12} {}", "APPID", "TITLE");
    for game in games {
        let _ = write!(output, "\n{:<12} {}", game.appid, game.name);
    }
    output
}

fn format_count(count: u64) -> String {
    let digits = count.to_string();
    group_digits(&digits)
}

fn group_digits(digits: &str) -> String {
    let mut formatted = String::new();
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            formatted.push(',');
        }
        formatted.push(digit);
    }
    formatted
}
