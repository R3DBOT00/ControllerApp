//! Nonexclusive desktop-input activity detection. No key decoding, storage,
//! logging, forwarding, grabs or suppression. Only a boolean leaves this module.
use evdev::{Device, EventType, InputEvent, KeyCode as K};
use std::{
    collections::BTreeMap,
    io,
    path::PathBuf,
    time::{Duration, Instant},
};

pub fn is_desktop_input(device: &Device) -> bool {
    if device.name().unwrap_or("").starts_with("ControllerApp") {
        return false;
    }
    device.supported_keys().is_some_and(|keys| {
        !keys.contains(K::BTN_SOUTH)
            && ((keys.contains(K::KEY_A) && keys.contains(K::KEY_Z))
                || keys.contains(K::BTN_LEFT)
                || keys.contains(K::BTN_TOUCH))
    })
}

pub fn is_activity(event: &InputEvent) -> bool {
    match event.event_type() {
        // Never examine the key code or translate it into a character.
        EventType::KEY => event.value() > 0,
        EventType::RELATIVE => event.value() != 0,
        EventType::ABSOLUTE => true,
        _ => false,
    }
}

pub struct DesktopActivity {
    devices: BTreeMap<PathBuf, Device>,
    scan: Instant,
}
impl Default for DesktopActivity {
    fn default() -> Self {
        Self {
            devices: BTreeMap::new(),
            scan: Instant::now() - Duration::from_secs(2),
        }
    }
}
impl DesktopActivity {
    pub fn device_count(&self) -> usize {
        self.devices.len()
    }

    pub fn poll(&mut self) -> bool {
        if self.scan.elapsed() >= Duration::from_secs(1) {
            for (path, device) in evdev::enumerate() {
                if self.devices.len() >= 32 {
                    break;
                }
                if !self.devices.contains_key(&path)
                    && is_desktop_input(&device)
                    && device.set_nonblocking(true).is_ok()
                {
                    self.devices.insert(path, device);
                }
            }
            self.scan = Instant::now();
        }
        let mut active = false;
        self.devices.retain(|path, device| {
            if !path.exists() {
                return false;
            }
            match device.fetch_events() {
                Ok(events) => {
                    // Drain a bounded batch; do not collect or retain raw events.
                    for event in events.take(256) {
                        active |= is_activity(&event);
                    }
                    true
                }
                Err(error) => error.kind() == io::ErrorKind::WouldBlock,
            }
        });
        active
    }
}
