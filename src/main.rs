//! tiny-dfr: Touch Bar daemon for Apple Silicon Macs

use std::panic::{self, AssertUnwindSafe};
use clap::Parser;
use egui::{Context, RawInput, Rect, Pos2, Vec2};
use nix::sys::signal::{Signal, SigSet};

mod app;
mod core;
mod config;
mod display;
mod input;
mod system;

use core::{DFR_WIDTH, DFR_HEIGHT};
use display::{DrmBackend, TinySkiaPainter, render_error, copy_to_drm};

#[derive(Parser)]
#[command(name = "tiny-dfr", about = "Touch Bar daemon")]
struct Cli {
    /// Path to config file
    #[arg(short, long, default_value = "/etc/tiny-dfr/config.toml")]
    config: String,
}

fn main() {
    let cli = Cli::parse();
    let mut drm = DrmBackend::open_card().unwrap();

    let result = panic::catch_unwind(AssertUnwindSafe(|| {
        app::run(&mut drm, cli.config)
    }));

    if result.is_err() {
        show_crash_screen(&mut drm);

        // Wait for SIGTERM
        let mut sigset = SigSet::empty();
        sigset.add(Signal::SIGTERM);
        sigset.wait().unwrap();
    }
}

fn show_crash_screen(drm: &mut DrmBackend) {
    let ctx = Context::default();
    let mut painter = TinySkiaPainter::new(DFR_WIDTH as u32, DFR_HEIGHT as u32);

    let input = RawInput {
        screen_rect: Some(Rect::from_min_size(
            Pos2::ZERO,
            Vec2::new(DFR_WIDTH as f32, DFR_HEIGHT as f32),
        )),
        ..Default::default()
    };

    let output = ctx.run(input, |ctx| {
        render_error(ctx, concat!(env!("CARGO_PKG_NAME"), " crashed"));
    });

    let primitives = ctx.tessellate(output.shapes, output.pixels_per_point);
    painter.paint(&primitives, &output.textures_delta);

    let _ = copy_to_drm(&painter, drm);
}
