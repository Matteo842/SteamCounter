mod chart;
mod data;
mod style;

use std::{sync::mpsc, time::Duration};

use chrono::{Datelike, NaiveDate, Utc};
use clap::Parser;
use eframe::egui::{
    self, Align, Align2, Color32, Context, FontId, Key, Layout, Rect, RichText, Sense, Stroke, Ui,
    UiBuilder, Vec2, pos2, vec2,
};

use crate::Game;
use chart::ChartRange;
use data::{DashboardData, Loaded, WorkerMessage};
use style::*;

#[derive(Parser)]
#[command(
    name = "steamcounter-gui",
    version,
    about = "SteamCounter: interfaccia desktop"
)]
struct Options {
    /// Apre un'anteprima con dati dimostrativi, senza rete
    #[arg(long)]
    demo: bool,
    /// Cerca subito un gioco all'apertura
    #[arg(long)]
    game: Option<String>,
    #[arg(long, hide = true)]
    compact: bool,
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
}

impl SteamCounterApp {
    fn new(ctx: &Context, options: Options) -> Self {
        configure(ctx);
        let now = Utc::now();
        let (sender, receiver) = mpsc::channel();
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
        };
        if options.demo {
            app.open_demo();
        } else if let Some(game) = options.game {
            app.query = game;
            app.search(ctx, None);
            // Solo per acquisizioni di sviluppo: il secondo frame deve contenere
            // il risultato, non la schermata di caricamento. Nessuna attesa nella UI normale.
            #[cfg(feature = "gui-preview")]
            if std::env::var_os("EFRAME_SCREENSHOT_TO").is_some()
                && let Ok(message) = app.receiver.recv_timeout(Duration::from_secs(70))
            {
                app.apply_message(message);
            }
        }
        app
    }

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
        if self.query.trim().is_empty() && selected.is_none() {
            return;
        }
        self.request_id += 1;
        self.has_searched = true;
        self.focus_search = false;
        self.busy = true;
        self.data = None;
        self.candidates.clear();
        self.error = None;
        let now = Utc::now();
        self.selected_month = NaiveDate::from_ymd_opt(now.year(), now.month(), 1).unwrap();
        self.selected_year = now.year();
        data::spawn(
            self.request_id,
            self.query.trim().to_owned(),
            selected,
            self.sender.clone(),
            ctx.clone(),
        );
    }

    fn receive(&mut self) {
        while let Ok(message) = self.receiver.try_recv() {
            self.apply_message(message);
        }
    }

    fn apply_message(&mut self, message: WorkerMessage) {
        if message.id != self.request_id {
            return;
        }
        self.busy = false;
        match message.result {
            Ok(Loaded::Dashboard(data)) => self.data = Some(*data),
            Ok(Loaded::Candidates(games)) => self.candidates = games,
            Err(error) => self.error = Some(error),
        }
    }

    fn search_field(&mut self, ui: &mut Ui, rect: Rect, large: bool) -> bool {
        let button_width = if large { 94.0 } else { 66.0 };
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
            pos2(edit_rect.left() + 43.0, edit_rect.center().y - 12.0),
            pos2(edit_rect.right() - 12.0, edit_rect.center().y + 13.0),
        );
        let response = ui.put(
            inner,
            egui::TextEdit::singleline(&mut self.query)
                .id(egui::Id::new("game_search"))
                .hint_text(if large {
                    "Nome del gioco o AppID"
                } else {
                    "Cerca un altro gioco"
                })
                .font(FontId::proportional(if large { 17.0 } else { 14.0 }))
                .margin(Vec2::ZERO)
                .frame(false)
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
                    RichText::new("Cerca")
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
        let content_width = 600.0_f32.min(rect.width() - 60.0);
        let center = rect.center() - vec2(0.0, 12.0);
        ui.painter().text(
            pos2(center.x, center.y - 101.0),
            Align2::CENTER_CENTER,
            "Chi sta giocando?",
            FontId::proportional(36.0),
            WHITE,
        );
        ui.painter().text(
            pos2(center.x, center.y - 61.0),
            Align2::CENTER_CENTER,
            "I tuoi giochi, in numeri.",
            FontId::proportional(16.0),
            MUTED,
        );
        let search = Rect::from_center_size(pos2(center.x, center.y), vec2(content_width, 56.0));
        if self.search_field(ui, search, true) {
            self.search(ctx, None);
        }
        let games = ["Counter-Strike 2", "Dota 2", "ELDEN RING"];
        let examples_width = ui.fonts(|fonts| {
            std::iter::once("Prova")
                .chain(games)
                .map(|label| {
                    fonts
                        .layout_no_wrap(label.to_owned(), FontId::proportional(12.0), SOFT)
                        .size()
                        .x
                })
                .sum::<f32>()
        }) + 6.0 * ui.spacing().button_padding.x
            + 3.0 * ui.spacing().item_spacing.x;
        let examples =
            Rect::from_center_size(pos2(center.x, center.y + 91.0), vec2(examples_width, 32.0));
        ui.scope_builder(
            UiBuilder::new()
                .max_rect(examples)
                .layout(Layout::left_to_right(Align::Center)),
            |ui| {
                ui.label(RichText::new("Prova").size(12.0).color(MUTED));
                for game in games {
                    if ui
                        .add(
                            egui::Button::new(RichText::new(game).size(12.0).color(SOFT))
                                .fill(PANEL)
                                .stroke(Stroke::new(1.0, BORDER))
                                .rounding(5.0),
                        )
                        .clicked()
                    {
                        self.query = game.to_owned();
                        self.search(ctx, None);
                    }
                }
            },
        );
        let demo = Rect::from_center_size(pos2(center.x, center.y + 144.0), vec2(220.0, 28.0));
        if ui
            .put(
                demo,
                egui::Button::new(
                    RichText::new("Esplora l'anteprima")
                        .size(13.0)
                        .color(ACCENT),
                )
                .frame(false),
            )
            .clicked()
        {
            self.open_demo();
        }
        ui.painter().text(
            pos2(rect.center().x, rect.bottom() - 6.0),
            Align2::CENTER_BOTTOM,
            "Steam per il live. SteamCharts per lo storico.",
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
            .on_hover_text("Torna alla ricerca")
            .clicked()
        {
            self.home();
            return;
        }
        let search_width = if rect.width() < 940.0 { 300.0 } else { 342.0 };
        let search = Rect::from_min_size(
            pos2(rect.right() - search_width, rect.top() + 12.0),
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
                "Cerco il tuo gioco…"
            } else {
                "Trova il tuo gioco"
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
        metric(ui, cards[0], "GIOCATORI ADESSO", &metrics.live, Some(GREEN));
        metric(ui, cards[1], "MEDIA SETTIMANALE", &metrics.week, None);
        metric(ui, cards[2], "MEDIA MENSILE", &metrics.month, None);
        metric(ui, cards[3], "MEDIA ANNUALE", &metrics.year, None);

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
        );
        let bottom = pos2(rect.left(), rect.bottom() - 7.0);
        let status = if data.demo {
            "Anteprima interfaccia · tutti i numeri sono dimostrativi".to_owned()
        } else {
            format!(
                "Steam + SteamCharts   ·   AppID {}   ·   {} UTC",
                data.appid,
                data.updated_at.format("%H:%M")
            )
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
            "~ valori stimati"
        } else {
            "Dati parziali (i)"
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
        ui.interact(note_rect, ui.id().with("data_warnings"), Sense::hover()).on_hover_text(if data.warnings.is_empty() { "Le medie contrassegnate da ~ sono stime. Passa sui riquadri per vedere copertura e periodo.".to_owned() } else { data.warnings.join("\n\n") });
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
                "Recupero giocatori e medie…",
                FontId::proportional(16.0),
                MUTED,
            );
            ctx.request_repaint_after(Duration::from_millis(40));
        } else if !self.candidates.is_empty() {
            let panel =
                Rect::from_center_size(rect.center(), vec2(620.0, rect.height().min(350.0)));
            let mut choice = None;
            ui.scope_builder(UiBuilder::new().max_rect(panel), |ui| {
                ui.label(RichText::new("Quale gioco cercavi?").size(23.0).strong());
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
                        RichText::new("Non riesco a trovare questi dati")
                            .size(24.0)
                            .color(WHITE),
                    );
                    ui.add_space(14.0);
                    ui.label(RichText::new(error).size(14.0).color(MUTED));
                    ui.add_space(14.0);
                    ui.label(
                        RichText::new("Prova il nome completo oppure l'AppID del gioco.")
                            .size(13.0)
                            .color(ACCENT),
                    );
                },
            );
        }
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
