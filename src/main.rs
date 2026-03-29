use std::{
    fs::{File, OpenOptions},
    os::{
        fd::AsFd,
        unix::{io::OwnedFd, fs::OpenOptionsExt}
    },
    path::Path,
    collections::HashMap,
    cmp::min,
    panic::{self, AssertUnwindSafe},
};
use egui::{Context, RawInput, Rect, Pos2, Vec2, CentralPanel, Frame, Color32, FontId, Align2, ViewportId, FontFamily, FontDefinitions, FontData};
use drm::control::ClipRect;
use anyhow::Result;
use clap::Parser;
use input::{
    Libinput, LibinputInterface, Device as InputDevice,
    event::{
        Event, device::DeviceEvent, EventTrait,
        touch::{TouchEvent, TouchEventPosition, TouchEventSlot},
        keyboard::{KeyboardEvent, KeyboardEventTrait, KeyState}
    }
};
use libc::{O_ACCMODE, O_RDONLY, O_RDWR, O_WRONLY};
use input_linux::Key;
use nix::{
    sys::{
        signal::{Signal, SigSet},
        epoll::{Epoll, EpollCreateFlags, EpollEvent, EpollFlags}
    },
    errno::Errno
};
use privdrop::PrivDrop;

#[derive(Parser)]
#[command(name = "tiny-dfr", about = "Touch Bar daemon")]
struct Cli {
    /// Path to config file
    #[arg(short, long, default_value = "/etc/tiny-dfr/config.toml")]
    config: String,
}

mod backlight;
mod display;
mod pixel_shift;
mod config;
mod painter;
mod input_bridge;
mod ui;
mod key_emitter;

use backlight::BacklightManager;
use display::DrmBackend;
use pixel_shift::{PixelShiftManager, PIXEL_SHIFT_WIDTH_PX};
use config::{ButtonConfig, Config, ConfigManager};
use painter::TinySkiaPainter;
use input_bridge::InputBridge;
use key_emitter::KeyEmitter;

const DFR_WIDTH: i32 = 2008;
const DFR_HEIGHT: i32 = 60;
const DFR_STRIDE: i32 = 64;
const BUTTON_SPACING_PX: i32 = 16;
pub const TIMEOUT_MS: i32 = 10 * 1000;

/// Represents the current state of the Touch Bar
enum TouchBarState {
    /// Normal operation with function layers
    Normal,
    /// Error state displaying a message
    Error(String),
}

/// A button in a function layer
pub struct Button {
    /// Display text
    pub text: String,
    /// Key to emit when pressed
    pub action: Key,
    /// Whether button is currently pressed
    active: bool,
    // Style
    pub font_size: f32,
    pub font_family: String,  // Font family name or empty for default
    pub bold: bool,
    pub italic: bool,
    pub foreground: Color32,
    pub background: Color32,
    pub background_active: Color32,
    pub corner_radius: f32,
}

fn parse_hex_color(hex: &str) -> Option<Color32> {
    let hex = hex.trim_start_matches('#');
    if hex.len() == 6 {
        let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
        let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
        let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
        Some(Color32::from_rgb(r, g, b))
    } else {
        None
    }
}

fn load_custom_fonts(ctx: &Context, layers: &[FunctionLayer; 2]) {
    // Collect unique font names from all buttons
    let mut font_names: std::collections::HashSet<String> = std::collections::HashSet::new();
    for layer in layers {
        for button in &layer.buttons {
            if !button.font_family.is_empty() {
                font_names.insert(button.font_family.clone());
            }
        }
    }

    // If no custom fonts needed, skip
    if font_names.is_empty() {
        return;
    }

    let mut fonts = FontDefinitions::default();
    let mut loaded_fonts: Vec<String> = Vec::new();

    // Try to load each font from system
    for font_name in &font_names {
        // If font_name looks like a path, load directly; otherwise use fc-match
        let font_data = if font_name.contains('/') {
            std::fs::read(font_name).ok()
        } else {
            load_system_font(font_name)
        };

        if let Some(data) = font_data {
            let mut fd = FontData::from_owned(data);
            fd.index = 0;  // Use first font in collection
            fonts.font_data.insert(font_name.clone(), fd);
            loaded_fonts.push(font_name.clone());
        }
    }

    // Replace Proportional family: custom fonts first, then defaults
    if !loaded_fonts.is_empty() {
        let mut new_proportional = loaded_fonts.clone();
        // Keep Ubuntu-Light as fallback for ASCII (it's egui's default)
        new_proportional.push("Ubuntu-Light".to_owned());
        eprintln!("[font] Setting Proportional family: {:?}", new_proportional);
        fonts.families.insert(FontFamily::Proportional, new_proportional);
    }

    ctx.set_fonts(fonts);
}

fn debug_glyph_coverage(ctx: &Context, layers: &[FunctionLayer; 2]) {
    eprintln!("[font] Checking glyph coverage for text samples...");
    ctx.fonts(|fonts_reader| {
        let font_id = FontId::proportional(24.0);
        for layer in layers {
            for button in &layer.buttons {
                for ch in button.text.chars() {
                    let width = fonts_reader.glyph_width(&font_id, ch);
                    let codepoint = ch as u32;
                    if width == 0.0 || codepoint > 0xFFFF {
                        eprintln!("[font] char '{}' U+{:05X}: width={}", ch, codepoint, width);
                    }
                }
            }
        }
    });
}

fn load_system_font(name: &str) -> Option<Vec<u8>> {
    // Try with fontconfig via fc-match
    if let Ok(output) = std::process::Command::new("fc-match")
        .args(["-f", "%{file}", name])
        .output()
    {
        if output.status.success() {
            let path = String::from_utf8_lossy(&output.stdout);
            let path = path.trim();
            if !path.is_empty() {
                if let Ok(data) = std::fs::read(path) {
                    return Some(data);
                }
            }
        }
    }
    None
}

impl Button {
    pub fn with_config(cfg: ButtonConfig, default_font: &Option<String>) -> Button {
        let text = cfg.text.unwrap_or_else(|| "?".to_string());
        let font_family = cfg.font
            .or_else(|| default_font.clone())
            .unwrap_or_default();
        Button {
            text,
            action: cfg.action,
            active: false,
            font_size: cfg.font_size.unwrap_or(24.0),
            font_family,
            bold: cfg.bold.unwrap_or(false),
            italic: cfg.italic.unwrap_or(false),
            foreground: cfg.foreground
                .and_then(|c| parse_hex_color(&c))
                .unwrap_or(Color32::WHITE),
            background: cfg.background
                .and_then(|c| parse_hex_color(&c))
                .unwrap_or(Color32::from_gray(51)),
            background_active: cfg.background_active
                .and_then(|c| parse_hex_color(&c))
                .unwrap_or(Color32::from_gray(102)),
            corner_radius: cfg.corner_radius.unwrap_or(8.0),
        }
    }
}

/// A layer of function buttons
#[derive(Default)]
pub struct FunctionLayer {
    pub buttons: Vec<Button>,
}

impl FunctionLayer {
    pub fn with_config(cfg: Vec<ButtonConfig>, default_font: &Option<String>) -> FunctionLayer {
        if cfg.is_empty() {
            panic!("Invalid configuration, layer has 0 buttons");
        }
        FunctionLayer {
            buttons: cfg.into_iter().map(|c| Button::with_config(c, default_font)).collect()
        }
    }
}

struct Interface;

impl LibinputInterface for Interface {
    fn open_restricted(&mut self, path: &Path, flags: i32) -> Result<OwnedFd, i32> {
        let mode = flags & O_ACCMODE;

        OpenOptions::new()
            .custom_flags(flags)
            .read(mode == O_RDONLY || mode == O_RDWR)
            .write(mode == O_WRONLY || mode == O_RDWR)
            .open(path)
            .map(|file| file.into())
            .map_err(|err| err.raw_os_error().unwrap())
    }
    fn close_restricted(&mut self, fd: OwnedFd) {
        _ = File::from(fd);
    }
}

fn main() {
    let cli = Cli::parse();
    let mut drm = DrmBackend::open_card().unwrap();
    let _ = panic::catch_unwind(AssertUnwindSafe(|| {
        run(&mut drm, cli.config)
    }));

    // Show crash screen on panic
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
    drm.dirty([ClipRect::new(0, 0, DFR_HEIGHT as u16, DFR_WIDTH as u16)].as_slice()).unwrap();

    // Wait for SIGTERM
    let mut sigset = SigSet::empty();
    sigset.add(Signal::SIGTERM);
    sigset.wait().unwrap();
}

fn run(drm: &mut DrmBackend, config_path: String) -> Result<()> {
    // Setup egui context
    let ctx = Context::default();
    // Painter matches user view: 2008 wide x 60 tall, will be rotated when copying to DRM
    let mut painter = TinySkiaPainter::new(DFR_WIDTH as u32, DFR_HEIGHT as u32);
    let mut input_bridge = InputBridge::new();

    // Load config
    let mut backlight = BacklightManager::new();
    let mut cfg_mgr = ConfigManager::new(config_path);
    let (mut cfg, mut layers, mut state) = match cfg_mgr.load_config() {
        Ok((c, l)) => (c, l, TouchBarState::Normal),
        Err(e) => {
            // Create default config for error state
            let default_cfg = Config {
                show_button_outlines: true,
                enable_pixel_shift: false,
                adaptive_brightness: true,
                active_brightness: 100,
            };
            let default_layers = [FunctionLayer::default(), FunctionLayer::default()];
            (default_cfg, default_layers, TouchBarState::Error(e.to_string()))
        }
    };

    // Load custom fonts and warm up the font atlas
    load_custom_fonts(&ctx, &layers);

    // Run a few warm-up frames to let egui build font atlas
    // IMPORTANT: Must process texture deltas or we lose the font atlas!
    for _ in 0..3 {
        let mut warmup_input = RawInput::default();
        warmup_input.screen_rect = Some(Rect::from_min_size(
            Pos2::ZERO,
            Vec2::new(DFR_WIDTH as f32, DFR_HEIGHT as f32),
        ));
        let full_output = ctx.run(warmup_input, |ctx| {
            // Render both layers to pre-cache all glyphs
            render_function_layer(ctx, &layers[0], &cfg, (0.0, 0.0));
            render_function_layer(ctx, &layers[1], &cfg, (0.0, 0.0));
        });
        // Process texture updates (creates the font atlas)
        let primitives = ctx.tessellate(full_output.shapes, full_output.pixels_per_point);
        painter.paint(&primitives, &full_output.textures_delta);
    }

    // Debug: check glyph coverage after fonts are ready
    debug_glyph_coverage(&ctx, &layers);

    let mut pixel_shift = PixelShiftManager::new();

    // Collect all keys for the KeyEmitter
    let all_keys: Vec<Key> = layers.iter()
        .flat_map(|layer| layer.buttons.iter().map(|b| b.action))
        .collect();
    let mut key_emitter = KeyEmitter::new(&all_keys)?;

    // Setup libinput
    let mut input_tb = Libinput::new_with_udev(Interface);
    let mut input_main = Libinput::new_with_udev(Interface);
    input_tb.udev_assign_seat("seat-touchbar").unwrap();
    input_main.udev_assign_seat("seat0").unwrap();

    // Setup epoll
    let epoll = Epoll::new(EpollCreateFlags::empty()).unwrap();
    epoll.add(input_main.as_fd(), EpollEvent::new(EpollFlags::EPOLLIN, 0)).unwrap();
    epoll.add(input_tb.as_fd(), EpollEvent::new(EpollFlags::EPOLLIN, 1)).unwrap();
    epoll.add(cfg_mgr.fd(), EpollEvent::new(EpollFlags::EPOLLIN, 2)).unwrap();

    // Drop privileges to input and video group
    let groups = ["input", "video"];
    PrivDrop::default()
        .user("nobody")
        .group_list(&groups)
        .apply()
        .unwrap_or_else(|e| { panic!("Failed to drop privileges: {}", e) });

    let mut active_layer = 0;
    let mut needs_redraw = true;
    let mut digitizer: Option<InputDevice> = None;
    let mut touches: HashMap<u32, (usize, usize)> = HashMap::new(); // slot -> (layer, button_idx)

    loop {
        // Check for config updates
        match cfg_mgr.try_update_config(&mut cfg, &mut layers) {
            Ok(true) => {
                active_layer = 0;
                needs_redraw = true;
                state = TouchBarState::Normal;
                load_custom_fonts(&ctx, &layers);  // Reload fonts on config change
                // Warm up font atlas after reload - must process texture deltas!
                for _ in 0..3 {
                    let mut warmup_input = RawInput::default();
                    warmup_input.screen_rect = Some(Rect::from_min_size(
                        Pos2::ZERO,
                        Vec2::new(DFR_WIDTH as f32, DFR_HEIGHT as f32),
                    ));
                    let full_output = ctx.run(warmup_input, |ctx| {
                        render_function_layer(ctx, &layers[0], &cfg, (0.0, 0.0));
                        render_function_layer(ctx, &layers[1], &cfg, (0.0, 0.0));
                    });
                    let primitives = ctx.tessellate(full_output.shapes, full_output.pixels_per_point);
                    painter.paint(&primitives, &full_output.textures_delta);
                }
            }
            Ok(false) => {}
            Err(e) => {
                state = TouchBarState::Error(e.to_string());
                needs_redraw = true;
            }
        }

        // Handle pixel shift
        let mut next_timeout_ms = TIMEOUT_MS;
        if cfg.enable_pixel_shift {
            let (pixel_shift_needs_redraw, pixel_shift_next_timeout_ms) = pixel_shift.update();
            if pixel_shift_needs_redraw {
                needs_redraw = true;
            }
            next_timeout_ms = min(next_timeout_ms, pixel_shift_next_timeout_ms);
        }

        // Build RawInput for egui
        // Screen is 2008 wide x 60 tall (user's perspective)
        let mut raw_input = RawInput::default();
        raw_input.screen_rect = Some(Rect::from_min_size(
            Pos2::ZERO,
            Vec2::new(DFR_WIDTH as f32, DFR_HEIGHT as f32),
        ));

        // Process input events
        input_tb.dispatch().unwrap();
        input_main.dispatch().unwrap();

        for event in &mut input_tb.clone().chain(input_main.clone()) {
            backlight.process_event(&event);
            match event {
                Event::Device(DeviceEvent::Added(evt)) => {
                    let dev = evt.device();
                    if dev.name().contains(" Touch Bar") {
                        digitizer = Some(dev);
                    }
                },
                Event::Keyboard(KeyboardEvent::Key(key)) => {
                    if key.key() == Key::Fn as u32 {
                        let new_layer = match key.key_state() {
                            KeyState::Pressed => 1,
                            KeyState::Released => 0
                        };
                        if active_layer != new_layer {
                            active_layer = new_layer;
                            needs_redraw = true;
                        }
                    }
                },
                Event::Touch(te) => {
                    if Some(te.device()) != digitizer || backlight.current_bl() == 0 {
                        continue
                    }

                    match te {
                        TouchEvent::Down(dn) => {
                            let x = dn.x_transformed(DFR_WIDTH as u32);
                            let y = dn.y_transformed(DFR_HEIGHT as u32);
                            let slot = dn.seat_slot();

                            // Determine which button was hit
                            let num_buttons = layers[active_layer].buttons.len();
                            let btn_idx = (x / (DFR_WIDTH as f64 / num_buttons as f64)) as usize;

                            if btn_idx < num_buttons && button_hit(num_buttons, btn_idx, x, y) {
                                touches.insert(slot, (active_layer, btn_idx));
                                layers[active_layer].buttons[btn_idx].active = true;
                                key_emitter.set_key(layers[active_layer].buttons[btn_idx].action, true);
                                needs_redraw = true;
                            }

                            // Also add egui event
                            let egui_event = input_bridge.handle_touch_down(slot, x, y);
                            raw_input.events.push(egui_event);
                        },
                        TouchEvent::Motion(mtn) => {
                            let x = mtn.x_transformed(DFR_WIDTH as u32);
                            let y = mtn.y_transformed(DFR_HEIGHT as u32);
                            let slot = mtn.seat_slot();

                            if let Some(&(layer, btn_idx)) = touches.get(&slot) {
                                let num_buttons = layers[layer].buttons.len();
                                let hit = button_hit(num_buttons, btn_idx, x, y);
                                if layers[layer].buttons[btn_idx].active != hit {
                                    layers[layer].buttons[btn_idx].active = hit;
                                    key_emitter.set_key(layers[layer].buttons[btn_idx].action, hit);
                                    needs_redraw = true;
                                }
                            }

                            if let Some(egui_event) = input_bridge.handle_touch_motion(slot, x, y) {
                                raw_input.events.push(egui_event);
                            }
                        },
                        TouchEvent::Up(up) => {
                            let slot = up.seat_slot();

                            if let Some((layer, btn_idx)) = touches.remove(&slot) {
                                if layers[layer].buttons[btn_idx].active {
                                    layers[layer].buttons[btn_idx].active = false;
                                    key_emitter.set_key(layers[layer].buttons[btn_idx].action, false);
                                    needs_redraw = true;
                                }
                            }

                            if let Some(egui_event) = input_bridge.handle_touch_up(slot) {
                                raw_input.events.push(egui_event);
                            }
                        }
                        _ => {}
                    }
                },
                _ => {}
            }
        }

        // Render if needed
        if needs_redraw {
            let pixel_shift_offset = if cfg.enable_pixel_shift {
                pixel_shift.get()
            } else {
                (0.0, 0.0)
            };

            // Run egui frames until atlas is ready (max 5 iterations to prevent infinite loop)
            let mut current_input = raw_input; // Use the real input for first frame
            for _ in 0..5 {
                // Run egui frame
                let full_output = ctx.run(current_input, |ctx| {
                    match &state {
                        TouchBarState::Error(msg) => {
                            ui::render_error(ctx, msg);
                        }
                        TouchBarState::Normal => {
                            render_function_layer(ctx, &layers[active_layer], &cfg, pixel_shift_offset);
                        }
                    }
                });

                // Tessellate and paint
                let primitives = ctx.tessellate(full_output.shapes, full_output.pixels_per_point);
                painter.paint(&primitives, &full_output.textures_delta);

                // Check if egui needs another frame (font atlas updates, etc.)
                if !full_output.viewport_output[&ViewportId::ROOT].repaint_delay.is_zero() {
                    break; // Atlas is ready, exit loop
                }

                // Build fresh input for subsequent frames (no events, just screen rect)
                current_input = RawInput::default();
                current_input.screen_rect = Some(Rect::from_min_size(
                    Pos2::ZERO,
                    Vec2::new(DFR_WIDTH as f32, DFR_HEIGHT as f32),
                ));
            }

            // Copy pixels to DRM buffer with 90° rotation
            // Painter: 2008 wide x 60 tall (user view)
            // DRM: 64 wide (stride) x 2008 tall (physical, rotated)
            let pixels = painter.pixels();
            let mut map = drm.map()?;
            let drm_data = map.as_mut();

            let src_width = DFR_WIDTH as usize;   // 2008
            let src_height = DFR_HEIGHT as usize; // 60
            let dst_stride = DFR_STRIDE as usize; // 64

            for src_y in 0..src_height {
                for src_x in 0..src_width {
                    // Rotate 90° CCW: user's left→physical top, user's top→physical right
                    let dst_x = (DFR_HEIGHT - 1) as usize - src_y;
                    let dst_y = src_x;

                    let src_idx = (src_y * src_width + src_x) * 4;
                    let dst_idx = (dst_y * dst_stride + dst_x) * 4;

                    if src_idx + 3 < pixels.len() && dst_idx + 3 < drm_data.len() {
                        drm_data[dst_idx] = pixels[src_idx + 2];     // B
                        drm_data[dst_idx + 1] = pixels[src_idx + 1]; // G
                        drm_data[dst_idx + 2] = pixels[src_idx];     // R
                        drm_data[dst_idx + 3] = pixels[src_idx + 3]; // A
                    }
                }
            }
            drop(map);

            drm.dirty([ClipRect::new(0, 0, DFR_HEIGHT as u16, DFR_WIDTH as u16)].as_slice())?;
            needs_redraw = false;
        }

        // Update backlight
        backlight.update_backlight(&cfg);

        // Wait for events with timeout
        match epoll.wait(&mut [EpollEvent::new(EpollFlags::EPOLLIN, 0)], next_timeout_ms as isize) {
            Err(Errno::EINTR) | Ok(_) => { }
            e => { e?; }
        };
    }
}

/// Check if a touch at (x, y) hits button at index btn_idx
fn button_hit(num_buttons: usize, btn_idx: usize, x: f64, y: f64) -> bool {
    let button_width = (DFR_WIDTH - (BUTTON_SPACING_PX * (num_buttons - 1) as i32)) as f64 / num_buttons as f64;
    let left_edge = btn_idx as f64 * (button_width + BUTTON_SPACING_PX as f64);
    if x < left_edge || x > (left_edge + button_width) {
        return false
    }
    y > 0.1 * DFR_HEIGHT as f64 && y < 0.9 * DFR_HEIGHT as f64
}

/// Render a function layer using egui
fn render_function_layer(ctx: &Context, layer: &FunctionLayer, cfg: &Config, pixel_shift: (f64, f64)) {
    CentralPanel::default()
        .frame(Frame::none().fill(Color32::BLACK))
        .show(ctx, |ui| {
            let available_width = ui.available_width();
            let available_height = ui.available_height();
            let num_buttons = layer.buttons.len();
            let pixel_shift_width = if cfg.enable_pixel_shift { PIXEL_SHIFT_WIDTH_PX as f32 } else { 0.0 };
            let total_spacing = BUTTON_SPACING_PX as f32 * (num_buttons - 1) as f32;
            let button_width = (available_width - pixel_shift_width - total_spacing) / num_buttons as f32;
            let button_height = available_height * 0.7;
            let button_y = (available_height - button_height) / 2.0 + pixel_shift.1 as f32;

            let (px_shift_x, _px_shift_y) = pixel_shift;
            let start_x = (pixel_shift_width / 2.0) + px_shift_x as f32;

            for (i, button) in layer.buttons.iter().enumerate() {
                let button_x = start_x + i as f32 * (button_width + BUTTON_SPACING_PX as f32);
                let rect = Rect::from_min_size(
                    Pos2::new(button_x, button_y),
                    Vec2::new(button_width, button_height),
                );

                // Determine button color
                let bg_color = if button.active {
                    button.background_active
                } else if cfg.show_button_outlines {
                    button.background
                } else {
                    Color32::BLACK
                };

                // Draw rounded rectangle background
                ui.painter().rect_filled(rect, button.corner_radius, bg_color);

                // Draw text (custom fonts are added to Proportional family)
                ui.painter().text(
                    rect.center(),
                    Align2::CENTER_CENTER,
                    &button.text,
                    FontId::new(button.font_size, FontFamily::Proportional),
                    button.foreground,
                );
            }
        });
}
