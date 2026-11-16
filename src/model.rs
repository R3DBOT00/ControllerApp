//! Deterministic input state machine. No window, device or rendering dependencies.
use crate::layout::{self, Action, Key, Layout};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(usize)]
pub enum Button {
    A,
    B,
    X,
    Y,
    Lb,
    Rb,
    Back,
    Start,
    R3,
    Up,
    Down,
    Left,
    Right,
}
pub const BUTTONS: usize = 13;
/// Scale only pointer motion; scrolling and keyboard navigation are unchanged.
pub const POINTER_SPEED_SCALE: f32 = 0.6;
#[derive(Clone, Debug, Default)]
pub struct Pad {
    pub connected: bool,
    pub name: String,
    pub axes: [f32; 4],
    pub buttons: [bool; BUTTONS],
}
#[derive(Clone, Debug, PartialEq)]
pub enum Effect {
    Move(i32, i32),
    Scroll(i32, i32),
    Mouse(u8, bool),
    Key(Key),
    Text(char),
    Rumble(u16, u16),
    ReleaseAll,
}
#[derive(Clone, Debug)]
pub struct View {
    pub visible: bool,
    pub enabled: bool,
    pub connected: bool,
    pub game: bool,
    pub controller_active: bool,
    pub physical_input_epoch: u64,
    pub shift: bool,
    pub caps_lock: Option<bool>,
    pub function: bool,
    pub row: usize,
    pub col: usize,
    pub preview: String,
    pub protected: bool,
    pub pulse: u64,
    pub error: Option<String>,
}
impl Default for View {
    fn default() -> Self {
        Self {
            visible: false,
            enabled: true,
            connected: false,
            game: false,
            controller_active: false,
            physical_input_epoch: 0,
            shift: false,
            caps_lock: None,
            function: false,
            row: 0,
            col: 0,
            preview: String::new(),
            protected: false,
            pulse: 0,
            error: None,
        }
    }
}
pub struct Engine {
    pub view: View,
    pub layouts: [Layout; 2],
    previous: Pad,
    armed: [bool; BUTTONS],
    chord: bool,
    mouse: [bool; 2],
    accum: [f32; 4],
    navigation_ms: u64,
    accent: Option<char>,
    auto_open: bool,
    editable: bool,
    activity_axes: [f32; 4],
    physical_override: bool,
    click_focus_due: Option<u64>,
}
impl Default for Engine {
    fn default() -> Self {
        Self {
            view: View::default(),
            layouts: [layout::main_layout(), layout::function_layout()],
            previous: Pad::default(),
            armed: [false; BUTTONS],
            chord: false,
            mouse: [false; 2],
            accum: [0.; 4],
            navigation_ms: 0,
            accent: None,
            auto_open: false,
            editable: false,
            activity_axes: [0.; 4],
            physical_override: false,
            click_focus_due: None,
        }
    }
}
impl Engine {
    /// Ordinary keyboard/mouse activity wins without consuming or replaying it.
    /// Forget held gamepad actions so their release cannot reopen the overlay.
    pub fn physical_activity(&mut self) -> Vec<Effect> {
        self.view.controller_active = false;
        self.physical_override = true;
        self.activity_axes = self.previous.axes;
        self.armed = self.previous.buttons.map(|down| !down);
        self.chord = false;
        self.mouse = [false; 2];
        self.accum = [0.; 4];
        if self.view.visible {
            self.view.physical_input_epoch = self.view.physical_input_epoch.wrapping_add(1);
        }
        let mut effects = self.show(false);
        if effects.is_empty() {
            effects.push(Effect::ReleaseAll);
        }
        effects
    }
    pub fn show(&mut self, show: bool) -> Vec<Effect> {
        self.auto_open = false;
        self.click_focus_due = None;
        if self.view.visible == show {
            return vec![];
        }
        self.view.visible = show;
        self.view.preview.clear();
        self.accent = None;
        self.mouse = [false; 2];
        self.accum = [0.; 4];
        vec![Effect::ReleaseAll]
    }
    pub fn focus(&mut self, editable: bool) -> Vec<Effect> {
        if editable == self.editable {
            return vec![];
        }
        self.editable = editable;
        if !editable && self.auto_open {
            self.show(false)
        } else {
            self.open_for_focus()
        }
    }
    fn open_for_focus(&mut self) -> Vec<Effect> {
        if self.editable
            && self.view.connected
            && self.view.controller_active
            && self.view.enabled
            && !self.view.game
            && !self.view.visible
            && !self.mouse.iter().any(|down| *down)
            && self.click_focus_due.is_none()
            && !self.previous.buttons[Button::Back as usize]
            && !self.previous.buttons[Button::Start as usize]
        {
            let effects = self.show(true);
            self.auto_open = true;
            effects
        } else {
            vec![]
        }
    }
    pub fn fullscreen(&mut self, game: bool) -> Vec<Effect> {
        if self.view.game == game {
            return vec![];
        }
        self.view.game = game;
        self.mouse = [false; 2];
        self.accum = [0.; 4];
        if game {
            self.show(false);
        }
        vec![Effect::ReleaseAll]
    }
    pub fn step(&mut self, pad: &Pad, now_ms: u64, dt: f32) -> Vec<Effect> {
        let mut out = Vec::new();
        self.view.connected = pad.connected;
        if !pad.connected {
            if self.previous.connected {
                out.extend(self.show(false));
                out.push(Effect::ReleaseAll);
            }
            self.previous = pad.clone();
            self.armed = [false; BUTTONS];
            self.chord = false;
            self.view.controller_active = false;
            return out;
        }
        if !self.previous.connected {
            self.previous = pad.clone();
            self.armed = pad.buttons.map(|v| !v);
            self.activity_axes = pad.axes;
            return out; // connect snapshots never click, type, or vibrate
        }
        let mut pressed = [false; BUTTONS];
        let mut released = [false; BUTTONS];
        for i in 0..BUTTONS {
            pressed[i] = self.armed[i] && pad.buttons[i] && !self.previous.buttons[i];
            released[i] = self.armed[i] && !pad.buttons[i] && self.previous.buttons[i];
            if !pad.buttons[i] {
                self.armed[i] = true;
            }
        }
        // A connected/idle controller is not permission to interrupt typing.
        // Small analog noise cannot re-enable auto-opening after physical input.
        if pressed.iter().any(|pressed| *pressed)
            || pad
                .axes
                .iter()
                .zip(self.activity_axes)
                .any(|(value, baseline)| value.abs() >= 4000. && (value - baseline).abs() >= 4000.)
        {
            self.view.controller_active = true;
            self.physical_override = false;
        }
        // Returning to center rearms takeover even if the next deliberate
        // movement goes to the same position held during physical typing.
        for (baseline, value) in self.activity_axes.iter_mut().zip(pad.axes) {
            if value.abs() < 1500. {
                *baseline = 0.;
            }
        }
        let back = Button::Back as usize;
        let start = Button::Start as usize;
        if pressed[back] || pressed[start] {
            self.click_focus_due = None; // explicit shortcuts take precedence
        }
        if self.armed[back]
            && self.armed[start]
            && pad.buttons[back]
            && pad.buttons[start]
            && (pressed[back] || pressed[start])
            && !self.chord
        {
            self.chord = true;
            self.view.enabled = !self.view.enabled;
            out.extend(self.show(false));
            out.push(Effect::ReleaseAll);
            out.push(Effect::Rumble(
                if self.view.enabled { 30000 } else { 50000 },
                if self.view.enabled { 80 } else { 220 },
            ));
        }
        if !self.chord && self.view.enabled && !self.view.game {
            if released[back] {
                out.extend(self.show(!self.view.visible));
                out.push(Effect::Rumble(25000, 40));
            }
            if released[start] && self.view.visible {
                out.push(Effect::Key(Key::Enter));
                out.extend(self.show(false));
                out.push(Effect::Rumble(40000, 60));
            }
        }
        if !pad.buttons[back] && !pad.buttons[start] {
            self.chord = false;
        }
        // Release tracked mouse buttons regardless of a concurrent overlay/mode change.
        for (i, b) in [Button::A, Button::B].iter().enumerate() {
            if !pad.buttons[*b as usize] && self.mouse[i] {
                self.mouse[i] = false;
                out.push(Effect::Mouse(i as u8, false));
                if i == 0 {
                    // Complete the real click first. Firefox need not emit a
                    // focus event when clicking an already-focused text field.
                    // Allow asynchronous blur/focus events to settle before
                    // rechecking the cached state, including clicks outside it.
                    self.click_focus_due = Some(now_ms.saturating_add(120));
                }
            }
        }
        if self.view.enabled && !self.view.game && !self.physical_override {
            for b in [
                Button::A,
                Button::B,
                Button::X,
                Button::Y,
                Button::Lb,
                Button::Rb,
                Button::R3,
            ] {
                if !pressed[b as usize] {
                    continue;
                }
                if self.view.visible {
                    match b {
                        Button::A => {
                            let key = self.layouts[self.view.function as usize][self.view.row]
                                [self.view.col]
                                .action;
                            out.extend(self.activate(key));
                        }
                        Button::B => {
                            out.extend(self.show(false));
                            out.push(Effect::Rumble(30000, 45));
                        }
                        Button::X => {
                            self.backspace(&mut out);
                            self.view.pulse += 1;
                            out.push(Effect::Rumble(35000, 50));
                        }
                        Button::Y => {
                            self.flush(&mut out);
                            self.view.preview.push(' ');
                            out.push(Effect::Key(Key::Space));
                            out.push(Effect::Rumble(35000, 50));
                            self.view.pulse += 1;
                        }
                        _ => {
                            self.view.shift = !self.view.shift;
                            out.push(Effect::Rumble(25000, 40));
                        }
                    }
                } else if matches!(b, Button::A | Button::B) {
                    self.click_focus_due = None;
                    let index = usize::from(b == Button::B);
                    self.mouse[index] = true;
                    out.push(Effect::Mouse(index as u8, true));
                    out.push(Effect::Rumble(20000, 35));
                }
            }
            if self.view.visible {
                if now_ms.saturating_sub(self.navigation_ms) >= 135 {
                    let dy = i32::from(pad.buttons[Button::Down as usize] || pad.axes[1] > 18000.)
                        - i32::from(pad.buttons[Button::Up as usize] || pad.axes[1] < -18000.);
                    let dx = i32::from(pad.buttons[Button::Right as usize] || pad.axes[0] > 18000.)
                        - i32::from(pad.buttons[Button::Left as usize] || pad.axes[0] < -18000.);
                    if dx != 0 || dy != 0 {
                        self.navigate(dx, dy);
                        self.navigation_ms = now_ms;
                        out.push(Effect::Rumble(18000, 30));
                    }
                }
            } else {
                let [x, y, _, _] = pad.axes;
                let mag = x.hypot(y);
                if mag >= 1500. {
                    let speed = (2.
                        + 28. * ((mag - 1500.) / (32767. - 1500.)).clamp(0., 1.).powf(1.4))
                        * POINTER_SPEED_SCALE;
                    self.accum[0] += x / mag * speed * (dt / 0.016).clamp(0., 4.);
                    self.accum[1] += y / mag * speed * (dt / 0.016).clamp(0., 4.);
                    let dx = take_integer(&mut self.accum[0]);
                    let dy = take_integer(&mut self.accum[1]);
                    if dx != 0 || dy != 0 {
                        out.push(Effect::Move(dx, dy));
                    }
                } else {
                    self.accum[0] = 0.;
                    self.accum[1] = 0.;
                }
            }
            // Original wheel values are 18/120 of a detent, not a full detent.
            for (i, value) in [-pad.axes[3], pad.axes[2]].iter().enumerate() {
                if value.abs() < 1200. {
                    self.accum[2 + i] = 0.;
                } else {
                    self.accum[2 + i] += value.signum()
                        * ((value.abs() - 1200.) / (32767. - 1200.))
                            .clamp(0., 1.)
                            .powf(1.3)
                        * 170.
                        * 0.0095
                        * 18.
                        * (dt / 0.016).clamp(0., 4.);
                }
            }
            let y = take_integer(&mut self.accum[2]);
            let x = take_integer(&mut self.accum[3]);
            if x != 0 || y != 0 {
                out.push(Effect::Scroll(y, x));
            }
        }
        if self.click_focus_due.is_some_and(|due| now_ms >= due) {
            self.click_focus_due = None;
            out.extend(self.open_for_focus());
        }
        self.previous = pad.clone();
        self.trim_preview();
        out
    }
    pub fn navigate(&mut self, dx: i32, dy: i32) {
        let rows = &self.layouts[self.view.function as usize];
        self.view.row = (self.view.row as i32 + dy).rem_euclid(rows.len() as i32) as usize;
        let cols = rows[self.view.row].len();
        self.view.col =
            ((self.view.col.min(cols - 1)) as i32 + dx).rem_euclid(cols as i32) as usize;
    }
    pub fn activate(&mut self, action: Action) -> Vec<Effect> {
        let mut out = vec![Effect::Rumble(48000, 75)];
        self.view.pulse += 1;
        match action {
            Action::Layer => {
                self.view.function = !self.view.function;
                self.view.row = 0;
                self.view.col = 0;
            }
            Action::Key(Key::Backspace) => self.backspace(&mut out),
            Action::Key(k) => {
                self.flush(&mut out);
                if k == Key::Space {
                    self.view.preview.push(' ');
                }
                out.push(Effect::Key(k));
            }
            Action::Character(a, b) => {
                let c = if self.view.shift { b } else { a };
                self.view.shift = false;
                if ['´', '~', '^', '`'].contains(&c) {
                    self.flush(&mut out);
                    self.accent = Some(c);
                    self.view.preview.push(c);
                } else if let Some(accent) = self.accent.take() {
                    if let Some(composed) = layout::compose(accent, c) {
                        self.view.preview.pop();
                        self.view.preview.push(composed);
                        out.push(Effect::Text(composed));
                    } else {
                        self.view.preview.push(c);
                        out.extend([Effect::Text(accent), Effect::Text(c)]);
                    }
                } else {
                    self.view.preview.push(c);
                    out.push(Effect::Text(c));
                }
            }
        }
        // Keep memory bounded; only the recent preview is displayed, never persisted.
        self.trim_preview();
        out
    }
    fn trim_preview(&mut self) {
        if self.view.preview.chars().count() > 256 {
            self.view.preview = self
                .view
                .preview
                .chars()
                .rev()
                .take(256)
                .collect::<String>()
                .chars()
                .rev()
                .collect();
        }
    }
    fn backspace(&mut self, out: &mut Vec<Effect>) {
        self.view.preview.pop();
        if self.accent.take().is_none() {
            out.push(Effect::Key(Key::Backspace));
        }
    }
    fn flush(&mut self, out: &mut Vec<Effect>) {
        if let Some(c) = self.accent.take() {
            out.push(Effect::Text(c));
        }
    }
}
fn take_integer(value: &mut f32) -> i32 {
    let n = *value as i32;
    *value -= n as f32;
    n
}
