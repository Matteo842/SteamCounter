// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Matteo842
// See LICENSE in the project root for the full terms.

#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

fn main() -> eframe::Result {
    steamcounter::gui::run()
}
