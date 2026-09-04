// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Matteo842
// See LICENSE in the project root for the full terms.

use chrono::{Datelike, NaiveDate};
use eframe::egui::{
    self, Align2, Color32, Context, FontId, Pos2, Rect, Stroke, TextStyle, Ui, vec2,
};

pub const BG: Color32 = Color32::from_rgb(13, 19, 27);
pub const PANEL: Color32 = Color32::from_rgb(24, 34, 47);
pub const INPUT: Color32 = Color32::from_rgb(18, 27, 39);
pub const BORDER: Color32 = Color32::from_rgb(39, 55, 72);
pub const WHITE: Color32 = Color32::from_rgb(238, 244, 250);
pub const SOFT: Color32 = Color32::from_rgb(184, 202, 218);
pub const MUTED: Color32 = Color32::from_rgb(132, 155, 175);
pub const DIM: Color32 = Color32::from_rgb(99, 123, 145);
pub const ACCENT: Color32 = Color32::from_rgb(77, 188, 248);
pub const BLUE: Color32 = Color32::from_rgb(27, 137, 216);
pub const GREEN: Color32 = Color32::from_rgb(108, 219, 168);
pub const AMBER: Color32 = Color32::from_rgb(228, 189, 112);

pub fn configure(ctx: &Context) {
    ctx.set_theme(egui::Theme::Dark);
    ctx.send_viewport_cmd(egui::ViewportCommand::SetTheme(egui::SystemTheme::Dark));
    let mut style = (*ctx.style()).clone();
    #[cfg(feature = "gui-preview")]
    if std::env::var_os("STEAMCOUNTER_SCREENSHOT_TO").is_some() {
        style.animation_time = 0.0;
    }
    style.visuals = egui::Visuals::dark();
    style.visuals.panel_fill = BG;
    style.visuals.window_fill = PANEL;
    style.visuals.extreme_bg_color = INPUT;
    style.visuals.faint_bg_color = PANEL;
    style.visuals.override_text_color = Some(WHITE);
    style.visuals.selection.bg_fill = BLUE;
    style.visuals.selection.stroke = Stroke::new(1.0, WHITE);
    for widget in [
        &mut style.visuals.widgets.noninteractive,
        &mut style.visuals.widgets.inactive,
        &mut style.visuals.widgets.open,
    ] {
        widget.bg_fill = PANEL;
        widget.weak_bg_fill = PANEL;
        widget.bg_stroke = Stroke::new(1.0, BORDER);
        widget.fg_stroke = Stroke::new(1.0, SOFT);
        widget.rounding = egui::Rounding::same(5.0);
    }
    style.visuals.widgets.hovered.bg_fill = Color32::from_rgb(32, 51, 70);
    style.visuals.widgets.hovered.weak_bg_fill = Color32::from_rgb(32, 51, 70);
    style.visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, ACCENT);
    style.visuals.widgets.hovered.fg_stroke = Stroke::new(1.0, WHITE);
    style.visuals.widgets.active.bg_fill = BLUE;
    style.visuals.widgets.active.weak_bg_fill = BLUE;
    style.visuals.widgets.active.fg_stroke = Stroke::new(1.0, WHITE);
    style.visuals.window_stroke = Stroke::new(1.0, BORDER);
    style.visuals.window_rounding = egui::Rounding::same(8.0);
    style.spacing.item_spacing = vec2(9.0, 8.0);
    style.spacing.button_padding = vec2(12.0, 6.0);
    style.spacing.interact_size.y = 26.0;
    style
        .text_styles
        .insert(TextStyle::Body, FontId::proportional(14.0));
    style
        .text_styles
        .insert(TextStyle::Button, FontId::proportional(14.0));
    style
        .text_styles
        .insert(TextStyle::Small, FontId::proportional(11.0));
    ctx.set_style(style);
}

pub fn brand(ui: &Ui, top_left: Pos2, size: f32) {
    let mark = Rect::from_min_size(top_left, vec2(24.0, 24.0));
    ui.painter().rect_filled(mark, 5.0, BLUE);
    let points = [
        mark.min + vec2(5.0, 16.0),
        mark.min + vec2(10.0, 11.0),
        mark.min + vec2(14.0, 14.0),
        mark.min + vec2(19.0, 7.0),
    ];
    ui.painter()
        .add(egui::Shape::line(points.to_vec(), Stroke::new(1.7, WHITE)));
    ui.painter().text(
        top_left + vec2(34.0, 12.0),
        Align2::LEFT_CENTER,
        "STEAMCOUNTER",
        FontId::proportional(size),
        SOFT,
    );
}

pub fn centered_brand(ui: &Ui, center: Pos2, size: f32) -> Rect {
    let text =
        ui.painter()
            .layout_no_wrap("STEAMCOUNTER".to_owned(), FontId::proportional(size), SOFT);
    let width = 34.0 + text.size().x;
    let rect = Rect::from_center_size(center, vec2(width, 24.0));
    brand(ui, rect.min, size);
    rect
}

pub fn month_name(date: NaiveDate) -> String {
    const MONTHS: [&str; 12] = [
        "January",
        "February",
        "March",
        "April",
        "May",
        "June",
        "July",
        "August",
        "September",
        "October",
        "November",
        "December",
    ];
    format!("{} {}", MONTHS[date.month0() as usize], date.year())
}

pub fn number(value: f64) -> String {
    let raw = format!("{value:.0}");
    let mut result = String::new();
    for (i, ch) in raw.chars().enumerate() {
        if i > 0 && (raw.len() - i).is_multiple_of(3) {
            result.push(',');
        }
        result.push(ch);
    }
    result
}

pub fn app_icon() -> egui::IconData {
    let mut rgba = vec![0; 32 * 32 * 4];
    for y in 0..32_usize {
        for x in 0..32_usize {
            let on_line = (5..=12).contains(&x) && (y as i32 - (26 - x as i32)).abs() <= 1
                || (12..=18).contains(&x) && (y as i32 - (x as i32 + 2)).abs() <= 1
                || (18..=27).contains(&x) && (y as i32 - (56 - x as i32 * 2)).abs() <= 1;
            let color = if on_line { WHITE } else { BLUE };
            rgba[(y * 32 + x) * 4..(y * 32 + x + 1) * 4].copy_from_slice(&color.to_array());
        }
    }
    egui::IconData {
        rgba,
        width: 32,
        height: 32,
    }
}
