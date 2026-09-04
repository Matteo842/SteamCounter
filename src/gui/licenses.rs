// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Matteo842

use super::style::MUTED;
use eframe::egui::{self, Align2, Context, RichText, TextStyle, Vec2, vec2};
use std::sync::LazyLock;

const GPL: &str = include_str!("../../LICENSE");
const NOTICES: &str = include_str!("../../docs/third-party/THIRD_PARTY_NOTICES.txt");
static GPL_LINES: LazyLock<Vec<String>> = LazyLock::new(|| display_lines(GPL));
static NOTICE_LINES: LazyLock<Vec<String>> = LazyLock::new(|| display_lines(NOTICES));

fn display_lines(text: &str) -> Vec<String> {
    let mut lines = Vec::new();
    for mut rest in text.lines() {
        while let Some((limit, _)) = rest.char_indices().nth(82) {
            let split = rest[..limit]
                .rfind(char::is_whitespace)
                .filter(|&i| i > 0)
                .unwrap_or(limit);
            lines.push(rest[..split].to_owned());
            rest = rest[split..].trim_start();
        }
        lines.push(rest.to_owned());
    }
    lines
}

#[derive(Default)]
pub struct LicenseViewer {
    pub open: bool,
    third_party: bool,
}

impl LicenseViewer {
    pub fn open(&mut self, third_party: bool) {
        self.third_party = third_party;
        self.open = true;
    }

    pub fn show(&mut self, ctx: &Context) {
        egui::Window::new("Licenses")
            .open(&mut self.open)
            .collapsible(false)
            .default_size(vec2(680.0, 440.0))
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .show(ctx, |ui| {
                ui.label(RichText::new("SteamCounter · Copyright © 2026 Matteo842").strong());
                ui.label(
                    RichText::new("Free software under GNU GPL v3 or later. No warranty.")
                        .color(MUTED),
                );
                ui.horizontal_wrapped(|ui| {
                    ui.hyperlink_to(
                        "Source code",
                        format!(
                            "https://github.com/Matteo842/SteamCounter/tree/v{}",
                            crate::DISPLAY_VERSION
                        ),
                    );
                    ui.hyperlink_to(
                        "Dependency sources",
                        format!(
                            "https://github.com/Matteo842/SteamCounter/blob/v{}/docs/third-party/SOURCES.md",
                            crate::DISPLAY_VERSION
                        ),
                    );
                });
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut self.third_party, false, "GPLv3");
                    ui.selectable_value(&mut self.third_party, true, "Third-party notices");
                    if ui.button("Copy text").clicked() {
                        ui.output_mut(|output| {
                            output.copied_text =
                                if self.third_party { NOTICES } else { GPL }.to_owned()
                        });
                    }
                });
                ui.separator();
                let lines = if self.third_party {
                    &*NOTICE_LINES
                } else {
                    &*GPL_LINES
                };
                let row_height = ui.text_style_height(&TextStyle::Monospace);
                // Virtualize the long notices so opening this window stays responsive.
                egui::ScrollArea::both()
                    .id_salt(if self.third_party {
                        "dependency_licenses"
                    } else {
                        "gpl_license"
                    })
                    .auto_shrink([false, false])
                    .show_rows(ui, row_height, lines.len(), |ui, rows| {
                        for line in &lines[rows] {
                            ui.add(
                                egui::Label::new(
                                    RichText::new(if line.is_empty() { " " } else { line })
                                        .monospace()
                                        .color(MUTED),
                                )
                                .extend(),
                            );
                        }
                    });
            });
    }
}
