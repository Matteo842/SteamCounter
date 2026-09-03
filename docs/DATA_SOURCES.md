# Fonti dello storico: verifica del 3 settembre 2026

Obiettivo: medie disponibili su richiesta per un singolo gioco, senza browser, account obbligatori, database locale o un raccoglitore sempre acceso.

## Strade verificate

| Fonte | Verifica | Esito |
| --- | --- | --- |
| Steam ufficiale | Endpoint `GetNumberOfCurrentPlayers`, gia in uso | Ottimo per il dato live; non fornisce la media del mese |
| SteamCharts | GET pubblico della pagina di CS2 e del JSON del grafico; secondo grafico verificato con Elden Ring | HTTP 200 senza login o chiave; integrato |
| SteamDB | FAQ su API e raccolta automatica | Non offre attualmente un'API pubblica documentata utilizzabile qui; la FAQ indirizza alle partnership e vieta scraping/crawling. Non implementato |
| OpenGameStats | Documentazione dello storico; GET senza chiave su `players/history?interval=hour&limit=1` | HTTP 401: serve una chiave personale. Alternativa documentata per il futuro, non attivata |

Fonti: [Valve](https://partner.steamgames.com/doc/webapi/ISteamUserStats#GetNumberOfCurrentPlayers), [SteamCharts About](https://steamcharts.com/about), [SteamDB FAQ](https://steamdb.info/faq/), [OpenGameStats storico](https://opengamestats.com/en-US/blog/historical-steam-data-api).

## SteamCharts: cosa viene letto

1. `https://steamcharts.com/app/{appid}`: la tabella HTML contiene `Last 30 Days` e le medie dei mesi completati. Non si scaricano immagini, script o pubblicita.
2. `https://steamcharts.com/app/{appid}/chart-data.json`: array di coppie `[timestamp_unix_ms, conteggio]`, referenziato direttamente dalla pagina pubblica.

Per CS2: pagina 52.289 byte e grafico 22.163 byte nella prova iniziale. Per Elden Ring il grafico era 18.334 byte. Sono dimensioni osservate, non limiti garantiti. L'applicazione impone 2 MiB per risposta e non salva questi dati sul disco.

Non e un'API documentata con garanzie di stabilita. Le richieste usano uno User-Agent identificabile `SteamCounter/<version> (personal CLI)`, senza sessioni, browser o tentativi di aggirare blocchi. HTTP 403, 404 e 429 sono riportati come indisponibilita della fonte.

## Interpretazione dei dati

La tabella e il grafico servono a scopi diversi. Nella risposta di CS2 osservata:

- I valori piu vecchi del grafico corrispondono ai **picchi mensili**, non alle medie della tabella.
- Seguono picchi giornalieri etichettati alle 00:00 UTC.
- Gli ultimi circa 30 giorni contengono letture a distanza approssimativa di un'ora.

Le medie pubblicate della tabella vengono conservate senza ricalcolarle. Non si rinomina `Last 30 Days` come "mese corrente".

Per le stime recenti vengono selezionati solo punti degli ultimi 30 giorni e scartati quelli etichettati esattamente alle 00:00 UTC, potenzialmente aggregati. Si rinuncia anche a un eventuale campione reale a quell'istante, preferendo una piccola lacuna all'inclusione di un picco. Si accettano per il calcolo solo coppie di punti distanti da 30 a 90 minuti.

Il calcolo integra linearmente il conteggio tra ciascuna coppia valida, ritagliando l'intervallo richiesto. La media e l'area risultante divisa per il tempo coperto. Intervalli maggiori, parti del periodo senza campioni e l'intervallo dall'ultima lettura fino alla richiesta non sono riempiti. La copertura e il tempo coperto diviso per la durata richiesta.

Questa e un'inferenza sul formato osservato, non un contratto della fonte. Il programma presenta questi numeri come **stime**; non pretende che coincidano con le medie calcolate internamente da SteamCharts. Se il formato cambia in modo incompatibile, la lettura fallisce con un avviso. Le medie mensili restano disponibili se fallisce soltanto il grafico.

La stima annuale usa la somma di `media_mensile * giorni_del_mese`, divisa per il numero di giorni dell'anno. Servono tutti i 12 mesi. Rimane approssimata per arrotondamenti e copertura dei dati originali non dichiarata dalla fonte; ad esempio il mese di lancio potrebbe essere parziale.

## Limiti del prodotto

- Il conteggio Steam e istantaneo. Anche la media giornaliera pronta proviene dallo storico esterno: non viene inventata da una singola lettura Steam.
- Oggi e mese corrente sono periodi incompleti per definizione; "ultimi 7 giorni" e una finestra mobile.
- Nei mesi da 31 giorni puo mancare l'inizio del mese nei campioni recenti; viene mostrata la copertura effettiva.
- Le medie misurano giocatori contemporanei, non utenti unici giornalieri o mensili.
- Non sono stati creati account, acquistati servizi o inviati messaggi ai fornitori.
