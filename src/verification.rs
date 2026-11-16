//! Real compositor test with the destination in a separate process/Wayland
//! connection, as in normal use. IPC reports only test booleans/counts, not text.
use crate::{
    drawing::{self, Animation},
    hardware::{Command, Output, Shared},
    integration, layout,
    model::{Effect, View},
    ui,
};
use gtk::{gdk, glib, prelude::*};
use std::{
    cell::RefCell,
    io::{BufRead, BufReader, Write},
    process::{Child, ChildStdin, Stdio},
    rc::Rc,
    sync::{Arc, Mutex, mpsc},
    thread,
    time::{Duration, Instant},
};
const SAMPLE: &str = "ControllerApp áÇ!";

pub fn target() -> Result<(), Box<dyn std::error::Error>> {
    gtk::init()?;
    let display = gdk::Display::default().ok_or("Target display unavailable")?;
    let entry = gtk::Entry::builder()
        .placeholder_text(SAMPLE)
        .im_module("gtk-im-context-simple")
        .build();
    let content = gtk::Box::new(gtk::Orientation::Vertical, 12);
    content.append(&gtk::Label::new(Some(
        "ControllerApp self-test — please do not use the keyboard or mouse.",
    )));
    content.append(&entry);
    let filler = gtk::Box::new(gtk::Orientation::Vertical, 0);
    filler.set_vexpand(true);
    filler.set_focusable(true);
    content.append(&filler);
    let clicks = Rc::new(RefCell::new(0));
    let count = clicks.clone();
    let click = gtk::GestureClick::new();
    click.set_propagation_phase(gtk::PropagationPhase::Capture);
    click.connect_pressed(move |_, _, _, _| *count.borrow_mut() += 1);
    content.add_controller(click);
    let window = gtk::Window::builder()
        .title("ControllerApp — self-test")
        .child(&content)
        .build();
    window.fullscreen();
    window.present();
    entry.grab_focus();
    let pointer = display
        .default_seat()
        .and_then(|s| s.pointer())
        .ok_or("Pointer unavailable")?;
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        for line in std::io::stdin().lock().lines() {
            let Ok(line) = line else {
                break;
            };
            if tx.send(line).is_err() {
                return;
            }
        }
        let _ = tx.send("QUIT".into());
    });
    let event_loop = glib::MainLoop::new(None, false);
    let loop_tick = event_loop.clone();
    let window_tick = window.clone();
    let mut previous = String::new();
    let begin = Instant::now();
    glib::timeout_add_local(Duration::from_millis(20), move || {
        for cmd in rx.try_iter() {
            match cmd.as_str() {
                "BLUR" => {
                    filler.grab_focus();
                }
                "FOCUS" => {
                    entry.grab_focus();
                }
                "CLEAR" => entry.set_text(""),
                "QUIT" => {
                    loop_tick.quit();
                    return glib::ControlFlow::Break;
                }
                _ => {}
            }
        }
        if begin.elapsed() > Duration::from_secs(30) {
            loop_tick.quit();
            return glib::ControlFlow::Break;
        }
        let (surface, x, y) = pointer.surface_at_position();
        let text = entry.text();
        let different = text
            .chars()
            .zip(SAMPLE.chars())
            .enumerate()
            .fold(0_u32, |mask, (i, (a, b))| {
                mask | if a != b { 1 << i } else { 0 }
            });
        let state = format!(
            "{} {} {} {} {} {} {} {} {} {}",
            u8::from(window_tick.is_active()),
            u8::from(text.as_str() == SAMPLE),
            text.chars().count(),
            *clicks.borrow(),
            window_tick.width(),
            window_tick.height(),
            x,
            y,
            u8::from(surface == window_tick.surface()),
            different
        );
        if state != previous {
            println!("{state}");
            let _ = std::io::stdout().flush();
            previous = state;
        }
        glib::ControlFlow::Continue
    });
    event_loop.run();
    window.close();
    Ok(())
}
#[derive(Clone, Default)]
struct Status {
    active: bool,
    exact: bool,
    len: usize,
    clicks: usize,
    w: f64,
    h: f64,
    x: f64,
    y: f64,
    owns: bool,
    different: u32,
    dead: bool,
}
struct Target {
    child: Child,
    input: ChildStdin,
    state: Arc<Mutex<Status>>,
}
impl Target {
    fn start() -> Result<Self, Box<dyn std::error::Error>> {
        let mut child = std::process::Command::new(std::env::current_exe()?)
            .arg("--test-target")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()?;
        let input = child.stdin.take().ok_or("Target stdin unavailable")?;
        let output = child.stdout.take().ok_or("Target stdout unavailable")?;
        let state = Arc::new(Mutex::new(Status::default()));
        let reader = state.clone();
        thread::spawn(move || {
            for line in BufReader::new(output).lines() {
                let Ok(line) = line else {
                    break;
                };
                let values: Vec<f64> = line
                    .split_whitespace()
                    .filter_map(|v| v.parse().ok())
                    .collect();
                if values.len() != 10 {
                    continue;
                }
                *reader.lock().unwrap() = Status {
                    active: values[0] == 1.,
                    exact: values[1] == 1.,
                    len: values[2] as usize,
                    clicks: values[3] as usize,
                    w: values[4],
                    h: values[5],
                    x: values[6],
                    y: values[7],
                    owns: values[8] == 1.,
                    different: values[9] as u32,
                    dead: false,
                };
            }
            reader.lock().unwrap().dead = true;
        });
        Ok(Self {
            child,
            input,
            state,
        })
    }
    fn command(&mut self, s: &str) -> std::io::Result<()> {
        writeln!(self.input, "{s}")?;
        self.input.flush()
    }
}
impl Drop for Target {
    fn drop(&mut self) {
        let _ = self.command("QUIT");
        for _ in 0..10 {
            if self.child.try_wait().is_ok_and(|s| s.is_some()) {
                return;
            }
            thread::sleep(Duration::from_millis(20));
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    let display = ui::initialize()?;
    let (overlay, area) = ui::panel(&display);
    let view = View {
        visible: true,
        ..View::default()
    };
    area.set_draw_func(move |_, ctx, w, h| {
        drawing::draw(
            ctx,
            f64::from(w),
            f64::from(h),
            &view,
            &layout::main_layout(),
            &mut Animation::settled_visible(),
        )
    });
    let shared = Arc::new(Mutex::new(Shared::default()));
    let (tx, rx) = mpsc::channel();
    integration::accessibility(tx, shared.clone());
    let target = Rc::new(RefCell::new(Target::start()?));
    let mut output = Output::new()?;
    let event_loop = glib::MainLoop::new(None, false);
    let loop_tick = event_loop.clone();
    let result = Rc::new(RefCell::new(None));
    let result_tick = result.clone();
    let target_tick = target.clone();
    let panel = overlay.clone();
    let mut stage = 0;
    let mut focus_seen = false;
    let mut requested = false;
    let mut index = 0;
    let chars: Vec<_> = SAMPLE.chars().collect();
    let mut origin = None;
    let mut restore_started: Option<Instant> = None;
    let begin = Instant::now();
    glib::timeout_add_local(Duration::from_millis(90), move || {
        let fail = |message: String| {
            *result_tick.borrow_mut() = Some(Err(message));
            loop_tick.quit();
            glib::ControlFlow::Break
        };
        let state = target_tick.borrow().state.lock().unwrap().clone();
        if state.dead {
            return fail("Test window closed".into());
        }
        // Restoration is cleanup, not the input assertion. Never keep moving
        // the pointer against new user activity after a verified click.
        if stage == 6
            && state.clicks > 0
            && (!state.active
                || !state.owns
                || restore_started.is_some_and(|t| t.elapsed() > Duration::from_secs(2)))
        {
            println!(
                "PASS: uinput mouse; click passes through overlay; cursor restoration interrupted (timeout/focus)"
            );
            *result_tick.borrow_mut() = Some(Ok(()));
            loop_tick.quit();
            return glib::ControlFlow::Break;
        }
        if begin.elapsed() > Duration::from_secs(15) {
            return fail(format!(
                "Timed out at stage {stage}; AT-SPI focus={focus_seen}"
            ));
        }
        if !state.active {
            return glib::ControlFlow::Continue;
        }
        for event in rx.try_iter() {
            if matches!(event, Command::Focus { editable: true, .. }) {
                focus_seen = true;
            }
        }
        match stage {
            0 => {
                if !shared.lock().unwrap().focus_monitor {
                    return glib::ControlFlow::Continue;
                }
                if !requested {
                    let _ = target_tick.borrow_mut().command("BLUR");
                    requested = true;
                    return glib::ControlFlow::Continue;
                }
                let _ = target_tick.borrow_mut().command("FOCUS");
                if !focus_seen {
                    return glib::ControlFlow::Continue;
                }
                output.text_map = ui::text_map(&display, None);
                if output.text_map.is_empty() {
                    return glib::ControlFlow::Continue;
                }
                panel.set_visible(true);
                stage = 1;
            }
            1 => {
                if !panel.surface().is_some_and(|s| s.is_mapped()) {
                    return glib::ControlFlow::Continue;
                }
                panel.set_visible(false);
                stage = 2;
            }
            2 => {
                panel.set_visible(true);
                let _ = target_tick.borrow_mut().command("CLEAR");
                stage = 3;
            }
            3 => {
                if !panel.surface().is_some_and(|s| s.is_mapped()) || (index == 0 && state.len != 0)
                {
                    return glib::ControlFlow::Continue;
                }
                if index < chars.len() {
                    if let Err(e) = output.apply(&Effect::Text(chars[index])) {
                        return fail(e.to_string());
                    }
                    index += 1;
                } else {
                    stage = 4;
                }
            }
            4 => {
                if !state.exact {
                    return fail(format!(
                        "Typing mismatch in target process; length={}, difference mask={}; content not logged",
                        state.len, state.different
                    ));
                }
                println!(
                    "PASS: separate target process; AT-SPI focus; map/hide/remap without stealing focus; uppercase/accented uinput text (local GTK compose)"
                );
                stage = 5;
            }
            5 | 6 => {
                if !state.owns {
                    return fail("Pointer outside test window or intercepted by overlay".into());
                }
                let initial = *origin.get_or_insert((state.x, state.y));
                let destination = if stage == 5 {
                    (state.w * 0.5, state.h * 0.8)
                } else {
                    initial
                };
                let (dx, dy) = (destination.0 - state.x, destination.1 - state.y);
                if dx.abs() > 8. || dy.abs() > 8. {
                    if let Err(e) = output.apply(&Effect::Move(
                        (dx * 0.3).round() as i32,
                        (dy * 0.3).round() as i32,
                    )) {
                        return fail(e.to_string());
                    }
                    return glib::ControlFlow::Continue;
                }
                if stage == 5 {
                    if let Err(e) = output
                        .apply(&Effect::Mouse(0, true))
                        .and_then(|_| output.apply(&Effect::Mouse(0, false)))
                    {
                        return fail(e.to_string());
                    }
                    stage = 6;
                    restore_started = Some(Instant::now());
                    return glib::ControlFlow::Continue;
                }
                if state.clicks == 0 {
                    return fail("Click did not pass through the keyboard".into());
                }
                println!("PASS: uinput mouse; click passes through overlay; cursor restored");
                *result_tick.borrow_mut() = Some(Ok(()));
                loop_tick.quit();
                return glib::ControlFlow::Break;
            }
            _ => unreachable!(),
        }
        glib::ControlFlow::Continue
    });
    event_loop.run();
    overlay.close();
    drop(target);
    result
        .borrow_mut()
        .take()
        .unwrap_or_else(|| Err("Test interrupted".into()))
        .map_err(Into::into)
}
