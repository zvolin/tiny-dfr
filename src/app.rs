//! Main application loop

use std::{
    collections::HashMap,
    cmp::min,
    fs::{File, OpenOptions},
    os::{fd::AsFd, unix::{io::OwnedFd, fs::OpenOptionsExt}},
    path::Path,
};
use anyhow::Result;
use egui::{Context, RawInput, Rect, Pos2, Vec2, ViewportId};
use input::{
    Libinput, LibinputInterface, Device as InputDevice,
    event::{
        Event, device::DeviceEvent, EventTrait,
        touch::{TouchEvent, TouchEventPosition, TouchEventSlot},
        keyboard::{KeyboardEvent, KeyboardEventTrait, KeyState},
    },
};
use input_linux::Key;
use libc::{O_ACCMODE, O_RDONLY, O_RDWR, O_WRONLY};
use nix::{
    sys::epoll::{Epoll, EpollCreateFlags, EpollEvent, EpollFlags},
    errno::Errno,
};
use privdrop::PrivDrop;

use crate::core::{TouchBarState, FunctionLayer, DFR_WIDTH, DFR_HEIGHT, TIMEOUT_MS};
use crate::config::{Config, ConfigWatcher};
use crate::display::{DrmBackend, TinySkiaPainter, render_function_layer, render_error, warm_up_fonts, copy_to_drm};
use crate::input::{InputBridge, KeyEmitter};
use crate::system::{BacklightManager, PixelShiftManager, load_custom_fonts};

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
        drop(File::from(fd));
    }
}

/// Run the Touch Bar daemon
pub fn run(drm: &mut DrmBackend, config_path: String) -> Result<()> {
    let ctx = Context::default();
    let mut painter = TinySkiaPainter::new(DFR_WIDTH as u32, DFR_HEIGHT as u32);
    let mut input_bridge = InputBridge::new();
    let mut backlight = BacklightManager::new();
    let mut config_watcher = ConfigWatcher::new(config_path);

    // Load initial config
    let (mut cfg, mut layers, mut state) = match config_watcher.load() {
        Ok((c, l)) => (c, l, TouchBarState::Normal),
        Err(e) => (Config::default(), default_layers(), TouchBarState::Error(e.to_string())),
    };

    // Initialize fonts
    load_custom_fonts(&ctx, &layers);
    warm_up_fonts(&ctx, &mut painter, &layers, &cfg);

    let mut pixel_shift = PixelShiftManager::new();

    // Setup key emitter
    let all_keys: Vec<Key> = layers
        .iter()
        .flat_map(|layer| layer.buttons.iter().map(|b| b.action))
        .collect();
    let mut key_emitter = KeyEmitter::new(&all_keys)?;

    // Setup libinput
    let mut input_tb = Libinput::new_with_udev(Interface);
    let mut input_main = Libinput::new_with_udev(Interface);
    input_tb.udev_assign_seat("seat-touchbar").unwrap();
    input_main.udev_assign_seat("seat0").unwrap();

    // Setup epoll
    let epoll = Epoll::new(EpollCreateFlags::empty())?;
    epoll.add(input_main.as_fd(), EpollEvent::new(EpollFlags::EPOLLIN, 0))?;
    epoll.add(input_tb.as_fd(), EpollEvent::new(EpollFlags::EPOLLIN, 1))?;
    epoll.add(config_watcher.fd(), EpollEvent::new(EpollFlags::EPOLLIN, 2))?;

    // Drop privileges
    PrivDrop::default()
        .user("nobody")
        .group_list(&["input", "video"])
        .apply()
        .unwrap_or_else(|e| panic!("Failed to drop privileges: {}", e));

    let mut active_layer = 0;
    let mut needs_redraw = true;
    let mut digitizer: Option<InputDevice> = None;
    let mut touches: HashMap<u32, (usize, usize)> = HashMap::new();

    loop {
        // Check config updates
        if let Ok(true) = config_watcher.try_update(&mut cfg, &mut layers) {
            active_layer = 0;
            needs_redraw = true;
            state = TouchBarState::Normal;
            load_custom_fonts(&ctx, &layers);
            warm_up_fonts(&ctx, &mut painter, &layers, &cfg);
        }

        // Handle pixel shift
        let mut next_timeout = TIMEOUT_MS;
        if cfg.enable_pixel_shift {
            let (shift_redraw, shift_timeout) = pixel_shift.update();
            if shift_redraw {
                needs_redraw = true;
            }
            next_timeout = min(next_timeout, shift_timeout);
        }

        // Build egui input
        let mut raw_input = RawInput {
            screen_rect: Some(Rect::from_min_size(
                Pos2::ZERO,
                Vec2::new(DFR_WIDTH as f32, DFR_HEIGHT as f32),
            )),
            ..Default::default()
        };

        // Process input events
        input_tb.dispatch().unwrap();
        input_main.dispatch().unwrap();

        for event in input_tb.clone().chain(input_main.clone()) {
            backlight.process_event(&event);

            match event {
                Event::Device(DeviceEvent::Added(evt)) => {
                    let dev = evt.device();
                    if dev.name().contains(" Touch Bar") {
                        digitizer = Some(dev);
                    }
                }
                Event::Keyboard(KeyboardEvent::Key(key)) => {
                    if key.key() == Key::Fn as u32 {
                        let new_layer = match key.key_state() {
                            KeyState::Pressed => 1,
                            KeyState::Released => 0,
                        };
                        if active_layer != new_layer {
                            active_layer = new_layer;
                            needs_redraw = true;
                        }
                    }
                }
                Event::Touch(te) => {
                    if Some(te.device()) != digitizer || backlight.current_bl() == 0 {
                        continue;
                    }
                    handle_touch_event(
                        te,
                        &mut layers,
                        active_layer,
                        &mut touches,
                        &mut key_emitter,
                        &mut needs_redraw,
                        &mut raw_input,
                        &mut input_bridge,
                    );
                }
                _ => {}
            }
        }

        // Render if needed
        if needs_redraw {
            let pixel_offset = if cfg.enable_pixel_shift {
                pixel_shift.get()
            } else {
                (0.0, 0.0)
            };

            render_frame(&ctx, &mut painter, &layers, active_layer, &cfg, &state, pixel_offset, raw_input)?;
            copy_to_drm(&painter, drm)?;
            needs_redraw = false;
        }

        // Update backlight
        backlight.update_backlight(&cfg);

        // Wait for events
        match epoll.wait(&mut [EpollEvent::empty()], next_timeout as isize) {
            Err(Errno::EINTR) | Ok(_) => {}
            e => { e?; }
        }
    }
}

fn default_layers() -> [FunctionLayer; 2] {
    [FunctionLayer::default(), FunctionLayer::default()]
}

#[allow(clippy::too_many_arguments)]
fn handle_touch_event(
    te: TouchEvent,
    layers: &mut [FunctionLayer; 2],
    active_layer: usize,
    touches: &mut HashMap<u32, (usize, usize)>,
    key_emitter: &mut KeyEmitter,
    needs_redraw: &mut bool,
    raw_input: &mut RawInput,
    input_bridge: &mut InputBridge,
) {
    match te {
        TouchEvent::Down(dn) => {
            let x = dn.x_transformed(DFR_WIDTH as u32);
            let y = dn.y_transformed(DFR_HEIGHT as u32);
            let slot = dn.seat_slot();

            let layer = &mut layers[active_layer];
            let num_buttons = layer.buttons.len();
            let btn_idx = (x / (DFR_WIDTH as f64 / num_buttons as f64)) as usize;

            if btn_idx < num_buttons && layer.button_hit(btn_idx, x, y) {
                touches.insert(slot, (active_layer, btn_idx));
                layer.buttons[btn_idx].active = true;
                key_emitter.set_key(layer.buttons[btn_idx].action, true);
                *needs_redraw = true;
            }

            raw_input.events.push(input_bridge.handle_touch_down(slot, x, y));
        }
        TouchEvent::Motion(mtn) => {
            let x = mtn.x_transformed(DFR_WIDTH as u32);
            let y = mtn.y_transformed(DFR_HEIGHT as u32);
            let slot = mtn.seat_slot();

            if let Some(&(layer_idx, btn_idx)) = touches.get(&slot) {
                let layer = &mut layers[layer_idx];
                let hit = layer.button_hit(btn_idx, x, y);
                if layer.buttons[btn_idx].active != hit {
                    layer.buttons[btn_idx].active = hit;
                    key_emitter.set_key(layer.buttons[btn_idx].action, hit);
                    *needs_redraw = true;
                }
            }

            if let Some(evt) = input_bridge.handle_touch_motion(slot, x, y) {
                raw_input.events.push(evt);
            }
        }
        TouchEvent::Up(up) => {
            let slot = up.seat_slot();

            if let Some((layer_idx, btn_idx)) = touches.remove(&slot) {
                let layer = &mut layers[layer_idx];
                if layer.buttons[btn_idx].active {
                    layer.buttons[btn_idx].active = false;
                    key_emitter.set_key(layer.buttons[btn_idx].action, false);
                    *needs_redraw = true;
                }
            }

            if let Some(evt) = input_bridge.handle_touch_up(slot) {
                raw_input.events.push(evt);
            }
        }
        _ => {}
    }
}

#[allow(clippy::too_many_arguments)]
fn render_frame(
    ctx: &Context,
    painter: &mut TinySkiaPainter,
    layers: &[FunctionLayer; 2],
    active_layer: usize,
    cfg: &Config,
    state: &TouchBarState,
    pixel_offset: (f64, f64),
    initial_input: RawInput,
) -> Result<()> {
    let mut current_input = initial_input;

    for _ in 0..5 {
        let output = ctx.run(current_input, |ctx| {
            match state {
                TouchBarState::Error(msg) => render_error(ctx, msg),
                TouchBarState::Normal => render_function_layer(ctx, &layers[active_layer], cfg, pixel_offset),
            }
        });

        let primitives = ctx.tessellate(output.shapes, output.pixels_per_point);
        painter.paint(&primitives, &output.textures_delta);

        if !output.viewport_output[&ViewportId::ROOT].repaint_delay.is_zero() {
            break;
        }

        current_input = RawInput::default();
        current_input.screen_rect = Some(Rect::from_min_size(
            Pos2::ZERO,
            Vec2::new(DFR_WIDTH as f32, DFR_HEIGHT as f32),
        ));
    }

    Ok(())
}
