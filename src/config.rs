use std::{
    fs::read_to_string,
    os::fd::AsFd
};
use anyhow::Result;
use crate::FunctionLayer;
use input_linux::Key;
use nix::{
    errno::Errno,
    sys::inotify::{AddWatchFlags, InitFlags, Inotify, WatchDescriptor}
};
use serde::Deserialize;

pub struct Config {
    pub show_button_outlines: bool,
    pub enable_pixel_shift: bool,
    pub adaptive_brightness: bool,
    pub active_brightness: u32,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct ConfigProxy {
    media_layer_default: Option<bool>,
    show_button_outlines: Option<bool>,
    enable_pixel_shift: Option<bool>,
    #[allow(dead_code)]
    font_template: Option<String>, // Kept for compatibility, but ignored (egui handles fonts)
    default_font: Option<String>,
    adaptive_brightness: Option<bool>,
    active_brightness: Option<u32>,
    primary_layer_keys: Option<Vec<ButtonConfig>>,
    media_layer_keys: Option<Vec<ButtonConfig>>
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct ButtonConfig {
    pub text: Option<String>,
    pub action: Key,
    // Style options (all optional, fallback to defaults)
    pub font_size: Option<f32>,
    pub font: Option<String>,             // Font family name (e.g., "Symbols Nerd Font")
    pub bold: Option<bool>,
    pub italic: Option<bool>,
    pub foreground: Option<String>,       // hex color like "#FFFFFF"
    pub background: Option<String>,       // hex color like "#333333"
    pub background_active: Option<String>, // hex color when pressed
    pub corner_radius: Option<f32>,
}

fn load_config(path: &str) -> Result<(Config, [FunctionLayer; 2])> {
    let proxy = toml::from_str::<ConfigProxy>(&read_to_string(path)?)?;
    let default_font = proxy.default_font.clone();
    let media_layer = FunctionLayer::with_config(
        proxy.media_layer_keys.ok_or_else(|| anyhow::anyhow!("Missing MediaLayerKeys"))?,
        &default_font
    );
    let fkey_layer = FunctionLayer::with_config(
        proxy.primary_layer_keys.ok_or_else(|| anyhow::anyhow!("Missing PrimaryLayerKeys"))?,
        &default_font
    );
    let layers = if proxy.media_layer_default.unwrap_or(false) { [media_layer, fkey_layer] } else { [fkey_layer, media_layer] };
    let cfg = Config {
        show_button_outlines: proxy.show_button_outlines.unwrap_or(true),
        enable_pixel_shift: proxy.enable_pixel_shift.unwrap_or(true),
        adaptive_brightness: proxy.adaptive_brightness.unwrap_or(true),
        active_brightness: proxy.active_brightness.unwrap_or(100),
    };
    Ok((cfg, layers))
}

pub struct ConfigManager {
    inotify_fd: Inotify,
    watch_desc: Option<WatchDescriptor>,
    config_path: String,
}

fn arm_inotify(inotify_fd: &Inotify, config_path: &str) -> Option<WatchDescriptor> {
    let flags = AddWatchFlags::IN_MOVED_TO | AddWatchFlags::IN_CLOSE | AddWatchFlags::IN_ONESHOT;
    match inotify_fd.add_watch(config_path, flags) {
        Ok(wd) => Some(wd),
        Err(Errno::ENOENT) => None,
        e => Some(e.unwrap())
    }
}

impl ConfigManager {
    pub fn new(config_path: String) -> ConfigManager {
        let inotify_fd = Inotify::init(InitFlags::IN_NONBLOCK).unwrap();
        let watch_desc = arm_inotify(&inotify_fd, &config_path);
        ConfigManager {
            inotify_fd, watch_desc, config_path
        }
    }

    pub fn load_config(&self) -> Result<(Config, [FunctionLayer; 2])> {
        load_config(&self.config_path)
    }

    pub fn try_update_config(&mut self, cfg: &mut Config, layers: &mut [FunctionLayer; 2]) -> Result<bool> {
        if self.watch_desc.is_none() {
            self.watch_desc = arm_inotify(&self.inotify_fd, &self.config_path);
            return Ok(false);
        }
        let evts = match self.inotify_fd.read_events() {
            Ok(e) => e,
            Err(Errno::EAGAIN) => Vec::new(),
            r => r?,
        };
        let mut ret = false;
        for evt in evts {
            if evt.wd != self.watch_desc.unwrap() {
                continue
            }
            let (new_cfg, new_layers) = load_config(&self.config_path)?;
            *cfg = new_cfg;
            *layers = new_layers;
            ret = true;
            self.watch_desc = arm_inotify(&self.inotify_fd, &self.config_path);
        }
        Ok(ret)
    }

    pub fn fd(&self) -> &impl AsFd {
        &self.inotify_fd
    }
}
