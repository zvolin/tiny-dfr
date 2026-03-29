//! Key emission via uinput virtual device

use std::collections::HashSet;
use std::fs::{File, OpenOptions};
use input_linux::{uinput::UInputHandle, EventKind, Key, SynchronizeKind};
use input_linux_sys::{uinput_setup, input_id, timeval, input_event};
use libc::c_char;
use anyhow::Result;

/// Emits key events via uinput
pub struct KeyEmitter {
    uinput: UInputHandle<File>,
    held_keys: HashSet<Key>,
}

impl KeyEmitter {
    /// Create a new key emitter for the given keys
    pub fn new(keys: &[Key]) -> Result<Self> {
        let uinput = UInputHandle::new(OpenOptions::new().write(true).open("/dev/uinput")?);
        uinput.set_evbit(EventKind::Key)?;

        for key in keys {
            uinput.set_keybit(*key)?;
        }

        let mut dev_name_c = [0 as c_char; 80];
        let dev_name = b"Dynamic Function Row Virtual Input Device";
        for (i, &byte) in dev_name.iter().enumerate() {
            dev_name_c[i] = byte as c_char;
        }

        uinput.dev_setup(&uinput_setup {
            id: input_id {
                bustype: 0x19,
                vendor: 0x1209,
                product: 0x316E,
                version: 1,
            },
            ff_effects_max: 0,
            name: dev_name_c,
        })?;
        uinput.dev_create()?;

        Ok(Self {
            uinput,
            held_keys: HashSet::new(),
        })
    }

    /// Set key state (pressed or released)
    pub fn set_key(&mut self, key: Key, pressed: bool) {
        if pressed && !self.held_keys.contains(&key) {
            self.held_keys.insert(key);
            self.emit(key, 1);
        } else if !pressed && self.held_keys.contains(&key) {
            self.held_keys.remove(&key);
            self.emit(key, 0);
        }
    }

    fn emit(&mut self, key: Key, value: i32) {
        self.uinput.write(&[input_event {
            type_: EventKind::Key as u16,
            code: key as u16,
            value,
            time: timeval { tv_sec: 0, tv_usec: 0 },
        }]).unwrap();
        self.uinput.write(&[input_event {
            type_: EventKind::Synchronize as u16,
            code: SynchronizeKind::Report as u16,
            value: 0,
            time: timeval { tv_sec: 0, tv_usec: 0 },
        }]).unwrap();
    }
}
