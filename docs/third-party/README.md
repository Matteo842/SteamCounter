# Third-party licenses and sources

The egui 0.29.1 crate family omits some workspace-level license files from its published crate archives. These original files were retrieved from the source commit recorded in eframe's `.cargo_vcs_info.json`:

- [MIT](https://github.com/emilk/egui/blob/fe368bacc4a15697e347607e73a56c0299b3d42d/LICENSE-MIT)
- [Apache 2.0](https://github.com/emilk/egui/blob/fe368bacc4a15697e347607e73a56c0299b3d42d/LICENSE-APACHE)

`scripts/generate-notices.ps1` combines dependency notices and font license texts from the locked Windows runtime and build dependencies into [THIRD_PARTY_NOTICES.txt](THIRD_PARTY_NOTICES.txt). Identical license texts are shared by reference number. This file is embedded in the executable, accessible through Settings → View licenses, so the app can be distributed as one file.

[SOURCES.md](SOURCES.md) lists exact source archives; [sources/](sources/) also contains unmodified MPL-covered archives. These components are additionally available under GPL-3.0-or-later as part of SteamCounter's combined work under MPL section 3.3, with their original MPL notices preserved.

The root [LICENSE](../../LICENSE) is the unmodified GPLv3 text from the [SPDX license catalog](https://github.com/spdx/license-list-data/blob/main/text/GPL-3.0-or-later.txt). SteamCounter uses GPL-3.0-or-later; third-party libraries and fonts retain the terms specified in their notices. These licenses do not cover Steam/SteamCharts data or trademarks.
