//! tiny-dfr: Touch Bar daemon for Apple Silicon Macs

use std::panic::{self, AssertUnwindSafe};
use clap::Parser;
use drm::control::ClipRect;
use nix::sys::signal::{Signal, SigSet};

mod app;
mod core;
mod config;
mod display;
mod input;
mod system;

use core::{DFR_WIDTH, DFR_HEIGHT};
use display::DrmBackend;

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

    let _ = panic::catch_unwind(AssertUnwindSafe(|| {
        app::run(&mut drm, cli.config)
    }));

    // Show crash screen on panic
    show_crash_screen(&mut drm);

    // Wait for SIGTERM
    let mut sigset = SigSet::empty();
    sigset.add(Signal::SIGTERM);
    sigset.wait().unwrap();
}

fn show_crash_screen(drm: &mut DrmBackend) {
    let crash_bitmap = include_bytes!("crash_bitmap.raw");
    let mut map = drm.map().unwrap();
    let data = map.as_mut();

    let mut wptr = 0;
    for byte in crash_bitmap {
        for i in 0..8 {
            let bit = ((byte >> i) & 0x1) == 0;
            let color = if bit { 0xFF } else { 0x0 };
            data[wptr] = color;
            data[wptr + 1] = color;
            data[wptr + 2] = color;
            data[wptr + 3] = color;
            wptr += 4;
        }
    }

    drop(map);
    drm.dirty(&[ClipRect::new(0, 0, DFR_HEIGHT as u16, DFR_WIDTH as u16)]).unwrap();
}
