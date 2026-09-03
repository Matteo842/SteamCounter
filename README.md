# SteamCounter

Una piccola CLI in Rust per leggere **quanti giocatori sono attivi adesso in un gioco Steam**, partendo dal nome o dall'AppID. Usa direttamente Steam, senza account, chiavi API o dipendenze da SteamDB.

## Avvio rapido

Servono Rust e Cargo **1.88 o successivi** e una connessione Internet. Su Windows serve anche il linker C++ della toolchain Rust MSVC (Visual Studio Build Tools, componente C++).

Da questa cartella:

```powershell
cargo run -- 730
cargo run -- "elden ring"
cargo run -- Counter-Strike 2
```

L'output contiene titolo, AppID, giocatori attivi e ora della lettura in UTC. I nomi con spazi funzionano anche senza virgolette; usa le virgolette se il nome contiene caratteri speciali per la shell.

Per compilare l'eseguibile ottimizzato e usarlo direttamente su Windows:

```powershell
cargo build --release --locked
.\target\release\steamcounter.exe 730
.\target\release\steamcounter.exe "elden ring"
```

Su Linux/macOS l'eseguibile e `./target/release/steamcounter`. In alternativa, `cargo install --path . --locked` installa il comando nella cartella bin di Cargo, da aggiungere al PATH se necessario.

## Ricerca per nome

```powershell
cargo run -- --search portal
```

La ricerca elenca i risultati restituiti dallo Store con il rispettivo AppID. Nella lettura del contatore viene scelto il nome esatto (ignorando maiuscole e spazi ripetuti), oppure l'unico risultato. Se rimangono piu candidati, vengono elencati e il comando termina con un errore: ripeti la richiesta con l'AppID desiderato.

L'**AppID** identifica il gioco: per esempio, in `https://store.steampowered.com/app/730/` e `730`. Lo SteamID di un profilo utente non e un AppID. Per cercare un titolo composto soltanto da cifre, usa `--search`, poi scegli l'AppID.

## JSON e opzioni

```powershell
cargo run --quiet -- 730 --json
cargo run --quiet -- --search portal --json
cargo run -- 730 --timeout 30
cargo run -- --help
```

Esempio di struttura JSON, con valori illustrativi:

```json
{
  "appid": 730,
  "name": "Counter-Strike 2",
  "player_count": 123456,
  "checked_at": "2026-09-03T12:00:00Z"
}
```

`checked_at` e l'istante UTC in cui SteamCounter ha ricevuto il conteggio, non un timestamp fornito da Steam. `name` puo essere `null` se lo Store non fornisce il titolo: la lettura tramite AppID funziona comunque. `--search --json` restituisce invece un array di oggetti con `appid` e `name`, anche vuoto.

Il timeout predefinito e **15 secondi per richiesta**, modificabile da 1 a 120. Il recupero opzionale del titolo tramite AppID attende al massimo 5 secondi. La durata complessiva puo includere piu richieste.

Gli errori vanno su stderr e producono un codice di uscita diverso da zero. Con `--json`, stdout contiene solo il risultato JSON in caso di successo e rimane vuoto in caso di errore. `--quiet` elimina anche i messaggi di compilazione di Cargo. Una risposta mancante o fallita non viene mai convertita in zero giocatori.

## Fonti e limiti

- Conteggio: API pubblica di Valve [`GetNumberOfCurrentPlayers`](https://partner.steamgames.com/doc/webapi/ISteamUserStats#GetNumberOfCurrentPlayers), interrogata su `api.steampowered.com`. Conta i giocatori attivi connessi a Steam; i giocatori offline non sono inclusi.
- Nomi: endpoint pubblici dello Steam Store [`storesearch`](https://store.steampowered.com/api/storesearch/?term=portal&l=english&cc=IT) e [`appdetails`](https://store.steampowered.com/api/appdetails?appids=730&filters=basic&l=english&cc=IT). Sono endpoint non documentati come API stabili da Valve: possono cambiare. La ricerca usa lingua inglese e regione Italia, e non garantisce di trovare titoli rimossi o non disponibili nella regione. In questi casi prova l'AppID direttamente.
- I bundle e i pacchetti vengono esclusi dalla ricerca; DLC, demo e altre applicazioni possono comparire. Per il conteggio del gioco usa l'AppID del **gioco base**. Steam puo non rendere disponibile un conteggio per alcune applicazioni.
- Ogni esecuzione fa una lettura singola. Il dato puo risentire dell'aggiornamento e della cache di Steam. Non sono implementati polling, salvataggio locale o statistiche storiche.

## Sviluppo e prossimi passi

```powershell
cargo fmt --check
cargo test --locked
cargo clippy --all-targets --locked -- -D warnings
```

I test usano risposte HTTP locali: verificano ricerca e selezione del titolo, conteggi, JSON, AppID non validi, indisponibilita dello Store ed errori delle API senza dipendere da Internet.

Il client Steam e il modello `PlayerSnapshot` sono in `src/lib.rs`; la presentazione e gli argomenti sono in `src/main.rs`. Questa separazione consente di riutilizzare il client in una UI o in un raccoglitore.

Per medie mensili e annuali, il passo successivo e un raccoglitore periodico che salvi le letture (per esempio in SQLite), seguito da aggregazioni che gestiscano anche intervalli mancanti. Questa API fornisce il conteggio attuale, non uno storico mensile/annuale: le medie si potranno calcolare sul periodo effettivamente raccolto o su un'altra fonte storica.
