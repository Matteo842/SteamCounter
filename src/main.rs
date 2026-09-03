use std::{
    fmt::Write as _,
    io::{self, Write},
    process::ExitCode,
    time::Duration,
};

use anyhow::{Result, bail};
use clap::Parser;
use steamcounter::{Game, GameQuery, NameMatch, SteamClient, match_name};

#[derive(Parser)]
#[command(
    name = "steamcounter",
    version,
    about = "Mostra i giocatori attivi adesso su Steam. Nessuna chiave API richiesta.",
    after_help = "Esempi:\n  steamcounter 730\n  steamcounter elden ring\n  steamcounter \"Counter-Strike 2\" --json\n  steamcounter --search portal\n\nUsa l'AppID del gioco, non lo SteamID di un utente."
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
    let snapshot = steam.snapshot(appid, name)?;
    if cli.json {
        writeln!(output, "{}", serde_json::to_string_pretty(&snapshot)?)?;
    } else {
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
    }
    Ok(())
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
    let mut formatted = String::new();
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            formatted.push('.');
        }
        formatted.push(digit);
    }
    formatted
}
