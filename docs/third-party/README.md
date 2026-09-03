# Shared egui license texts

The egui 0.29.1 crate family omits some workspace-level license files from its published crate archives. These original files were retrieved from the source commit recorded in eframe's `.cargo_vcs_info.json`:

- [MIT](https://github.com/emilk/egui/blob/fe368bacc4a15697e347607e73a56c0299b3d42d/LICENSE-MIT)
- [Apache 2.0](https://github.com/emilk/egui/blob/fe368bacc4a15697e347607e73a56c0299b3d42d/LICENSE-APACHE)

The Windows packaging script combines dependency notices and font license texts from the locked runtime dependencies. It also includes unmodified source archives for MPL-covered crates. These are dependency licenses, not licenses for Steam/SteamCharts data or trademarks.
