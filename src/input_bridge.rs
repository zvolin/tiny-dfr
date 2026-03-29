use std::collections::HashMap;
use egui::{Event, Pos2, TouchId, TouchPhase, TouchDeviceId};

pub struct InputBridge {
    active_touches: HashMap<u32, Pos2>,
    device_id: TouchDeviceId,
}

impl InputBridge {
    pub fn new() -> Self {
        Self {
            active_touches: HashMap::new(),
            device_id: TouchDeviceId(0),
        }
    }

    pub fn handle_touch_down(&mut self, slot: u32, x: f64, y: f64) -> Event {
        let pos = self.transform(x, y);
        self.active_touches.insert(slot, pos);
        Event::Touch {
            device_id: self.device_id,
            id: TouchId(slot as u64),
            phase: TouchPhase::Start,
            pos,
            force: None,
        }
    }

    pub fn handle_touch_motion(&mut self, slot: u32, x: f64, y: f64) -> Option<Event> {
        if !self.active_touches.contains_key(&slot) {
            return None;
        }
        let pos = self.transform(x, y);
        self.active_touches.insert(slot, pos);
        Some(Event::Touch {
            device_id: self.device_id,
            id: TouchId(slot as u64),
            phase: TouchPhase::Move,
            pos,
            force: None,
        })
    }

    pub fn handle_touch_up(&mut self, slot: u32) -> Option<Event> {
        let pos = self.active_touches.remove(&slot)?;
        Some(Event::Touch {
            device_id: self.device_id,
            id: TouchId(slot as u64),
            phase: TouchPhase::End,
            pos,
            force: None,
        })
    }

    fn transform(&self, x: f64, y: f64) -> Pos2 {
        // Rotate 90 degrees - swap x and y
        Pos2::new(y as f32, x as f32)
    }
}
