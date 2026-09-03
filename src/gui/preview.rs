//! Development-only screenshots of the actual native renderer, after layout settles.
use eframe::egui::{Context, Event, ViewportCommand};
use std::path::PathBuf;

pub struct Capture {
    path: PathBuf,
    frames: usize,
    requested: bool,
}
impl Capture {
    pub fn from_env() -> Option<Self> {
        std::env::var_os("STEAMCOUNTER_SCREENSHOT_TO").map(|path| Self {
            path: path.into(),
            frames: 0,
            requested: false,
        })
    }
    pub fn update(&mut self, ctx: &Context, busy: bool) {
        let screenshot = ctx.input(|input| {
            input.events.iter().find_map(|event| match event {
                Event::Screenshot { image, .. } => Some(image.clone()),
                _ => None,
            })
        });
        if let Some(screenshot) = screenshot {
            let bytes: Vec<_> = screenshot
                .pixels
                .iter()
                .flat_map(|color| color.to_array())
                .collect();
            image::save_buffer(
                &self.path,
                &bytes,
                screenshot.width() as u32,
                screenshot.height() as u32,
                image::ColorType::Rgba8,
            )
            .expect("Could not save the development screenshot");
            ctx.send_viewport_cmd(ViewportCommand::Close);
            return;
        }
        self.frames += 1;
        if !busy && self.frames >= 5 && !self.requested {
            self.requested = true;
            ctx.send_viewport_cmd(ViewportCommand::Screenshot);
        }
        ctx.request_repaint_after(std::time::Duration::from_millis(30));
    }
}
