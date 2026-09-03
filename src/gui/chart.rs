use chrono::{Datelike, NaiveDate, Utc};
use eframe::egui::{self, Align2, Color32, FontId, Rect, Sense, Stroke, Ui, pos2, vec2};

use super::style::*;

#[derive(Clone, Copy, PartialEq)]
pub enum ChartRange {
    Hours,
    Week,
    Month,
    Year,
}

impl ChartRange {
    fn label(self) -> &'static str {
        match self {
            Self::Hours => "48h",
            Self::Week => "1w",
            Self::Month => "1m",
            Self::Year => "1y",
        }
    }
    fn cycles(self) -> f32 {
        match self {
            Self::Hours => 2.0,
            Self::Week => 7.0,
            Self::Month => 5.0,
            Self::Year => 3.0,
        }
    }
}

pub fn draw(ui: &mut Ui, rect: Rect, range: &mut ChartRange, month: NaiveDate, year: i32) {
    ui.painter()
        .rect(rect, 9.0, PANEL, Stroke::new(1.0, BORDER));
    let left = rect.left() + 21.0;
    ui.painter().text(
        pos2(left, rect.top() + 22.0),
        Align2::LEFT_CENTER,
        "Andamento giocatori",
        FontId::proportional(16.0),
        WHITE,
    );
    let badge = Rect::from_min_size(
        pos2(rect.right() - 179.0, rect.top() + 12.0),
        vec2(158.0, 23.0),
    );
    ui.painter()
        .rect_filled(badge, 5.0, Color32::from_rgb(31, 55, 74));
    ui.painter().text(
        badge.center(),
        Align2::CENTER_CENTER,
        "ANTEPRIMA GRAFICO",
        FontId::proportional(10.0),
        ACCENT,
    );
    ui.interact(badge, ui.id().with("chart_demo"), Sense::hover()).on_hover_text("Il grafico contiene dati dimostrativi. I numeri nei riquadri sopra sono reali dopo una ricerca. Collegheremo la curva allo storico nel prossimo passo.");
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
                .stroke(Stroke::new(
                    1.0,
                    if *range == option { BLUE } else { BORDER },
                ))
                .rounding(5.0),
            )
            .clicked()
        {
            *range = option;
        }
    }
    let range_title = match range {
        ChartRange::Hours => "Ultime 48 ore".to_owned(),
        ChartRange::Week => "Ultimi 7 giorni".to_owned(),
        ChartRange::Month => month_name(month),
        ChartRange::Year => year.to_string(),
    };
    ui.painter().text(
        pos2(rect.right() - 21.0, rect.top() + 61.0),
        Align2::RIGHT_CENTER,
        range_title,
        FontId::proportional(12.0),
        MUTED,
    );

    let plot = Rect::from_min_max(
        pos2(left, rect.top() + 100.0),
        pos2(rect.right() - 69.0, rect.bottom() - 99.0),
    );
    let max_value = 1_400_000.0;
    for i in 0..=3 {
        let fraction = i as f32 / 3.0;
        let y = plot.bottom() - plot.height() * fraction;
        ui.painter().line_segment(
            [pos2(plot.left(), y), pos2(plot.right(), y)],
            Stroke::new(1.0, BORDER),
        );
        let label = match i {
            0 => "0",
            1 => "467k",
            2 => "933k",
            _ => "1,4M",
        };
        ui.painter().text(
            pos2(plot.right() + 10.0, y),
            Align2::LEFT_CENTER,
            label,
            FontId::proportional(10.0),
            DIM,
        );
    }
    let points = demo_points(*range);
    let path: Vec<_> = points
        .iter()
        .enumerate()
        .map(|(i, value)| {
            pos2(
                plot.left() + plot.width() * i as f32 / (points.len() - 1) as f32,
                plot.bottom() - plot.height() * value / max_value,
            )
        })
        .collect();
    let mut mesh = egui::Mesh::default();
    for point in &path {
        mesh.colored_vertex(*point, Color32::from_rgba_unmultiplied(44, 159, 230, 38));
        mesh.colored_vertex(
            pos2(point.x, plot.bottom()),
            Color32::from_rgba_unmultiplied(44, 159, 230, 2),
        );
    }
    for i in 0..path.len() as u32 - 1 {
        let top = i * 2;
        mesh.add_triangle(top, top + 1, top + 2);
        mesh.add_triangle(top + 1, top + 3, top + 2);
    }
    ui.painter().add(egui::Shape::mesh(mesh));
    ui.painter()
        .add(egui::Shape::line(path.clone(), Stroke::new(2.1, ACCENT)));
    if let Some(last) = path.last() {
        ui.painter().circle_filled(*last, 3.0, ACCENT);
    }
    let axis = axis_labels(*range, month, year);
    for (i, label) in axis.iter().enumerate() {
        let fraction = i as f32 / (axis.len() - 1) as f32;
        let x = plot.left() + plot.width() * fraction;
        ui.painter().text(
            pos2(x, plot.bottom() + 18.0),
            if i == 0 {
                Align2::LEFT_CENTER
            } else if i == axis.len() - 1 {
                Align2::RIGHT_CENTER
            } else {
                Align2::CENTER_CENTER
            },
            label,
            FontId::proportional(11.0),
            MUTED,
        );
    }
    let hover = ui.interact(plot, ui.id().with("demo_plot"), Sense::hover());
    if let Some(pointer) = hover.hover_pos() {
        let fraction = ((pointer.x - plot.left()) / plot.width()).clamp(0.0, 1.0);
        let index = (fraction * (path.len() - 1) as f32).round() as usize;
        let point = path[index];
        ui.painter().line_segment(
            [pos2(point.x, plot.top()), pos2(point.x, plot.bottom())],
            Stroke::new(1.0, DIM),
        );
        ui.painter().circle_filled(point, 4.5, WHITE);
        hover.on_hover_text(format!(
            "Esempio · {} giocatori\nGrafico dimostrativo",
            number(points[index] as f64)
        ));
    }
    let navigator = Rect::from_min_max(
        pos2(left, rect.bottom() - 53.0),
        pos2(rect.right() - 21.0, rect.bottom() - 22.0),
    );
    ui.painter().rect_filled(navigator, 4.0, INPUT);
    let mini: Vec<_> = (0..130)
        .map(|i| {
            let t = i as f32 / 129.0;
            let value =
                0.13 + t * 0.42 + (t * 27.0).sin().abs() * (t * 0.24) + (t * 93.0).sin() * 0.05;
            pos2(
                navigator.left() + t * navigator.width(),
                navigator.bottom() - 3.0 - value * (navigator.height() - 4.0),
            )
        })
        .collect();
    ui.painter()
        .add(egui::Shape::line(mini, Stroke::new(1.0, DIM)));
    let selection = Rect::from_min_max(
        pos2(navigator.left() + navigator.width() * 0.73, navigator.top()),
        navigator.max,
    );
    ui.painter().rect(
        selection,
        3.0,
        Color32::from_rgba_unmultiplied(40, 135, 203, 18),
        Stroke::new(1.0, BLUE),
    );
    ui.painter().text(
        pos2(left, rect.bottom() - 9.0),
        Align2::LEFT_BOTTOM,
        "Curva dimostrativa · non rappresenta lo storico del gioco",
        FontId::proportional(9.0),
        DIM,
    );
}

fn demo_points(range: ChartRange) -> Vec<f32> {
    (0..180)
        .map(|i| {
            let t = i as f32 / 179.0;
            let wave = (t * range.cycles() * std::f32::consts::TAU + 1.1).sin();
            let detail = (t * 93.0).sin() * 36_000.0 + (t * 157.0).cos() * 19_000.0;
            820_000.0 + wave * 320_000.0 + detail + (t * 15.0).sin() * 70_000.0
        })
        .collect()
}

fn axis_labels(range: ChartRange, month: NaiveDate, year: i32) -> Vec<String> {
    match range {
        ChartRange::Hours => ["−48h", "−36h", "−24h", "−12h", "Adesso"]
            .into_iter()
            .map(str::to_owned)
            .collect(),
        ChartRange::Week => (0..7)
            .map(|i| {
                (Utc::now() - chrono::Duration::days(6 - i))
                    .format("%d/%m")
                    .to_string()
            })
            .collect(),
        ChartRange::Month => {
            let now = Utc::now();
            let end = if month.year() == now.year() && month.month() == now.month() {
                now.day()
            } else {
                (month.checked_add_months(chrono::Months::new(1)).unwrap()
                    - chrono::Duration::days(1))
                .day()
            };
            let count = end.clamp(2, 5);
            (0..count)
                .map(|i| {
                    format!(
                        "{:02}/{:02}",
                        1 + (end - 1) * i / (count - 1),
                        month.month()
                    )
                })
                .collect()
        }
        ChartRange::Year => vec![
            format!("Gen {year}"),
            "Mar".to_owned(),
            "Giu".to_owned(),
            "Set".to_owned(),
            "Dic".to_owned(),
        ],
    }
}
