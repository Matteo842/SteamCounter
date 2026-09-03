# SteamCounter

Una piccola CLI in Rust per leggere **quanti giocatori sono attivi adesso in un gioco Steam**, partendo dal nome o dall'AppID. Con `--stats` aggiunge le medie da SteamCharts. Non servono account, chiavi API, browser, database o un server sempre acceso.

## Avvio rapido

Servono Rust e Cargo **1.88 o successivi** e una connessione Internet. Su Windows serve anche il linker C++ della toolchain Rust MSVC (Visual Studio Build Tools, componente C++).

Da questa cartella:

```powershell
cargo run -- 730
cargo run -- "elden ring"
cargo run -- Counter-Strike 2
cargo run -- "elden ring" --stats
```

L'output contiene titolo, AppID, giocatori attivi e ora della lettura in UTC. I nomi con spazi funzionano anche senza virgolette; usa le virgolette se il nome contiene caratteri speciali per la shell.

Per compilare l'eseguibile ottimizzato e usarlo direttamente su Windows:

```powershell
cargo build --release --locked
.\target\release\steamcounter.exe 730
.\target\release\steamcounter.exe "elden ring"
```

Su Linux/macOS l'eseguibile e `./target/release/steamcounter`. In alternativa, `cargo install --path . --locked` installa il comando nella cartella bin di Cargo, da aggiungere al PATH se necessario.

L'eseguibile Windows si puo copiare e usare da solo. `target` contiene anche i file temporanei e le dipendenze della compilazione: questi non servono per eseguire l'applicazione. Durante l'uso non vengono scaricati immagini, pubblicita o script del sito e non viene salvato un archivio sul disco.

## Ricerca per nome

```powershell
cargo run -- --search portal
```

La ricerca elenca i risultati restituiti dallo Store con il rispettivo AppID. Nella lettura del contatore viene scelto il nome esatto (ignorando maiuscole e spazi ripetuti), oppure l'unico risultato. Se rimangono piu candidati, vengono elencati e il comando termina con un errore: ripeti la richiesta con l'AppID desiderato.

L'**AppID** identifica il gioco: per esempio, in `https://store.steampowered.com/app/730/` e `730`. Lo SteamID di un profilo utente non e un AppID. Per cercare un titolo composto soltanto da cifre, usa `--search`, poi scegli l'AppID.

## Medie e storico su richiesta

```powershell
.\target\release\steamcounter.exe 730 --stats
.\target\release\steamcounter.exe "elden ring" --month 2026-08
.\target\release\steamcounter.exe 730 --year 2025
.\target\release\steamcounter.exe 730 --stats --json
```

`--month` e `--year` includono automaticamente le statistiche. Tutti i periodi del nostro calcolo usano UTC.

| Dato | Origine e significato |
| --- | --- |
| Giocatori attivi | Conteggio attuale ottenuto direttamente da Steam |
| Oggi | Stima dai campioni orari SteamCharts, da mezzanotte UTC alla richiesta |
| Ultimi 7 giorni | Stima dai campioni orari SteamCharts, su una finestra mobile di 168 ore |
| Mese in corso | Stima dai campioni orari disponibili dal primo del mese alla richiesta |
| Ultimi 30 giorni | Media pubblicata da SteamCharts; non equivale al mese di calendario |
| Mese completato | Media della tabella SteamCharts, selezionabile con `--month YYYY-MM` |
| Anno completato | Stima dalle 12 medie mensili, ponderate per i giorni dei mesi (anche bisestili) |

Le stime sono indicate con `~`. Per giorno, settimana e mese corrente mostriamo anche la percentuale del periodo coperta dai campioni. Si interpola linearmente solo tra letture distanti da 30 a 90 minuti: i buchi maggiori e le estremita senza dati rimangono esclusi, non diventano zero. Il risultato descrive il **tempo coperto**, non promette una media esatta sull'intero intervallo se la copertura e incompleta.

Il grafico SteamCharts contiene circa 30 giorni di campioni orari recenti e picchi aggregati per i periodi piu vecchi. Questi picchi non sono medie: il programma li esclude dai calcoli. Si tratta di un formato non documentato come API stabile; i risultati recenti sono stime, non le medie ufficiali di SteamCharts. Il 31 del mese i primi campioni del mese possono gia essere fuori dalla finestra disponibile: la copertura lo segnala.

La media annuale viene mostrata soltanto con tutti i 12 mesi presenti ed e comunque una stima: le medie mensili pubblicate sono arrotondate e non dichiarano il numero di campioni sottostanti. Un anno incompleto, compreso quello in corso, risulta non disponibile; non viene sostituito con una media di pochi mesi.

Il conteggio live non e una media giornaliera. Per avere una media giornaliera pronta senza tenere acceso il programma, anche questa viene ricavata dai campioni di SteamCharts. Non raccogliamo dati in background.

Con `--stats` vengono fatte due piccole richieste aggiuntive, solo per il gioco scelto: tabella delle medie e JSON del grafico. Nella prova con CS2 erano circa **75 KB complessivi**. Nessuna cache persistente; limite di 2 MiB per risposta. Evita aggiornamenti continui: la fonte storica si aggiorna circa ogni ora.

Se una fonte fallisce, i dati dell'altra restano disponibili con un avviso. Se fallisce solo il grafico, rimangono le medie mensili pubblicate. La modalita statistiche termina con successo se almeno una fonte e disponibile; in caso di risultato parziale occorre leggere gli avvisi. Senza alcuna fonte disponibile termina con un errore.

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

`--stats --json` restituisce un oggetto con `appid`, `current`, `history`, `selected_month`, `selected_year` e `warnings`. `history` contiene fonte, ora del download, aggiornamento dichiarato dalla fonte, ultimo campione, medie recenti e tabella dei mesi. Le medie mancanti sono `null`; una media numerica `0` indica un valore effettivamente disponibile. Gli avvisi sono inclusi nel JSON. Senza `--stats` la struttura JSON originale resta invariata.

Il timeout predefinito e **15 secondi per richiesta**, modificabile da 1 a 120. Il recupero opzionale del titolo tramite AppID attende al massimo 5 secondi. La durata complessiva puo includere piu richieste.

Gli errori fatali vanno su stderr e producono un codice di uscita diverso da zero. Con `--json`, stdout contiene solo il risultato JSON in caso di successo e rimane vuoto in caso di errore fatale. `--quiet` elimina anche i messaggi di compilazione di Cargo. Una risposta mancante o fallita non viene mai convertita in zero giocatori.

## Fonti e limiti

- Conteggio: API pubblica di Valve [`GetNumberOfCurrentPlayers`](https://partner.steamgames.com/doc/webapi/ISteamUserStats#GetNumberOfCurrentPlayers), interrogata su `api.steampowered.com`. Conta i giocatori attivi connessi a Steam; i giocatori offline non sono inclusi.
- Nomi: endpoint pubblici dello Steam Store [`storesearch`](https://store.steampowered.com/api/storesearch/?term=portal&l=english&cc=IT) e [`appdetails`](https://store.steampowered.com/api/appdetails?appids=730&filters=basic&l=english&cc=IT). Sono endpoint non documentati come API stabili da Valve: possono cambiare. La ricerca usa lingua inglese e regione Italia, e non garantisce di trovare titoli rimossi o non disponibili nella regione. In questi casi prova l'AppID direttamente.
- I bundle e i pacchetti vengono esclusi dalla ricerca; DLC, demo e altre applicazioni possono comparire. Per il conteggio del gioco usa l'AppID del **gioco base**. Steam puo non rendere disponibile un conteggio per alcune applicazioni.
- Storico: tabella pubblica di [SteamCharts](https://steamcharts.com/app/730) e JSON usato dal suo grafico. Gli endpoint non sono un'API ufficiale stabile e potrebbero cambiare o diventare indisponibili. Le fonti provate e le scelte del calcolo sono descritte in [docs/DATA_SOURCES.md](docs/DATA_SOURCES.md).
- Ogni esecuzione fa una lettura su richiesta. Il dato puo risentire dell'aggiornamento e della cache delle fonti. Non sono implementati polling o salvataggio locale.

## Sviluppo e prossimi passi

```powershell
cargo fmt --check
cargo test --locked
cargo clippy --all-targets --locked -- -D warnings
```

I test usano risposte HTTP locali e dati sintetici: verificano ricerca e selezione del titolo, conteggi, JSON, AppID non validi, errori delle fonti, parsing delle medie, esclusione dei picchi, copertura temporale e ponderazione annuale senza dipendere da Internet.

Il client Steam e il modello `PlayerSnapshot` sono in `src/lib.rs`; il provider SteamCharts e in `src/history.rs`; la presentazione e gli argomenti sono in `src/main.rs`. Questa separazione consente di riutilizzare i dati nella futura UI egui/eframe.

La direzione del progetto e una piccola applicazione locale che interroga fonti esistenti. Non e necessario costruire un servizio di raccolta o replicare il database di SteamDB. Un eventuale provider API documentato puo essere aggiunto in futuro mantenendo esplicita la provenienza delle medie.
