// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Matteo842
// See LICENSE in the project root for the full terms.

//! Accesso a Steam separato dalla CLI, riutilizzabile per storico e interfacce future.

pub mod cache;
pub mod history;
pub mod series;

#[cfg(feature = "gui")]
pub mod gui;

use std::{collections::HashMap, num::NonZeroU32, time::Duration};

use anyhow::{Context, Result, bail};
use chrono::{DateTime, Utc};
use reqwest::{StatusCode, blocking::Client};
use serde::{Deserialize, Serialize, de::DeserializeOwned};

const PLAYERS_URL: &str =
    "https://api.steampowered.com/ISteamUserStats/GetNumberOfCurrentPlayers/v1/";
const SEARCH_URL: &str = "https://store.steampowered.com/api/storesearch/";
const DETAILS_URL: &str = "https://store.steampowered.com/api/appdetails";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Game {
    pub appid: NonZeroU32,
    pub name: String,
}

/// Una lettura puntuale: checked_at indica quando abbiamo ricevuto il conteggio.
#[derive(Debug, Clone, Serialize)]
pub struct PlayerSnapshot {
    pub appid: NonZeroU32,
    pub name: Option<String>,
    pub player_count: u64,
    pub checked_at: DateTime<Utc>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum GameQuery {
    AppId(NonZeroU32),
    Name(String),
}

impl GameQuery {
    pub fn parse(input: &str) -> Result<Self> {
        let input = input.trim();
        if input.is_empty() {
            bail!("Enter a game name or its Steam AppID.");
        }
        if input.bytes().all(|byte| byte.is_ascii_digit()) {
            let appid = input.parse::<NonZeroU32>().context(
                "Invalid AppID: use a number from 1 to 4294967295, not a user's SteamID.",
            )?;
            Ok(Self::AppId(appid))
        } else {
            Ok(Self::Name(input.to_owned()))
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum NameMatch {
    Found(Game),
    Ambiguous(Vec<Game>),
    NotFound,
}

/// Sceglie solo un risultato univoco, privilegiando il nome esatto.
pub fn match_name(query: &str, games: Vec<Game>) -> NameMatch {
    let normalized = normalize_name(query);
    let mut exact = games
        .iter()
        .filter(|game| normalize_name(&game.name) == normalized);
    if let Some(game) = exact.next()
        && exact.next().is_none()
    {
        return NameMatch::Found(game.clone());
    }
    match games.len() {
        0 => NameMatch::NotFound,
        1 => NameMatch::Found(games.into_iter().next().expect("un risultato")),
        _ => NameMatch::Ambiguous(games),
    }
}

fn normalize_name(name: &str) -> String {
    name.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

pub struct SteamClient {
    http: Client,
    players_url: String,
    search_url: String,
    details_url: String,
    timeout: Duration,
}

impl SteamClient {
    pub fn new(timeout: Duration) -> Result<Self> {
        let http = Client::builder()
            .user_agent(concat!("SteamCounter/", env!("CARGO_PKG_VERSION")))
            .timeout(timeout)
            .connect_timeout(timeout.min(Duration::from_secs(10)))
            .build()
            .context("Could not initialize the Steam connection")?;
        Ok(Self {
            http,
            players_url: PLAYERS_URL.to_owned(),
            search_url: SEARCH_URL.to_owned(),
            details_url: DETAILS_URL.to_owned(),
            timeout,
        })
    }

    /// Ricerca pubblica dello Store: non richiede una chiave API.
    pub fn search(&self, query: &str) -> Result<Vec<Game>> {
        if query.trim().is_empty() {
            bail!("The search name cannot be empty.");
        }
        let response: SearchResponse = self.get_json(
            &self.search_url,
            &[("term", query), ("l", "english"), ("cc", "IT")],
            self.timeout,
        )?;
        let mut games = Vec::new();
        for item in response.items {
            // Gli ID di bundle e pacchetti non sono AppID e non vanno interrogati.
            if item.kind == "app"
                && let Some(appid) = NonZeroU32::new(item.id)
                && !item.name.trim().is_empty()
                && !games.iter().any(|game: &Game| game.appid == appid)
            {
                games.push(Game {
                    appid,
                    name: item.name,
                });
            }
        }
        Ok(games)
    }

    /// Il fallimento del nome opzionale non impedisce la lettura tramite AppID.
    pub fn snapshot(
        &self,
        appid: NonZeroU32,
        known_name: Option<String>,
    ) -> Result<PlayerSnapshot> {
        let response: PlayersResponse = self.get_json(
            &self.players_url,
            &[("appid", &appid.to_string())],
            self.timeout,
        )?;
        if response.response.result != 1 {
            bail!(
                "Steam has no count for AppID {appid} (code {}). Check that this is the base game's AppID.",
                response.response.result
            );
        }
        let player_count = response
            .response
            .player_count
            .context("Steam's response does not contain a player count. Try again later.")?;
        let checked_at = Utc::now();
        let name = known_name.or_else(|| self.app_name(appid).ok().flatten());
        Ok(PlayerSnapshot {
            appid,
            name,
            player_count,
            checked_at,
        })
    }

    fn app_name(&self, appid: NonZeroU32) -> Result<Option<String>> {
        let key = appid.to_string();
        let mut response: HashMap<String, AppDetails> = self.get_json(
            &self.details_url,
            &[
                ("appids", &key),
                ("filters", "basic"),
                ("l", "english"),
                ("cc", "IT"),
            ],
            self.timeout.min(Duration::from_secs(5)),
        )?;
        Ok(response
            .remove(&key)
            .filter(|details| details.success)
            .and_then(|details| details.data)
            .map(|data| data.name)
            .filter(|name| !name.trim().is_empty()))
    }

    fn get_json<T: DeserializeOwned>(
        &self,
        url: &str,
        query: &[(&str, &str)],
        timeout: Duration,
    ) -> Result<T> {
        let response = self
            .http
            .get(url)
            .query(query)
            .timeout(timeout)
            .send()
            .map_err(|error| {
                let context = if error.is_timeout() {
                    "Steam timed out. Try again later or increase --timeout."
                } else {
                    "Could not connect to Steam. Check your connection and try again."
                };
                anyhow::Error::new(error.without_url()).context(context)
            })?;
        if response.status() == StatusCode::TOO_MANY_REQUESTS {
            bail!("Steam received too many requests (HTTP 429). Wait before trying again.");
        }
        let response = response
            .error_for_status()
            .map_err(|error| anyhow::Error::new(error.without_url()))
            .context("Steam returned an HTTP error")?;
        response
            .json()
            .map_err(|error| anyhow::Error::new(error.without_url()))
            .context("Steam returned invalid or incompatible JSON")
    }
}

#[derive(Deserialize)]
struct PlayersResponse {
    response: PlayersData,
}

#[derive(Deserialize)]
struct PlayersData {
    result: u32,
    // L'assenza del campo e un errore, non equivale a zero giocatori.
    player_count: Option<u64>,
}

#[derive(Deserialize)]
struct SearchResponse {
    items: Vec<SearchItem>,
}

#[derive(Deserialize)]
struct SearchItem {
    #[serde(rename = "type")]
    kind: String,
    id: u32,
    name: String,
}

#[derive(Deserialize)]
struct AppDetails {
    success: bool,
    data: Option<AppData>,
}

#[derive(Deserialize)]
struct AppData {
    name: String,
}

#[cfg(test)]
mod tests;
