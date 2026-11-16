use crate::{
    layout::Key,
    model::{Button, Effect, Engine, Pad, View},
};
use evdev::{
    AbsoluteAxisCode as Abs, AttributeSet, Device, EventType, FFEffect, FFEffectData, FFEffectKind,
    FFReplay, FFTrigger, InputEvent, KeyCode as K, RelativeAxisCode as Rel, uinput::VirtualDevice,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    io,
    path::PathBuf,
    sync::{
        Arc, Mutex,
        mpsc::{Receiver, Sender},
    },
    thread,
    time::{Duration, Instant},
};

#[derive(Clone, Debug)]
pub struct Stroke {
    pub code: u16,
    pub shift: bool,
    pub altgr: bool,
    pub alphabetic: bool,
}
pub type TextMap = BTreeMap<char, Vec<Stroke>>;
pub enum Command {
    Show(bool),
    Toggle,
    Focus { editable: bool, protected: bool },
    Fullscreen(bool),
    TextMap(TextMap),
    Stop,
}
#[derive(Default)]
pub struct Shared {
    pub view: View,
    pub ticks: u64,
    pub events: u64,
    pub device: String,
    pub mapped: bool,
    pub focus_monitor: bool,
    pub kwin_monitor: bool,
    pub stopped: bool,
    pub layout_group: Option<i32>,
    pub desktop_input_devices: usize,
}
pub type State = Arc<Mutex<Shared>>;

pub struct Gamepad {
    device: Device,
    path: PathBuf,
    pub state: Pad,
    ranges: BTreeMap<u16, (i32, i32)>,
    effect: Option<FFEffect>,
    rumble_until: Option<Instant>,
    pub events: u64,
}
pub fn is_gamepad(device: &Device) -> bool {
    !device.name().unwrap_or("").starts_with("ControllerApp")
        && device
            .supported_keys()
            .is_some_and(|keys| keys.contains(K::BTN_SOUTH))
        && device
            .supported_absolute_axes()
            .is_some_and(|a| a.contains(Abs::ABS_X) && a.contains(Abs::ABS_Y))
}
impl Gamepad {
    pub fn find() -> Option<Self> {
        let mut candidates: Vec<_> = evdev::enumerate().filter(|(_, d)| is_gamepad(d)).collect();
        candidates.sort_by(|a, b| a.0.cmp(&b.0));
        for (path, device) in candidates {
            if let Ok(pad) = Self::from_device(path, device) {
                eprintln!(
                    "Controller connected: {} ({})",
                    pad.state.name,
                    pad.path.display()
                );
                return Some(pad);
            }
        }
        None
    }
    pub fn open(path: impl AsRef<std::path::Path>) -> io::Result<Self> {
        Self::from_device(path.as_ref().into(), Device::open(path.as_ref())?)
    }
    fn from_device(path: PathBuf, device: Device) -> io::Result<Self> {
        if !is_gamepad(&device) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Device is not a gamepad",
            ));
        }
        device.set_nonblocking(true)?;
        let infos: Vec<_> = device.get_absinfo()?.collect();
        let keys = device.get_key_state()?;
        let mut pad = Self {
            state: Pad {
                connected: true,
                name: device.name().unwrap_or("Gamepad").into(),
                ..Pad::default()
            },
            ranges: BTreeMap::new(),
            device,
            path,
            effect: None,
            rumble_until: None,
            events: 0,
        };
        for (axis, info) in infos {
            pad.ranges.insert(axis.0, (info.minimum(), info.maximum()));
            pad.axis(axis.0, info.value());
        }
        for k in keys.iter() {
            pad.button(k.0, true);
        }
        Ok(pad)
    }
    pub fn poll(&mut self) -> io::Result<()> {
        // A bounded nonblocking read is independent of the Wayland frame clock.
        let events: Vec<_> = match self.device.fetch_events() {
            Ok(events) => events.take(512).collect(),
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => Vec::new(),
            Err(e) => return Err(e),
        };
        for event in events {
            match event.event_type() {
                EventType::ABSOLUTE => {
                    self.events += 1;
                    self.axis(event.code(), event.value());
                }
                EventType::KEY => {
                    self.events += 1;
                    self.button(event.code(), event.value() != 0);
                }
                _ => {}
            }
        }
        if !self.path.exists() {
            return Err(io::Error::new(
                io::ErrorKind::NotConnected,
                "Controller disconnected",
            ));
        }
        if self.rumble_until.is_some_and(|t| Instant::now() >= t) {
            self.stop_rumble();
        }
        Ok(())
    }
    fn axis(&mut self, code: u16, value: i32) {
        let index = match Abs(code) {
            Abs::ABS_X => Some(0),
            Abs::ABS_Y => Some(1),
            Abs::ABS_RX => Some(2),
            Abs::ABS_RY => Some(3),
            _ => None,
        };
        if let Some(index) = index
            && let Some(range) = self.ranges.get(&code)
        {
            self.state.axes[index] = normalize(value, *range);
        }
        if code == Abs::ABS_HAT0X.0 {
            self.state.buttons[Button::Left as usize] = value < 0;
            self.state.buttons[Button::Right as usize] = value > 0;
        }
        if code == Abs::ABS_HAT0Y.0 {
            self.state.buttons[Button::Up as usize] = value < 0;
            self.state.buttons[Button::Down as usize] = value > 0;
        }
    }
    fn button(&mut self, code: u16, down: bool) {
        let b = match K(code) {
            K::BTN_SOUTH => Button::A,
            K::BTN_EAST => Button::B,
            K::BTN_WEST => Button::X,
            K::BTN_NORTH => Button::Y,
            K::BTN_TL => Button::Lb,
            K::BTN_TR => Button::Rb,
            K::BTN_SELECT => Button::Back,
            K::BTN_START => Button::Start,
            K::BTN_THUMBR => Button::R3,
            K::BTN_DPAD_UP => Button::Up,
            K::BTN_DPAD_DOWN => Button::Down,
            K::BTN_DPAD_LEFT => Button::Left,
            K::BTN_DPAD_RIGHT => Button::Right,
            _ => return,
        };
        self.state.buttons[b as usize] = down;
    }
    pub fn rumble(&mut self, magnitude: u16, ms: u16) -> io::Result<()> {
        let data = FFEffectData {
            direction: 0,
            trigger: FFTrigger::default(),
            replay: FFReplay {
                length: ms.min(250),
                delay: 0,
            },
            kind: FFEffectKind::Rumble {
                strong_magnitude: magnitude,
                weak_magnitude: magnitude,
            },
        };
        if let Some(effect) = self.effect.as_mut() {
            effect.stop()?;
            effect.update(data)?;
        } else {
            self.effect = Some(self.device.upload_ff_effect(data)?);
        }
        if let Some(effect) = self.effect.as_mut() {
            effect.play(1)?;
        }
        self.rumble_until = Some(Instant::now() + Duration::from_millis(u64::from(ms.min(250))));
        Ok(())
    }
    fn stop_rumble(&mut self) {
        if let Some(effect) = self.effect.as_mut() {
            let _ = effect.stop();
        }
        self.rumble_until = None;
    }
}
impl Drop for Gamepad {
    fn drop(&mut self) {
        self.stop_rumble();
    }
}
pub fn normalize(value: i32, (min, max): (i32, i32)) -> f32 {
    if max <= min {
        return 0.;
    }
    (((f64::from(value) - f64::from(min)) / (f64::from(max) - f64::from(min)) * 2. - 1.) * 32767.)
        .clamp(-32767., 32767.) as f32
}

pub struct Output {
    mouse: VirtualDevice,
    keyboard: VirtualDevice,
    held: BTreeSet<u16>,
    buttons: [bool; 2],
    scroll: [i32; 2],
    pub text_map: TextMap,
}
impl Output {
    pub fn new() -> io::Result<Self> {
        let mouse = VirtualDevice::builder()?
            .name("ControllerApp Mouse")
            .with_keys(&AttributeSet::from_iter([K::BTN_LEFT, K::BTN_RIGHT]))?
            .with_relative_axes(&AttributeSet::from_iter([
                Rel::REL_X,
                Rel::REL_Y,
                Rel::REL_WHEEL,
                Rel::REL_HWHEEL,
                Rel::REL_WHEEL_HI_RES,
                Rel::REL_HWHEEL_HI_RES,
            ]))?
            .build()?;
        let keyboard = VirtualDevice::builder()?
            .name("ControllerApp Keyboard")
            .with_keys(&AttributeSet::from_iter((1..=248).map(K)))?
            .build()?;
        Ok(Self {
            mouse,
            keyboard,
            held: BTreeSet::new(),
            buttons: [false; 2],
            scroll: [0; 2],
            text_map: BTreeMap::new(),
        })
    }
    fn key(&mut self, code: u16, down: bool) -> io::Result<()> {
        self.keyboard
            .emit(&[InputEvent::new(EventType::KEY.0, code, i32::from(down))])?;
        // Keep transitions timestamp-separated, including modifiers and dead
        // keys. A bounded 2 ms dwell avoids zero-duration taps under load.
        thread::sleep(Duration::from_millis(2));
        if down {
            self.held.insert(code);
        } else {
            self.held.remove(&code);
        }
        Ok(())
    }
    fn tap(&mut self, code: u16) -> io::Result<()> {
        self.key(code, true)?;
        self.key(code, false)
    }
    pub fn text(&mut self, c: char) -> io::Result<()> {
        let sequence = self.text_map.get(&c).cloned().ok_or_else(|| {
            io::Error::other("Character unavailable in the active keyboard layout")
        })?;
        let caps = caps_lock_state().unwrap_or(false);
        for mut s in sequence {
            if s.alphabetic && caps {
                s.shift = !s.shift;
            }
            let result = (|| {
                if s.shift {
                    self.key(K::KEY_LEFTSHIFT.0, true)?;
                }
                if s.altgr {
                    self.key(K::KEY_RIGHTALT.0, true)?;
                }
                self.tap(s.code)?;
                if s.altgr {
                    self.key(K::KEY_RIGHTALT.0, false)?;
                }
                if s.shift {
                    self.key(K::KEY_LEFTSHIFT.0, false)?;
                }
                Ok(())
            })();
            if result.is_err() {
                self.release_all();
                return result;
            }
        }
        Ok(())
    }
    pub fn apply(&mut self, e: &Effect) -> io::Result<()> {
        match *e {
            Effect::Move(x, y) => self.mouse.emit(&[
                InputEvent::new(EventType::RELATIVE.0, Rel::REL_X.0, x),
                InputEvent::new(EventType::RELATIVE.0, Rel::REL_Y.0, y),
            ]),
            Effect::Mouse(i, down) => {
                self.mouse.emit(&[InputEvent::new(
                    EventType::KEY.0,
                    if i == 0 {
                        K::BTN_LEFT.0
                    } else {
                        K::BTN_RIGHT.0
                    },
                    i32::from(down),
                )])?;
                self.buttons[usize::from(i)] = down;
                Ok(())
            }
            Effect::Scroll(y, x) => {
                let mut events = Vec::new();
                for (i, v) in [y, x].into_iter().enumerate() {
                    self.scroll[i] += v;
                    if v != 0 {
                        events.push(InputEvent::new(
                            EventType::RELATIVE.0,
                            if i == 0 {
                                Rel::REL_WHEEL_HI_RES.0
                            } else {
                                Rel::REL_HWHEEL_HI_RES.0
                            },
                            v,
                        ));
                    }
                    let detents = self.scroll[i] / 120;
                    self.scroll[i] %= 120;
                    if detents != 0 {
                        events.push(InputEvent::new(
                            EventType::RELATIVE.0,
                            if i == 0 {
                                Rel::REL_WHEEL.0
                            } else {
                                Rel::REL_HWHEEL.0
                            },
                            detents,
                        ));
                    }
                }
                self.mouse.emit(&events)
            }
            Effect::Key(k) => self.tap(key_code(k)),
            Effect::Text(c) => self.text(c),
            Effect::ReleaseAll => {
                self.release_all();
                Ok(())
            }
            Effect::Rumble(..) => Ok(()),
        }
    }
    pub fn release_all(&mut self) {
        for k in self.held.clone() {
            let _ = self.key(k, false);
        }
        for i in 0..2 {
            if self.buttons[i] {
                let _ = self.apply(&Effect::Mouse(i as u8, false));
            }
        }
        self.scroll = [0; 2];
    }
}
impl Drop for Output {
    fn drop(&mut self) {
        self.release_all();
    }
}
/// Read only LED status, never keyboard input events. Unlike wl_keyboard
/// modifiers, these remain current while our non-focusable panel is hidden.
pub(crate) fn caps_lock_state() -> Option<bool> {
    let entries = std::fs::read_dir("/sys/class/leds").ok()?;
    let states: Vec<bool> = entries
        .filter_map(Result::ok)
        .filter(|e| e.file_name().to_string_lossy().ends_with("::capslock"))
        .filter_map(|e| {
            std::fs::read_to_string(e.path().join("brightness"))
                .ok()
                .map(|s| s.trim() != "0")
        })
        .collect();
    (!states.is_empty()).then(|| states.iter().any(|s| *s))
}
pub fn key_code(key: Key) -> u16 {
    match key {
        Key::Backspace => K::KEY_BACKSPACE,
        Key::Enter => K::KEY_ENTER,
        Key::Escape => K::KEY_ESC,
        Key::Tab => K::KEY_TAB,
        Key::Delete => K::KEY_DELETE,
        Key::Home => K::KEY_HOME,
        Key::End => K::KEY_END,
        Key::PageUp => K::KEY_PAGEUP,
        Key::PageDown => K::KEY_PAGEDOWN,
        Key::Super => K::KEY_LEFTMETA,
        Key::Control => K::KEY_LEFTCTRL,
        Key::Alt => K::KEY_LEFTALT,
        Key::AltGr => K::KEY_RIGHTALT,
        Key::PrintScreen => K::KEY_SYSRQ,
        Key::Left => K::KEY_LEFT,
        Key::Up => K::KEY_UP,
        Key::Down => K::KEY_DOWN,
        Key::Right => K::KEY_RIGHT,
        Key::Space => K::KEY_SPACE,
        Key::F(n) => [
            K::KEY_F1,
            K::KEY_F2,
            K::KEY_F3,
            K::KEY_F4,
            K::KEY_F5,
            K::KEY_F6,
            K::KEY_F7,
            K::KEY_F8,
            K::KEY_F9,
            K::KEY_F10,
            K::KEY_F11,
            K::KEY_F12,
        ][usize::from(n.clamp(1, 12) - 1)],
    }
    .0
}
pub fn run(
    state: State,
    receiver: Receiver<Command>,
    ready: Sender<Result<(), String>>,
    show: bool,
) {
    let mut output = match Output::new() {
        Ok(v) => v,
        Err(e) => {
            let _ = ready.send(Err(format!("uinput: {e}. Run ./install.sh on the host.")));
            return;
        }
    };
    let mut engine = Engine::default();
    let mut desktop_activity = crate::activity::DesktopActivity::default();
    engine.show(show);
    let _ = ready.send(Ok(()));
    let mut pad: Option<Gamepad> = None;
    let mut scan = Instant::now() - Duration::from_secs(1);
    let start = Instant::now();
    let mut previous = Instant::now();
    let mut last_error = String::new();
    let mut stop = false;
    while !stop {
        let now = Instant::now();
        let dt = now.duration_since(previous).as_secs_f32();
        previous = now;
        let mut effects = Vec::new();
        let physical = desktop_activity.poll();
        if physical {
            effects.extend(engine.physical_activity());
        }
        for cmd in receiver.try_iter().take(128) {
            match cmd {
                Command::Stop => stop = true,
                Command::Show(v) => effects.extend(engine.show(v)),
                Command::Toggle => effects.extend(engine.show(!engine.view.visible)),
                Command::Focus {
                    editable,
                    protected,
                } => {
                    engine.view.protected = protected;
                    // Without readable ordinary devices we cannot safely tell
                    // physical typing apart. Fall back to manual Back opening.
                    effects.extend(engine.focus(editable && desktop_activity.device_count() > 0));
                }
                Command::Fullscreen(v) => effects.extend(engine.fullscreen(v)),
                Command::TextMap(m) => output.text_map = m,
            }
        }
        if pad.is_none() && scan.elapsed() >= Duration::from_millis(500) {
            pad = Gamepad::find();
            scan = now;
        }
        if let Some(p) = pad.as_mut()
            && let Err(e) = p.poll()
        {
            eprintln!("Controller removed: {e}");
            pad = None;
        }
        let sample = pad.as_ref().map(|p| p.state.clone()).unwrap_or_default();
        effects.extend(engine.step(&sample, start.elapsed().as_millis() as u64, dt));
        if physical {
            // Physical input also wins if a simultaneous gamepad packet arrived.
            effects.clear();
            effects.extend(engine.physical_activity());
        }
        for e in effects {
            let result = match e {
                Effect::Rumble(m, ms) => {
                    if let Some(p) = pad.as_mut() {
                        let _ = p.rumble(m, ms);
                    }
                    Ok(())
                }
                _ => output.apply(&e),
            };
            if let Err(err) = result {
                let text = err.to_string();
                if text != last_error {
                    eprintln!("Input error: {text}");
                    last_error = text.clone();
                }
                engine.view.error = Some(text);
            } else if matches!(e, Effect::Text(_) | Effect::Key(_)) {
                engine.view.error = None;
                last_error.clear();
            }
        }
        if let Ok(mut shared) = state.lock() {
            shared.view = engine.view.clone();
            shared.ticks += 1;
            shared.events = pad.as_ref().map_or(0, |p| p.events);
            shared.device = sample.name;
            shared.desktop_input_devices = desktop_activity.device_count();
        }
        thread::sleep(Duration::from_millis(8).saturating_sub(now.elapsed()));
    }
    output.release_all();
    if let Ok(mut s) = state.lock() {
        s.stopped = true;
    }
}
