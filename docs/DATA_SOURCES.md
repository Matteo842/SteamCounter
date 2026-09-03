# Data sources and methodology

Verified on September 3, 2026. SteamCounter fetches data for a requested game without a browser, API key or permanent collector.

## Providers

| Source | Finding |
| --- | --- |
| Steam | Public `GetNumberOfCurrentPlayers` supplies the current concurrent-player count, not historical averages. |
| SteamCharts | Public game page and chart JSON work without login. The page supplies completed-month averages; the graph supplies recent hourly counts mixed with historical peaks. |
| SteamDB | Its FAQ does not offer an applicable public API and disallows scraping/crawling. Not used. |
| OpenGameStats | Historical API requires a personal key (the tested unauthenticated request returned HTTP 401). Not integrated. |

References: [Valve](https://partner.steamgames.com/doc/webapi/ISteamUserStats#GetNumberOfCurrentPlayers), [SteamCharts About](https://steamcharts.com/about), [SteamDB FAQ](https://steamdb.info/faq/), [OpenGameStats historical API](https://opengamestats.com/en-US/blog/historical-steam-data-api).

## SteamCharts requests

1. `https://steamcharts.com/app/{appid}`: HTML table with `Last 30 Days` and averages for completed calendar months.
2. `https://steamcharts.com/app/{appid}/chart-data.json`: the public chart's array of `[unix_timestamp_ms, count]` pairs.

Initial CS2 responses were about 52 KB of HTML and 22 KB of chart JSON; these are observations, not guaranteed sizes. Each response is capped at 2 MiB. The app downloads no page images, ads or scripts. Requests use an identifiable SteamCounter User-Agent. There are no login sessions or attempts to bypass provider blocks.

These endpoints are not a documented stable API. A changed table, invalid value, duplicate or out-of-order timestamp, or unexpected response is reported as unavailable. Published monthly data remains accessible if only the hourly chart fails.

## Separating counts, peaks and means

Observed chart responses contain older **monthly peaks**, then daily peaks timestamped at exactly midnight UTC, then approximately hourly counts for about the last 30 days. Peaks must not be interpreted as averages.

The app retains only points from the 30 days preceding the download and excludes exact-midnight points as ambiguous. This may also exclude an actual sample at midnight; a small gap is preferable to treating an aggregate peak as a concurrent-player sample. Interpretation of this undocumented format is an inference, not a guarantee from the provider.

For daily, weekly and current-month estimates, the app integrates linearly only between samples 30–90 minutes apart and clips each interval to the requested period. The area is divided by the time actually covered. Longer gaps, missing edges and the time after the latest sample are not filled. Coverage is covered time divided by the requested duration. All boundaries use UTC.

The table's published monthly means are preserved. A yearly estimate is the sum of `monthly_mean * days_in_month` divided by the included days. Full-year CLI and past-year GUI results require 12 months. The current-year GUI summary uses available completed months, explicitly labeled provisional. It excludes the current month. Published rounding and unknown sampling coverage make yearly results estimates.

## Charts

- 48h / 1w: retained hourly counts plotted at their real timestamps.
- 1m: available hourly counts in the selected calendar month; no extension into missing time. If hourly detail has expired, a single bar shows the published monthly mean when available.
- 1y: published means for completed months of the selected year. Points are positioned at the middle of their month; missing months break the line.
- The lower overview uses published monthly averages, not the provider's aggregate peaks.

No real-data view is supplemented with demo points. Demo mode is explicitly labeled throughout the UI.

## Cache and freshness

Opt-in local storage preserves the source responses and their download timestamp in per-AppID JSON files. Cached responses are revalidated before reuse; current-period averages are recalculated for the time of the request. Reuse lasts one hour and can survive restarts. The GUI displays cached/stale status and the original history timestamp separately from the Steam count's timestamp.

On a refresh failure, a previous snapshot can be returned with a stale warning. The cache records a retry pause of at least 15 minutes and respects longer provider retry intervals. Even an initial failure without saved data gets a pause marker. No missing response becomes zero players. With persistence disabled, no history cache is read or written.

The history cache has a 50 MiB limit, removes oldest entries, and does not retain a permanent growing hourly archive. Clear cache only deletes this application's per-AppID history entries. Steam live counts and search results have short in-memory reuse in the GUI; the CLI always requests its live count directly.
