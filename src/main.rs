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
use steamcounter::history::{
    HistorySnapshot, MonthlyAverage, SampleAverage, SteamChartsClient, YearAverage, parse_month,
};
use steamcounter::{Game, GameQuery, NameMatch, PlayerSnapshot, SteamClient, match_name};

#[derive(Parser)]
#[command(
    name = "steamcounter",
    version,
    about = "Mostra i giocatori attivi adesso su Steam. Nessuna chiave API richiesta.",
    after_help = "Esempi:\n  steamcounter 730\n  steamcounter elden ring --stats\n  steamcounter 730 --month 2026-08 --year 2025\n  steamcounter \"Counter-Strike 2\" --stats --json\n  steamcounter --search portal\n\nUsa l'AppID del gioco, non lo SteamID di un utente."
)]
struct Cli {
    /// Nome del gioco oppure AppID Steam
    #[arg(required = true, num_args = 1.., value_name = "GIOCO")]
    game: Vec<String>,

    /// Elenca i risultati dello Store e i loro AppID, senza leggere il contatore
    #[arg(long)]
    search: bool,

    /// Stampa JSON, utile per script e future raccolte dati
    #[arg(long)]
    json: bool,

    /// Aggiunge medie e storico da SteamCharts, scaricati solo su richiesta
    #[arg(long, conflicts_with = "search")]
    stats: bool,

    /// Media pubblicata di un mese completato, formato YYYY-MM (include --stats)
    #[arg(long, value_name = "YYYY-MM", value_parser = parse_month, conflicts_with = "search")]
    month: Option<NaiveDate>,

    /// Stima annuale dai 12 mesi pubblicati (include --stats)
    #[arg(long, value_name = "YYYY", value_parser = clap::value_parser!(i32).range(2012..=9998), conflicts_with = "search")]
    year: Option<i32>,

    /// Timeout massimo di ciascuna richiesta, in secondi (1-120)
    #[arg(long, default_value_t = 15, value_parser = clap::value_parser!(u64).range(1..=120))]
    timeout: u64,
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
            eprintln!("Errore: {error:#}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> Result<()> {
    let input = cli.game.join(" ");
    // Valida prima di inizializzare la rete; --search consente anche titoli numerici.
    let query = if cli.search {
        if input.trim().is_empty() {
            bail!("Inserisci un nome da cercare.");
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
                "Nessun risultato per \"{input}\". Prova un altro nome oppure l'AppID."
            )?;
        } else {
            writeln!(output, "{}", format_games(&games))?;
            writeln!(output, "\nPer il contatore: steamcounter <APPID>")?;
        }
        return Ok(());
    }

    let (appid, name) = match query {
        GameQuery::AppId(appid) => (appid, None),
        GameQuery::Name(name) => match match_name(&name, steam.search(&name)?) {
            NameMatch::Found(game) => (game.appid, Some(game.name)),
            NameMatch::NotFound => bail!(
                "Nessun risultato per \"{name}\". Prova il nome completo oppure l'AppID del gioco."
            ),
            NameMatch::Ambiguous(games) => bail!(
                "Piu risultati per \"{name}\":\n\n{}\n\nScegli il gioco usando: steamcounter <APPID>",
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
        Err(error) => report.warnings.push(format!(
            "Conteggio Steam attuale non disponibile: {error:#}"
        )),
    }
    match SteamChartsClient::new(Duration::from_secs(cli.timeout))
        .and_then(|client| client.history(appid))
    {
        Ok(history) => {
            if let Some(month) = cli.month {
                report.selected_month = history.month(month).cloned();
                if report.selected_month.is_none() {
                    report.warnings.push(format!("Media pubblicata per {} non disponibile. Per il mese in corso consulta la stima e la sua copertura.", month.format("%Y-%m")));
                }
            }
            if let Some(year) = cli.year {
                let average = history.year(year);
                if average.average_players.is_none() {
                    report.warnings.push(format!(
                        "Media annuale {year} non disponibile: presenti {}/12 mesi completati.",
                        average.months_available
                    ));
                }
                report.selected_year = Some(average);
            }
            report.warnings.extend(history.warnings.iter().cloned());
            report.history = Some(history);
        }
        Err(error) => report.warnings.push(format!(
            "Statistiche SteamCharts non disponibili: {error:#}"
        )),
    }
    if report.current.is_none() && report.history.is_none() {
        bail!("Nessuna fonte disponibile. {}", report.warnings.join("\n"));
    }
    if cli.json {
        writeln!(output, "{}", serde_json::to_string_pretty(&report)?)?;
        return Ok(());
    }
    if let Some(current) = &report.current {
        write_current(output, current)?;
    } else {
        writeln!(output, "AppID: {appid}\nGiocatori attivi: non disponibili")?;
    }
    if let Some(history) = &report.history {
        writeln!(output, "\nMedie dei giocatori contemporanei - SteamCharts")?;
        write_estimate(output, "Oggi (UTC, fino alla lettura)", &history.today)?;
        write_estimate(output, "Ultimi 7 giorni", &history.last_7_days)?;
        write_estimate(
            output,
            &format!(
                "Mese in corso ({})",
                history.current_month.starts_at.format("%Y-%m")
            ),
            &history.current_month,
        )?;
        writeln!(
            output,
            "Ultimi 30 giorni: {} (media pubblicata)",
            format_average(history.last_30_days.average_players)
        )?;
        if let Some(month) = report
            .selected_month
            .as_ref()
            .or_else(|| history.months.first())
        {
            writeln!(
                output,
                "Mese {}: {} (media pubblicata)",
                month.month.format("%Y-%m"),
                format_average(month.players.average_players)
            )?;
        }
        if let Some(year) = &report.selected_year
            && let Some(average) = year.average_players
        {
            writeln!(
                output,
                "Anno {}: ~{} (stima ponderata dai 12 mesi)",
                year.year,
                format_average(average)
            )?;
        }
        writeln!(
            output,
            "\n~ = stima sui dati disponibili; la copertura esclude intervalli mancanti."
        )?;
        if let Some(at) = history.latest_sample_at {
            writeln!(
                output,
                "Ultimo campione orario (UTC): {}",
                at.format("%Y-%m-%d %H:%M:%S")
            )?;
        }
        if let Some(at) = history.source_updated_at {
            writeln!(
                output,
                "Aggiornamento indicato dalla fonte (UTC): {}",
                at.format("%Y-%m-%d %H:%M:%S")
            )?;
        }
        writeln!(output, "Fonte: {}", history.source_url)?;
    }
    for warning in report.warnings {
        eprintln!("Avviso: {warning}");
    }
    Ok(())
}

fn write_current(output: &mut impl Write, snapshot: &PlayerSnapshot) -> Result<()> {
    writeln!(
        output,
        "{}",
        snapshot.name.as_deref().unwrap_or("Nome non disponibile")
    )?;
    writeln!(output, "AppID:           {}", snapshot.appid)?;
    writeln!(
        output,
        "Giocatori attivi: {}",
        format_count(snapshot.player_count)
    )?;
    writeln!(
        output,
        "Rilevato (UTC):   {}",
        snapshot.checked_at.format("%Y-%m-%d %H:%M:%S")
    )?;
    Ok(())
}

fn write_estimate(output: &mut impl Write, label: &str, average: &SampleAverage) -> Result<()> {
    if let Some(value) = average.average_players {
        writeln!(
            output,
            "{label}: ~{} (copertura {:.1}%, {} campioni)",
            format_average(value),
            average.coverage_percent,
            average.sample_count
        )?;
    } else {
        writeln!(
            output,
            "{label}: non disponibile (campioni orari insufficienti)"
        )?;
    }
    Ok(())
}

fn format_average(value: f64) -> String {
    let number = format!("{value:.2}");
    let (integer, decimal) = number
        .split_once('.')
        .expect("media finita con due decimali");
    format!("{},{decimal}", group_digits(integer))
}

fn format_games(games: &[Game]) -> String {
    let mut output = format!("{:<12} {}", "APPID", "TITOLO");
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
            formatted.push('.');
        }
        formatted.push(digit);
    }
    formatted
}
