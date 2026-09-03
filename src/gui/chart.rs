use super::style::*;
pub use crate::series::ChartRange;
use crate::{
    history::HistorySnapshot,
    series::{Point, Series, SeriesKind},
};
use chrono::{DateTime, NaiveDate, Utc};
use eframe::egui::{self, Align2, Color32, FontId, Rect, Sense, Stroke, Ui, pos2, vec2};

pub fn draw(
    ui: &mut Ui,
    rect: Rect,
    range: &mut ChartRange,
    month: NaiveDate,
    year: i32,
    history: Option<&HistorySnapshot>,
    demo: bool,
) {
    ui.painter()
        .rect(rect, 9.0, PANEL, Stroke::new(1.0, BORDER));
    let left = rect.left() + 21.0;
    ui.painter().text(
        pos2(left, rect.top() + 22.0),
        Align2::LEFT_CENTER,
        "Player history",
        FontId::proportional(16.0),
        WHITE,
    );
    for (i, option) in [
        ChartRange::Hours,
        ChartRange::Week,
        ChartRange::Month,
        ChartRange::Year,
    ]
    .into_iter()
    .enumerate()
    {
        let button = Rect::from_min_size(
            pos2(left + i as f32 * 60.0, rect.top() + 47.0),
            vec2(52.0, 29.0),
        );
        if ui
            .put(
                button,
                egui::Button::new(
                    egui::RichText::new(option.label())
                        .size(13.0)
                        .color(if *range == option { WHITE } else { MUTED }),
                )
                .fill(if *range == option { BLUE } else { INPUT })
                .stroke(Stroke::new(1.0, BORDER))
                .rounding(5.0),
            )
            .clicked()
        {
            *range = option;
        }
    }
    let mut series = Series::build(history, *range, month, year, Utc::now());
    if demo {
        series.kind = SeriesKind::Hourly;
        series.note = "Demo data · search for a game to load real player history".to_owned();
        series.points = (0..180)
            .map(|i| {
                let t = i as f64 / 179.0;
                Point {
                    at: series.start + (series.end - series.start) * i / 179,
                    value: 820_000.0 + (t * 7.0 * std::f64::consts::TAU).sin() * 320_000.0,
                }
            })
            .collect();
    }
    let badge = if demo { "DEMO DATA" } else { series.label() };
    ui.painter().text(
        pos2(rect.right() - 21.0, rect.top() + 22.0),
        Align2::RIGHT_CENTER,
        badge,
        FontId::proportional(11.0),
        ACCENT,
    );
    let title = match range {
        ChartRange::Hours => "Last 48 hours".to_owned(),
        ChartRange::Week => "Last 7 days".to_owned(),
        ChartRange::Month => month_name(month),
        ChartRange::Year => year.to_string(),
    };
    ui.painter().text(
        pos2(rect.right() - 21.0, rect.top() + 61.0),
        Align2::RIGHT_CENTER,
        title,
        FontId::proportional(12.0),
        MUTED,
    );
    let plot = Rect::from_min_max(
        pos2(left, rect.top() + 100.0),
        pos2(rect.right() - 78.0, rect.bottom() - 99.0),
    );
    let max = nice_max(
        series
            .points
            .iter()
            .map(|point| point.value)
            .fold(0.0, f64::max),
    );
    for i in 0..=4 {
        let y = plot.bottom() - plot.height() * i as f32 / 4.0;
        ui.painter().line_segment(
            [pos2(plot.left(), y), pos2(plot.right(), y)],
            Stroke::new(1.0, BORDER),
        );
        ui.painter().text(
            pos2(plot.right() + 10.0, y),
            Align2::LEFT_CENTER,
            short_number(max * i as f64 / 4.0),
            FontId::proportional(10.0),
            DIM,
        );
    }
    let x_at = |at: DateTime<Utc>| {
        let fraction = (at - series.start).num_seconds() as f64
            / (series.end - series.start).num_seconds().max(1) as f64;
        plot.left() + plot.width() * fraction as f32
    };
    let path: Vec<_> = series
        .points
        .iter()
        .map(|point| {
            pos2(
                x_at(point.at),
                plot.bottom() - plot.height() * (point.value / max) as f32,
            )
        })
        .collect();
    if series.kind == SeriesKind::MonthSummary {
        if let Some(point) = path.first() {
            let bar = Rect::from_min_max(
                pos2(point.x - 30.0, point.y),
                pos2(point.x + 30.0, plot.bottom()),
            );
            ui.painter().rect_filled(bar, 4.0, BLUE);
            ui.painter().text(
                *point - vec2(0.0, 12.0),
                Align2::CENTER_BOTTOM,
                number(series.points[0].value),
                FontId::proportional(15.0),
                WHITE,
            );
        }
    } else {
        // Draw each valid interval independently: never bridge missing hours or months.
        let mut mesh = egui::Mesh::default();
        for (i, pair) in series.points.windows(2).enumerate() {
            if demo || series.connects(&pair[0], &pair[1]) {
                let start = mesh.vertices.len() as u32;
                mesh.colored_vertex(path[i], Color32::from_rgba_unmultiplied(44, 159, 230, 38));
                mesh.colored_vertex(pos2(path[i].x, plot.bottom()), Color32::TRANSPARENT);
                mesh.colored_vertex(
                    path[i + 1],
                    Color32::from_rgba_unmultiplied(44, 159, 230, 38),
                );
                mesh.colored_vertex(pos2(path[i + 1].x, plot.bottom()), Color32::TRANSPARENT);
                mesh.add_triangle(start, start + 1, start + 2);
                mesh.add_triangle(start + 1, start + 3, start + 2);
            }
        }
        ui.painter().add(egui::Shape::mesh(mesh));
        for (i, pair) in series.points.windows(2).enumerate() {
            if demo || series.connects(&pair[0], &pair[1]) {
                ui.painter()
                    .line_segment([path[i], path[i + 1]], Stroke::new(2.0, ACCENT));
            }
        }
        for (i, point) in path.iter().enumerate() {
            let isolated = (i == 0 || !series.connects(&series.points[i - 1], &series.points[i]))
                && (i + 1 == path.len()
                    || !series.connects(&series.points[i], &series.points[i + 1]));
            if series.kind == SeriesKind::Monthly || path.len() == 1 || isolated {
                ui.painter().circle_filled(*point, 3.0, ACCENT);
            }
        }
    }
    if series.points.is_empty() {
        ui.painter().text(
            plot.center(),
            Align2::CENTER_CENTER,
            "No data available for this period",
            FontId::proportional(16.0),
            MUTED,
        );
    }
    for i in 0..5 {
        let at = series.start + (series.end - series.start) * i / 4;
        let label = match series.kind {
            SeriesKind::Monthly => at.format("%b %Y").to_string(),
            SeriesKind::MonthSummary => {
                if i == 2 {
                    at.format("%B %Y").to_string()
                } else {
                    String::new()
                }
            }
            SeriesKind::Hourly => {
                if *range == ChartRange::Hours || (series.end - series.start).num_days() < 5 {
                    at.format("%d %b %H:%M").to_string()
                } else {
                    at.format("%d %b").to_string()
                }
            }
        };
        ui.painter().text(
            pos2(
                plot.left() + plot.width() * i as f32 / 4.0,
                plot.bottom() + 18.0,
            ),
            if i == 0 {
                Align2::LEFT_CENTER
            } else if i == 4 {
                Align2::RIGHT_CENTER
            } else {
                Align2::CENTER_CENTER
            },
            label,
            FontId::proportional(11.0),
            MUTED,
        );
    }
    let hover = ui.interact(plot, ui.id().with("player_plot"), Sense::hover());
    if let Some(pointer) = hover.hover_pos()
        && let Some((i, point)) = path
            .iter()
            .enumerate()
            .min_by(|(_, a), (_, b)| (a.x - pointer.x).abs().total_cmp(&(b.x - pointer.x).abs()))
        && (point.x - pointer.x).abs() < 18.0
    {
        ui.painter().line_segment(
            [pos2(point.x, plot.top()), pos2(point.x, plot.bottom())],
            Stroke::new(1.0, DIM),
        );
        ui.painter().circle_filled(*point, 4.0, WHITE);
        let sample = &series.points[i];
        let time = if series.kind == SeriesKind::Hourly {
            sample.at.format("%d %b %Y, %H:%M UTC").to_string()
        } else {
            sample.at.format("%B %Y").to_string()
        };
        egui::show_tooltip_for(
            ui.ctx(),
            ui.layer_id(),
            hover.id,
            &Rect::from_center_size(*point, vec2(24.0, 24.0)),
            |ui| {
                ui.label(format!(
                    "{time}\n{}: {}{}",
                    series.label(),
                    number(sample.value),
                    if demo { " (demo)" } else { "" }
                ));
            },
        );
    }
    draw_overview(
        ui,
        Rect::from_min_max(
            pos2(left, rect.bottom() - 53.0),
            pos2(rect.right() - 21.0, rect.bottom() - 22.0),
        ),
        history,
        &series,
    );
    ui.painter().text(
        pos2(left, rect.bottom() - 9.0),
        Align2::LEFT_BOTTOM,
        &series.note,
        FontId::proportional(9.0),
        DIM,
    );
}

fn draw_overview(ui: &Ui, rect: Rect, history: Option<&HistorySnapshot>, series: &Series) {
    ui.painter().rect_filled(rect, 4.0, INPUT);
    let Some(history) = history else {
        return;
    };
    let mut months: Vec<_> = history.months.iter().collect();
    months.sort_by_key(|row| row.month);
    let Some(first) = months.first() else {
        return;
    };
    let start = first.month.and_hms_opt(0, 0, 0).unwrap().and_utc();
    let span = (Utc::now() - start).num_seconds().max(1) as f64;
    let x_at = |at: DateTime<Utc>| {
        rect.left()
            + ((at - start).num_seconds() as f64 / span).clamp(0.0, 1.0) as f32 * rect.width()
    };
    let max = months
        .iter()
        .map(|row| row.players.average_players)
        .fold(1.0, f64::max);
    for pair in months.windows(2) {
        if pair[0].month.checked_add_months(chrono::Months::new(1)) != Some(pair[1].month) {
            continue;
        }
        let to_pos = |row: &crate::history::MonthlyAverage| {
            pos2(
                x_at(row.month.and_hms_opt(0, 0, 0).unwrap().and_utc()),
                rect.bottom()
                    - 3.0
                    - (row.players.average_players / max) as f32 * (rect.height() - 6.0),
            )
        };
        ui.painter()
            .line_segment([to_pos(pair[0]), to_pos(pair[1])], Stroke::new(1.0, DIM));
    }
    let right = x_at(series.end);
    let selected = Rect::from_min_max(
        pos2(
            x_at(series.start).min(right - 3.0).max(rect.left()),
            rect.top(),
        ),
        pos2(right, rect.bottom()),
    );
    ui.painter().rect(
        selected,
        2.0,
        Color32::from_rgba_unmultiplied(40, 135, 203, 18),
        Stroke::new(1.0, BLUE),
    );
    ui.interact(rect, ui.id().with("overview"), Sense::hover()).on_hover_text("Overview of published monthly averages. Use the period buttons and month/year menus to change the main chart.");
}

fn nice_max(value: f64) -> f64 {
    if value < 4.0 {
        return 4.0;
    }
    let step = 10.0_f64.powf((value / 4.0).log10().floor());
    (value * 1.12 / (step * 4.0)).ceil() * step * 4.0
}
fn short_number(value: f64) -> String {
    if value >= 1_000_000.0 {
        format!("{:.1}M", value / 1_000_000.0)
    } else if value >= 1000.0 {
        format!("{:.0}k", value / 1000.0)
    } else {
        format!("{value:.0}")
    }
}
