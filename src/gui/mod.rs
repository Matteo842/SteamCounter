// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Matteo842
// See LICENSE in the project root for the full terms.

mod chart;
mod data;
mod licenses;
#[cfg(feature = "gui-preview")]
mod preview;
mod style;

use std::{
    sync::{Arc, Mutex, mpsc},
    time::Duration,
};

use chrono::{Datelike, NaiveDate, Utc};
use clap::Parser;
use eframe::egui::{
    self, Align, Align2, Color32, Context, FontId, Key, Layout, Rect, RichText, Sense, Stroke, Ui,
    UiBuilder, Vec2, pos2, vec2,
};

use crate::Game;
use crate::cache::{CacheState, HistoryCache, Settings};
use chart::ChartRange;
use data::{DashboardData, Loaded, WorkerMessage};
use style::*;

#[derive(Parser)]
#[command(name = "steamcounter-gui", version, about = "SteamCounter desktop")]
struct Options {
    /// Open an offline development preview with example data
    #[cfg(feature = "gui-preview")]
    #[arg(long, hide = true)]
    demo: bool,
    /// Search for a game on startup
    #[arg(long)]
    game: Option<String>,
    #[arg(long, hide = true)]
    compact: bool,
    #[cfg(feature = "gui-preview")]
    #[arg(long, hide = true, value_parser = ["48h", "1w", "1m", "1y"])]
    preview_range: Option<String>,
    #[cfg(feature = "gui-preview")]
    #[arg(long, hide = true, value_parser = crate::history::parse_month)]
    preview_month: Option<NaiveDate>,
    #[cfg(feature = "gui-preview")]
    #[arg(long, hide = true, value_parser = clap::value_parser!(i32).range(2012..=9998))]
    preview_year: Option<i32>,
    #[cfg(feature = "gui-preview")]
    #[arg(long, hide = true)]
    preview_settings: bool,
    #[cfg(feature = "gui-preview")]
    #[arg(long, hide = true, value_parser = ["gpl", "third-party"])]
    preview_licenses: Option<String>,
}

pub fn run() -> eframe::Result {
    let options = Options::parse();
    let size = if options.compact {
        [900.0, 640.0]
    } else {
        [1060.0, 740.0]
    };
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("SteamCounter")
            .with_inner_size(size)
            .with_min_inner_size([900.0, 640.0])
            .with_icon(app_icon()),
        centered: true,
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    };
    eframe::run_native(
        "SteamCounter",
        native_options,
        Box::new(move |context| Ok(Box::new(SteamCounterApp::new(&context.egui_ctx, options)))),
    )
}

struct SteamCounterApp {
    query: String,
    has_searched: bool,
    focus_search: bool,
    busy: bool,
    data: Option<DashboardData>,
    candidates: Vec<Game>,
    error: Option<String>,
    selected_month: NaiveDate,
    selected_year: i32,
    range: ChartRange,
    request_id: u64,
    sender: mpsc::Sender<WorkerMessage>,
    receiver: mpsc::Receiver<WorkerMessage>,
    settings: Settings,
    settings_open: bool,
    licenses: licenses::LicenseViewer,
    settings_message: Option<String>,
    cache_size: u64,
    session: Arc<Mutex<data::Session>>,
    pending_requests: usize,
    #[cfg(feature = "gui-preview")]
    preview: Option<preview::Capture>,
}

impl SteamCounterApp {
    fn new(ctx: &Context, options: Options) -> Self {
        configure(ctx);
        let now = Utc::now();
        let (sender, receiver) = mpsc::channel();
        let (settings, settings_message) = match Settings::load() {
            Ok(settings) => (settings, None),
            Err(error) => (Settings::default(), Some(format!("{error:#}"))),
        };
        let mut app = Self {
            query: String::new(),
            has_searched: false,
            focus_search: true,
            busy: false,
            data: None,
            candidates: Vec::new(),
            error: None,
            selected_month: NaiveDate::from_ymd_opt(now.year(), now.month(), 1).unwrap(),
            selected_year: now.year(),
            range: ChartRange::Month,
            request_id: 0,
            sender,
            receiver,
            settings,
            settings_open: false,
            licenses: licenses::LicenseViewer::default(),
            settings_message,
            cache_size: 0,
            session: Arc::new(Mutex::new(data::Session::default())),
            pending_requests: 0,
            #[cfg(feature = "gui-preview")]
            preview: preview::Capture::from_env(),
        };
        #[cfg(feature = "gui-preview")]
        if options.demo {
            app.open_demo();
        }
        if !app.has_searched
            && let Some(game) = options.game
        {
            app.query = game;
            app.search(ctx, None);
        }
        #[cfg(feature = "gui-preview")]
        {
            if let Some(month) = options.preview_month {
                app.selected_month = month;
            }
            if let Some(year) = options.preview_year {
                app.selected_year = year;
            }
            if let Some(range) = options.preview_range {
                app.range = match range.as_str() {
                    "48h" => ChartRange::Hours,
                    "1w" => ChartRange::Week,
                    "1y" => ChartRange::Year,
                    _ => ChartRange::Month,
                };
            }
            app.settings_open = options.preview_settings;
            if let Some(licenses) = options.preview_licenses {
                app.licenses.open(licenses == "third-party");
            }
            if app.settings_open {
                app.cache_size = HistoryCache::new(true)
                    .and_then(|cache| cache.size_bytes())
                    .unwrap_or(0);
            }
        }
        app
    }

    #[cfg(feature = "gui-preview")]
    fn open_demo(&mut self) {
        self.request_id += 1;
        self.has_searched = true;
        self.focus_search = false;
        self.busy = false;
        self.error = None;
        self.candidates.clear();
        self.data = Some(DashboardData::demo());
        self.query.clear();
        let now = Utc::now();
        self.selected_month = NaiveDate::from_ymd_opt(now.year(), now.month(), 1).unwrap();
        self.selected_year = now.year();
        self.range = ChartRange::Month;
    }

    fn home(&mut self) {
        self.request_id += 1;
        self.has_searched = false;
        self.focus_search = true;
        self.busy = false;
        self.data = None;
        self.candidates.clear();
        self.error = None;
        self.query.clear();
    }

    fn search(&mut self, ctx: &Context, selected: Option<Game>) {
        if self.busy {
            return;
        }
        if self.query.trim().is_empty() && selected.is_none() {
            return;
        }
        self.request_id += 1;
        self.has_searched = true;
        self.focus_search = false;
        self.busy = true;
        self.pending_requests += 1;
        self.data = None;
        self.candidates.clear();
        self.error = None;
        let now = Utc::now();
        self.selected_month = NaiveDate::from_ymd_opt(now.year(), now.month(), 1).unwrap();
        self.selected_year = now.year();
        let cache = match HistoryCache::new(self.settings.cache_enabled) {
            Ok(cache) => cache,
            Err(error) => {
                self.settings_message = Some(format!("{error:#}"));
                HistoryCache::disabled()
            }
        };
        data::spawn(
            self.request_id,
            self.query.trim().to_owned(),
            selected,
            self.sender.clone(),
            ctx.clone(),
            cache,
            self.session.clone(),
        );
    }

    fn receive(&mut self) {
        while let Ok(message) = self.receiver.try_recv() {
            self.apply_message(message);
        }
    }

    fn apply_message(&mut self, message: WorkerMessage) {
        self.pending_requests = self.pending_requests.saturating_sub(1);
        if message.id != self.request_id {
            return;
        }
        self.busy = false;
        match message.result {
            Ok(Loaded::Dashboard(data)) => {
                self.settings
                    .recent_games
                    .retain(|game| game.appid != data.appid);
                self.settings.recent_games.insert(
                    0,
                    Game {
                        appid: data.appid,
                        name: data.name.clone(),
                    },
                );
                self.settings.recent_games.truncate(3);
                if let Err(error) = self.settings.save() {
                    self.settings_message =
                        Some(format!("Could not save recent searches: {error:#}"));
                }
                self.data = Some(*data);
            }
            Ok(Loaded::Candidates(games)) => self.candidates = games,
            Err(error) => self.error = Some(error),
        }
    }

    fn search_field(&mut self, ui: &mut Ui, rect: Rect, large: bool) -> bool {
        let button_width = if large { 94.0 } else { 76.0 };
        let edit_rect = Rect::from_min_max(
            rect.min,
            pos2(rect.right() - button_width - 8.0, rect.bottom()),
        );
        ui.painter()
            .rect(edit_rect, 7.0, INPUT, Stroke::new(1.0, BORDER));
        let icon_center = pos2(edit_rect.left() + 23.0, edit_rect.center().y - 1.0);
        ui.painter()
            .circle_stroke(icon_center, 6.0, Stroke::new(1.6, MUTED));
        ui.painter().line_segment(
            [icon_center + vec2(4.5, 4.5), icon_center + vec2(9.0, 9.0)],
            Stroke::new(1.6, MUTED),
        );
        let inner = Rect::from_min_max(
            pos2(edit_rect.left() + 43.0, edit_rect.top()),
            pos2(edit_rect.right() - 12.0, edit_rect.bottom()),
        );
        let response = ui.put(
            inner,
            egui::TextEdit::singleline(&mut self.query)
                .id(egui::Id::new("game_search"))
                .hint_text(if large {
                    "Game name or AppID"
                } else {
                    "Search for another game"
                })
                .font(FontId::proportional(if large { 17.0 } else { 14.0 }))
                .margin(Vec2::ZERO)
                .frame(false)
                .min_size(inner.size())
                .vertical_align(Align::Center)
                .desired_width(inner.width()),
        );
        if self.focus_search {
            response.request_focus();
            self.focus_search = false;
        }
        if response.has_focus() {
            ui.painter()
                .rect_stroke(edit_rect, 7.0, Stroke::new(1.0, ACCENT));
        }
        let enter = response.lost_focus() && ui.input(|input| input.key_pressed(Key::Enter));
        let button_rect =
            Rect::from_min_max(pos2(rect.right() - button_width, rect.top()), rect.max);
        let clicked = ui
            .put(
                button_rect,
                egui::Button::new(
                    RichText::new("Search")
                        .size(if large { 16.0 } else { 14.0 })
                        .strong()
                        .color(WHITE),
                )
                .fill(BLUE)
                .stroke(Stroke::NONE)
                .rounding(7.0),
            )
            .clicked();
        (clicked || enter) && !self.query.trim().is_empty()
    }

    fn welcome(&mut self, ui: &mut Ui, rect: Rect, ctx: &Context) {
        brand(ui, rect.left_top(), 18.0);
        self.settings_button(
            ui,
            Rect::from_min_size(pos2(rect.right() - 80.0, rect.top()), vec2(80.0, 32.0)),
        );
        let content_width = 600.0_f32.min(rect.width() - 60.0);
        let center = rect.center() - vec2(0.0, 12.0);
        ui.painter().text(
            pos2(center.x, center.y - 101.0),
            Align2::CENTER_CENTER,
            "Who's playing?",
            FontId::proportional(36.0),
            WHITE,
        );
        ui.painter().text(
            pos2(center.x, center.y - 61.0),
            Align2::CENTER_CENTER,
            "Your games, in numbers.",
            FontId::proportional(16.0),
            MUTED,
        );
        let search = Rect::from_center_size(pos2(center.x, center.y), vec2(content_width, 56.0));
        if self.search_field(ui, search, true) {
            self.search(ctx, None);
        }
        let has_recent = !self.settings.recent_games.is_empty();
        let games: Vec<(String, Option<Game>)> = if has_recent {
            self.settings
                .recent_games
                .iter()
                .take(3)
                .map(|game| (game.name.clone(), Some(game.clone())))
                .collect()
        } else {
            ["Counter-Strike 2", "Dota 2", "ELDEN RING"]
                .into_iter()
                .map(|name| (name.to_owned(), None))
                .collect()
        };
        let row_width = (48.0 + games.len() as f32 * 182.0).min(content_width);
        let button_width = (row_width - 48.0) / games.len() as f32 - 8.0;
        let shortcuts =
            Rect::from_center_size(pos2(center.x, center.y + 91.0), vec2(row_width, 32.0));
        ui.scope_builder(
            UiBuilder::new()
                .max_rect(shortcuts)
                .layout(Layout::left_to_right(Align::Center)),
            |ui| {
                ui.label(
                    RichText::new(if has_recent { "Last" } else { "Try" })
                        .size(12.0)
                        .color(MUTED),
                );
                for (name, game) in games {
                    if ui
                        .add_sized(
                            [button_width, 28.0],
                            egui::Button::new(RichText::new(&name).size(12.0).color(SOFT))
                                .truncate()
                                .fill(PANEL)
                                .stroke(Stroke::new(1.0, BORDER))
                                .rounding(5.0),
                        )
                        .on_hover_text(&name)
                        .clicked()
                    {
                        self.query = name;
                        self.search(ctx, game);
                    }
                }
            },
        );
        ui.painter().text(
            pos2(rect.center().x, rect.bottom() - 6.0),
            Align2::CENTER_BOTTOM,
            "Live counts from Steam. History from SteamCharts.",
            FontId::proportional(12.0),
            DIM,
        );
    }

    fn header(&mut self, ui: &mut Ui, rect: Rect, ctx: &Context) {
        let home = Rect::from_min_size(rect.min, vec2(184.0, 25.0));
        brand(ui, home.min, 12.0);
        if self.data.as_ref().is_some_and(|data| data.demo) {
            ui.painter().text(
                rect.min + vec2(170.0, 12.0),
                Align2::LEFT_CENTER,
                "DEMO",
                FontId::proportional(10.0),
                ACCENT,
            );
        }
        if ui
            .interact(home, ui.id().with("home"), Sense::click())
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .on_hover_text("Back to search")
            .clicked()
        {
            self.home();
            return;
        }
        let search_width = if rect.width() < 940.0 { 300.0 } else { 342.0 };
        self.settings_button(
            ui,
            Rect::from_min_size(
                pos2(rect.right() - 80.0, rect.top() + 16.0),
                vec2(80.0, 34.0),
            ),
        );
        let search = Rect::from_min_size(
            pos2(rect.right() - 94.0 - search_width, rect.top() + 12.0),
            vec2(search_width, 42.0),
        );
        if self.search_field(ui, search, false) {
            self.search(ctx, None);
        }
        let title = self
            .data
            .as_ref()
            .map(|data| data.name.as_str())
            .unwrap_or(if self.busy {
                "Finding your game…"
            } else {
                "Find your game"
            });
        let title_rect = Rect::from_min_max(
            pos2(rect.left(), rect.top() + 32.0),
            pos2(search.left() - 22.0, rect.top() + 71.0),
        );
        ui.scope_builder(UiBuilder::new().max_rect(title_rect), |ui| {
            ui.add(
                egui::Label::new(RichText::new(title).size(28.0).strong().color(WHITE)).truncate(),
            )
            .on_hover_text(title);
        });
    }

    fn dashboard(&mut self, ui: &mut Ui, rect: Rect) {
        let Some(data) = &self.data else {
            return;
        };
        let gap = 12.0;
        let card_width = (rect.width() - gap * 3.0) / 4.0;
        let card_height = 136.0;
        let cards: Vec<_> = (0..4)
            .map(|index| {
                Rect::from_min_size(
                    pos2(rect.left() + index as f32 * (card_width + gap), rect.top()),
                    vec2(card_width, card_height),
                )
            })
            .collect();
        for card in &cards {
            ui.painter()
                .rect(*card, 9.0, PANEL, Stroke::new(1.0, BORDER));
        }
        let metrics = data.metrics(self.selected_month, self.selected_year);
        metric(ui, cards[0], "PLAYERS NOW", &metrics.live, Some(GREEN));
        metric(ui, cards[1], "WEEKLY AVERAGE", &metrics.week, None);
        metric(ui, cards[2], "MONTHLY AVERAGE", &metrics.month, None);
        metric(ui, cards[3], "YEARLY AVERAGE", &metrics.year, None);

        let month_rect = Rect::from_min_size(
            cards[2].min + vec2(16.0, 31.0),
            vec2(card_width - 32.0, 27.0),
        );
        ui.scope_builder(UiBuilder::new().max_rect(month_rect), |ui| {
            egui::ComboBox::from_id_salt("month")
                .selected_text(
                    RichText::new(month_name(self.selected_month))
                        .size(12.0)
                        .color(ACCENT),
                )
                .width(card_width - 38.0)
                .height(230.0)
                .show_ui(ui, |ui| {
                    for month in data.months() {
                        ui.selectable_value(&mut self.selected_month, month, month_name(month));
                    }
                });
        });
        let year_rect = Rect::from_min_size(
            cards[3].min + vec2(16.0, 31.0),
            vec2(card_width - 32.0, 27.0),
        );
        ui.scope_builder(UiBuilder::new().max_rect(year_rect), |ui| {
            egui::ComboBox::from_id_salt("year")
                .selected_text(
                    RichText::new(self.selected_year.to_string())
                        .size(12.0)
                        .color(ACCENT),
                )
                .width(card_width - 38.0)
                .height(230.0)
                .show_ui(ui, |ui| {
                    for year in data.years() {
                        ui.selectable_value(&mut self.selected_year, year, year.to_string());
                    }
                });
        });
        let chart_rect = Rect::from_min_max(
            pos2(rect.left(), rect.top() + card_height + 18.0),
            pos2(rect.right(), rect.bottom() - 32.0),
        );
        chart::draw(
            ui,
            chart_rect,
            &mut self.range,
            self.selected_month,
            self.selected_year,
            data.history.as_ref(),
            data.demo,
        );
        let bottom = pos2(rect.left(), rect.bottom() - 7.0);
        let status = if data.demo {
            "Demo mode · all numbers are examples".to_owned()
        } else {
            let history = data
                .history
                .as_ref()
                .map(|history| {
                    format!(
                        "History {} {} UTC",
                        match history.cache_state {
                            CacheState::Network => "fetched",
                            CacheState::Fresh => "cached",
                            CacheState::Stale => "stale",
                        },
                        history.retrieved_at.format("%d %b %H:%M")
                    )
                })
                .unwrap_or_else(|| "History unavailable".to_owned());
            let live = data
                .current
                .as_ref()
                .map(|current| format!("Steam {} UTC", current.checked_at.format("%H:%M")))
                .unwrap_or_else(|| "Steam unavailable".to_owned());
            format!("AppID {}  ·  {live}  ·  {history}", data.appid)
        };
        ui.painter().circle_filled(
            bottom + vec2(3.0, -5.0),
            3.0,
            if data.demo { ACCENT } else { GREEN },
        );
        ui.painter().text(
            bottom + vec2(14.0, 0.0),
            Align2::LEFT_BOTTOM,
            status,
            FontId::proportional(11.0),
            MUTED,
        );
        let note = if data.warnings.is_empty() {
            "~ estimated values"
        } else {
            "Partial data (i)"
        };
        let note_rect =
            Rect::from_min_max(pos2(rect.right() - 150.0, rect.bottom() - 28.0), rect.max);
        ui.painter().text(
            pos2(rect.right(), bottom.y),
            Align2::RIGHT_BOTTOM,
            note,
            FontId::proportional(11.0),
            if data.warnings.is_empty() { DIM } else { AMBER },
        );
        ui.interact(note_rect, ui.id().with("data_warnings"), Sense::hover())
            .on_hover_text(if data.warnings.is_empty() {
                "Averages marked ~ are estimates. Hover over a card for its period and coverage."
                    .to_owned()
            } else {
                data.warnings.join("\n\n")
            });
    }

    fn intermediate(&mut self, ui: &mut Ui, rect: Rect, ctx: &Context) {
        if self.busy {
            let center = rect.center() - vec2(0.0, 22.0);
            ui.put(
                Rect::from_center_size(center, vec2(28.0, 28.0)),
                egui::Spinner::new().color(ACCENT),
            );
            ui.painter().text(
                center + vec2(0.0, 48.0),
                Align2::CENTER_CENTER,
                "Loading players and averages…",
                FontId::proportional(16.0),
                MUTED,
            );
            ctx.request_repaint_after(Duration::from_millis(40));
        } else if !self.candidates.is_empty() {
            let panel =
                Rect::from_center_size(rect.center(), vec2(620.0, rect.height().min(350.0)));
            let mut choice = None;
            ui.scope_builder(UiBuilder::new().max_rect(panel), |ui| {
                ui.label(
                    RichText::new("Which game did you mean?")
                        .size(23.0)
                        .strong(),
                );
                ui.add_space(8.0);
                // Il pannello principale non scorre; solo una lista ambigua puo scorrere.
                egui::ScrollArea::vertical()
                    .max_height(panel.height() - 45.0)
                    .show(ui, |ui| {
                        for game in &self.candidates {
                            let label = format!("{}   ·   {}", game.name, game.appid);
                            if ui
                                .add_sized(
                                    [panel.width() - 16.0, 42.0],
                                    egui::Button::new(RichText::new(label).size(15.0)).fill(PANEL),
                                )
                                .clicked()
                            {
                                choice = Some(game.clone());
                            }
                        }
                    });
            });
            if let Some(game) = choice {
                self.search(ctx, Some(game));
            }
        } else if let Some(error) = &self.error {
            let panel = Rect::from_center_size(rect.center(), vec2(600.0, 180.0));
            ui.scope_builder(
                UiBuilder::new()
                    .max_rect(panel)
                    .layout(Layout::top_down(Align::Center)),
                |ui| {
                    ui.label(
                        RichText::new("These data are unavailable")
                            .size(24.0)
                            .color(WHITE),
                    );
                    ui.add_space(14.0);
                    ui.label(RichText::new(error).size(14.0).color(MUTED));
                    ui.add_space(14.0);
                    ui.label(
                        RichText::new("Try the full game name or its AppID.")
                            .size(13.0)
                            .color(ACCENT),
                    );
                },
            );
        }
    }

    fn settings_button(&mut self, ui: &mut Ui, rect: Rect) {
        if ui
            .put(
                rect,
                egui::Button::new(RichText::new("Settings").size(12.0).color(SOFT)).fill(PANEL),
            )
            .clicked()
        {
            self.settings_open = !self.settings_open;
            self.cache_size = HistoryCache::new(true)
                .and_then(|cache| cache.size_bytes())
                .unwrap_or(0);
        }
    }

    fn settings_window(&mut self, ctx: &Context) {
        let mut open = self.settings_open;
        egui::Window::new("Settings").open(&mut open).collapsible(false).resizable(false)
            .default_width(410.0).anchor(Align2::CENTER_CENTER, Vec2::ZERO).show(ctx, |ui| {
                ui.label(RichText::new("Local history").size(18.0).strong());
                ui.add_space(8.0);
                let old = self.settings.cache_enabled;
                if ui.add_enabled(self.pending_requests == 0, egui::Checkbox::new(&mut self.settings.cache_enabled, "Save history on this computer")).changed() {
                    match self.settings.save() {
                        Ok(()) => self.settings_message = Some("Saved. Applies to your next search.".to_owned()),
                        Err(error) => { self.settings.cache_enabled = old; self.settings_message = Some(format!("Could not save settings: {error:#}")); }
                    }
                }
                ui.add_space(8.0);
                ui.label(RichText::new("Reuse charts and averages for one hour, including after restarting the app. A failed refresh can use older saved history, clearly marked as stale.").color(MUTED));
                ui.add_space(8.0);
                ui.label(RichText::new("Steam counts are reused for up to 60 seconds in this session. Changing chart periods makes no network requests.").color(MUTED));
                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    ui.label(format!("Cache: {:.1} MiB / 50 MiB", self.cache_size as f64 / 1_048_576.0));
                    if ui.add_enabled(self.pending_requests == 0, egui::Button::new("Clear cache")).clicked() {
                        match HistoryCache::new(true).and_then(|cache| cache.clear()) {
                            Ok(()) => { self.cache_size = 0; self.settings_message = Some("Saved history cleared.".to_owned()); }
                            Err(error) => self.settings_message = Some(format!("Could not clear cache: {error:#}")),
                        }
                    }
                });
                ui.label(RichText::new("Oldest entries are removed at the size limit. Disabling the option stops cache use; Clear cache deletes saved history.").size(12.0).color(MUTED));
                if let Ok(path) = crate::cache::data_dir() {
                    ui.add_space(8.0);
                    ui.label(RichText::new(path.display().to_string()).size(11.0).color(DIM));
                }
                if self.pending_requests > 0 { ui.label(RichText::new("Wait for the current search to finish before changing storage.").color(AMBER)); }
                if let Some(message) = &self.settings_message { ui.add_space(8.0); ui.label(RichText::new(message).size(12.0).color(ACCENT)); }
                ui.add_space(12.0);
                ui.label(RichText::new(format!("SteamCounter {} · Steam + SteamCharts", env!("CARGO_PKG_VERSION"))).size(11.0).color(DIM));
                if ui.button("View licenses").clicked() {
                    self.licenses.open(false);
                }
            });
        self.settings_open = open;
    }
}

impl eframe::App for SteamCounterApp {
    fn update(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
        self.receive();
        egui::CentralPanel::default()
            .frame(
                egui::Frame::none()
                    .fill(BG)
                    .inner_margin(egui::Margin::symmetric(28.0, 26.0)),
            )
            .show(ctx, |ui| {
                let rect = ui.max_rect();
                if !self.has_searched {
                    self.welcome(ui, rect, ctx);
                    return;
                }
                let header = Rect::from_min_size(rect.min, vec2(rect.width(), 76.0));
                self.header(ui, header, ctx);
                if !self.has_searched {
                    ctx.request_repaint();
                    return;
                }
                let body = Rect::from_min_max(pos2(rect.left(), header.bottom() + 17.0), rect.max);
                if self.data.is_some() {
                    self.dashboard(ui, body);
                } else {
                    self.intermediate(ui, body, ctx);
                }
            });
        self.settings_window(ctx);
        self.licenses.show(ctx);
        #[cfg(feature = "gui-preview")]
        if let Some(preview) = &mut self.preview {
            preview.update(ctx, self.busy);
        }
    }
}

fn metric(ui: &Ui, rect: Rect, label: &str, metric: &data::Metric, dot: Option<Color32>) {
    let left = rect.left() + 17.0;
    if let Some(color) = dot {
        ui.painter()
            .circle_filled(pos2(rect.right() - 20.0, rect.top() + 22.0), 3.5, color);
    }
    ui.painter().text(
        pos2(left, rect.top() + 16.0),
        Align2::LEFT_TOP,
        label,
        FontId::proportional(10.0),
        MUTED,
    );
    if !metric.period.is_empty() {
        ui.painter().text(
            pos2(left, rect.top() + 39.0),
            Align2::LEFT_TOP,
            &metric.period,
            FontId::proportional(12.0),
            SOFT,
        );
    }
    ui.painter().text(
        pos2(left, rect.top() + 69.0),
        Align2::LEFT_TOP,
        &metric.value,
        FontId::proportional(if rect.width() < 215.0 { 26.0 } else { 30.0 }),
        WHITE,
    );
    ui.painter().text(
        pos2(left, rect.bottom() - 16.0),
        Align2::LEFT_BOTTOM,
        &metric.note,
        FontId::proportional(11.0),
        MUTED,
    );
    ui.interact(rect, ui.id().with(label), Sense::hover())
        .on_hover_text(&metric.detail);
}
