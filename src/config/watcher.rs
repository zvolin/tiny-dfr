//! Configuration file watcher using inotify

use std::os::fd::AsFd;
use nix::{
    errno::Errno,
    sys::inotify::{AddWatchFlags, InitFlags, Inotify, WatchDescriptor},
};

use crate::core::FunctionLayer;
use super::types::Config;
use super::parser::load_config;

/// Watches config file for changes
pub struct ConfigWatcher {
    inotify: Inotify,
    watch_desc: Option<WatchDescriptor>,
    path: String,
}

fn arm_inotify(inotify: &Inotify, path: &str) -> Option<WatchDescriptor> {
    let flags = AddWatchFlags::IN_MOVED_TO | AddWatchFlags::IN_CLOSE | AddWatchFlags::IN_ONESHOT;
    match inotify.add_watch(path, flags) {
        Ok(wd) => Some(wd),
        Err(Errno::ENOENT) => None,
        e => Some(e.unwrap()),
    }
}

impl ConfigWatcher {
    /// Create a new watcher for the given config path
    pub fn new(path: String) -> Self {
        let inotify = Inotify::init(InitFlags::IN_NONBLOCK).unwrap();
        let watch_desc = arm_inotify(&inotify, &path);
        Self {
            inotify,
            watch_desc,
            path,
        }
    }

    /// Get the file descriptor for epoll registration
    pub fn fd(&self) -> impl AsFd + '_ {
        &self.inotify
    }

    /// Load initial configuration
    pub fn load(&self) -> anyhow::Result<(Config, [FunctionLayer; 2])> {
        load_config(&self.path)
    }

    /// Check for config updates and reload if changed
    ///
    /// Returns Ok(true) if config was reloaded, Ok(false) if no changes
    pub fn try_update(
        &mut self,
        cfg: &mut Config,
        layers: &mut [FunctionLayer; 2],
    ) -> anyhow::Result<bool> {
        if self.watch_desc.is_none() {
            self.watch_desc = arm_inotify(&self.inotify, &self.path);
            return Ok(false);
        }

        let events = match self.inotify.read_events() {
            Ok(e) => e,
            Err(Errno::EAGAIN) => return Ok(false),
            Err(e) => return Err(e.into()),
        };

        let mut reloaded = false;
        for evt in events {
            if Some(evt.wd) != self.watch_desc {
                continue;
            }
            let (new_cfg, new_layers) = load_config(&self.path)?;
            *cfg = new_cfg;
            *layers = new_layers;
            reloaded = true;
            self.watch_desc = arm_inotify(&self.inotify, &self.path);
        }

        Ok(reloaded)
    }
}
