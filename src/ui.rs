use crate::{
    drawing::{self, Animation},
    hardware::{Command, State, Stroke, TextMap},
    layout,
    model::View,
    theme,
};
use gtk::{gdk, glib, prelude::*};
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};
use std::{
    cell::RefCell,
    rc::Rc,
    sync::mpsc::Sender,
    time::{Duration, Instant},
};

pub(crate) fn initialize() -> Result<gdk::Display, Box<dyn std::error::Error>> {
    gtk::init()?;
    if !gtk4_layer_shell::is_supported() {
        return Err("This compositor does not support layer-shell; use KDE Plasma Wayland or a compatible compositor".into());
    }
    let display = gdk::Display::default().ok_or("Wayland display unavailable")?;
    let css = gtk::CssProvider::new();
    css.load_from_string(
        "window.controller-overlay { background: transparent; box-shadow: none; }",
    );
    gtk::style_context_add_provider_for_display(
        &display,
        &css,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
    Ok(display)
}
pub(crate) fn panel(display: &gdk::Display) -> (gtk::Window, gtk::DrawingArea) {
    let monitor = display
        .monitors()
        .item(0)
        .and_downcast::<gdk::Monitor>()
        .expect("Wayland monitor");
    let window = gtk::Window::builder()
        .title("ControllerApp")
        .decorated(false)
        .resizable(false)
        .focusable(false)
        .build();
    window.add_css_class("controller-overlay");
    window.init_layer_shell();
    window.set_namespace(Some("controllerapp"));
    window.set_layer(Layer::Overlay);
    window.set_keyboard_mode(KeyboardMode::None);
    // Respect panels' reserved areas without reserving space for this overlay.
    window.set_exclusive_zone(0);
    window.set_monitor(Some(&monitor));
    window.set_anchor(Edge::Bottom, true);
    window.set_margin(Edge::Bottom, 12);
    let size = monitor.geometry();
    let (width, height) = theme::overlay_size(size.width(), size.height());
    window.set_default_size(width, height);
    let drawing = gtk::DrawingArea::builder()
        .content_width(width)
        .content_height(height)
        .can_target(false)
        .focusable(false)
        .build();
    window.set_child(Some(&drawing));
    window.connect_map(|window| {
        if let Some(surface) = window.surface() {
            surface.set_input_region(Some(&gtk::cairo::Region::create()));
        }
    });
    // Follow resolution, scale and monitor hotplug changes without taking focus.
    // Weak references ensure this timer does not keep closed test windows alive.
    let weak_window = window.downgrade();
    let weak_drawing = drawing.downgrade();
    let monitors = display.monitors();
    let mut last_monitor = monitor;
    let mut last_size = (width, height);
    glib::timeout_add_local(Duration::from_millis(500), move || {
        let (Some(window), Some(drawing)) = (weak_window.upgrade(), weak_drawing.upgrade()) else {
            return glib::ControlFlow::Break;
        };
        if let Some(monitor) = monitors.item(0).and_downcast::<gdk::Monitor>() {
            if monitor != last_monitor {
                window.set_monitor(Some(&monitor));
                last_monitor = monitor.clone();
            }
            let geometry = monitor.geometry();
            let size = theme::overlay_size(geometry.width(), geometry.height());
            if size != last_size {
                drawing.set_content_width(size.0);
                drawing.set_content_height(size.1);
                window.set_default_size(size.0, size.1);
                last_size = size;
            }
        }
        glib::ControlFlow::Continue
    });
    (window, drawing)
}
fn stroke(display: &gdk::Display, key: gdk::Key, group: i32) -> Option<Stroke> {
    let mapping = display
        .map_keyval(key)?
        .into_iter()
        .filter(|k| {
            k.group() == group && (0..4).contains(&k.level()) && (8..=256).contains(&k.keycode())
        })
        .min_by_key(|k| k.level())?;
    Some(Stroke {
        code: (mapping.keycode() - 8) as u16,
        shift: mapping.level() & 1 != 0,
        altgr: mapping.level() & 2 != 0,
        alphabetic: false,
    })
}
fn symbol(c: char) -> gdk::Key {
    // GDK's safe unicode conversion returns a validated numeric keysym.
    unsafe { glib::translate::from_glib(gdk::unicode_to_keyval(c as u32)) }
}
fn dead(c: char) -> Option<gdk::Key> {
    match c {
        '´' => Some(gdk::Key::dead_acute),
        '~' => Some(gdk::Key::dead_tilde),
        '^' => Some(gdk::Key::dead_circumflex),
        '`' => Some(gdk::Key::dead_grave),
        '¨' => Some(gdk::Key::dead_diaeresis),
        _ => None,
    }
}
pub fn text_map(display: &gdk::Display, group_override: Option<i32>) -> TextMap {
    let keyboard = display.default_seat().and_then(|s| s.keyboard());
    let group = group_override.unwrap_or_else(|| {
        keyboard
            .as_ref()
            .map_or(0, |k| k.active_layout_index().max(0))
    });
    let mut characters = std::collections::BTreeSet::new();
    for row in layout::main_layout() {
        for k in row {
            if let layout::Action::Character(a, b) = k.action {
                characters.extend([a, b]);
            }
        }
    }
    characters.extend(" áéíóúçãõñâêîôûàèìòùÁÉÍÓÚÇÃÕÑÂÊÎÔÛÀÈÌÒÙ".chars());
    let direct = |c: char| -> Option<Stroke> {
        let mut s = stroke(display, symbol(c), group)?;
        s.alphabetic = c.is_alphabetic();
        Some(s)
    };
    let mut map = TextMap::new();
    for c in characters {
        let sequence = if let Some(s) = direct(c) {
            Some(vec![s])
        } else if let Some((accent, base)) = layout::decompose(c) {
            dead(accent)
                .and_then(|k| stroke(display, k, group))
                .zip(direct(base))
                .map(|(a, b)| vec![a, b])
        } else {
            dead(c)
                .and_then(|k| stroke(display, k, group))
                .zip(direct(' '))
                .map(|(a, b)| vec![a, b])
        };
        if let Some(s) = sequence {
            map.insert(c, s);
        }
    }
    map
}
pub fn run(sender: Sender<Command>, shared: State) -> Result<(), Box<dyn std::error::Error>> {
    let display = initialize()?;
    let (window, area) = panel(&display);
    let event_loop = glib::MainLoop::new(None, false);
    let data = Rc::new(RefCell::new((View::default(), Animation::default())));
    let render = data.clone();
    area.set_draw_func(move |_, ctx, w, h| {
        let mut state = render.borrow_mut();
        let view = state.0.clone();
        let layout = if view.function {
            layout::function_layout()
        } else {
            layout::main_layout()
        };
        drawing::draw(
            ctx,
            f64::from(w),
            f64::from(h),
            &view,
            &layout,
            &mut state.1,
        );
    });
    let loop_close = event_loop.clone();
    window.connect_close_request(move |_| {
        loop_close.quit();
        glib::Propagation::Proceed
    });
    let mut previous = Instant::now();
    let mut keymap_time = Instant::now() - Duration::from_secs(1);
    let mut caps_lock = None;
    let loop_tick = event_loop.clone();
    let window_tick = window.clone();
    let source = glib::timeout_add_local(Duration::from_millis(16), move || {
        let now = Instant::now();
        let dt = now.duration_since(previous).as_secs_f64();
        previous = now;
        let (mut view, stopped) = {
            let s = shared.lock().unwrap();
            (s.view.clone(), s.stopped)
        };
        if stopped {
            loop_tick.quit();
            return glib::ControlFlow::Continue;
        }
        if keymap_time.elapsed() >= Duration::from_millis(500) {
            let group = shared.lock().unwrap().layout_group;
            let _ = sender.send(Command::TextMap(text_map(&display, group)));
            caps_lock = crate::hardware::caps_lock_state();
            keymap_time = now;
        }
        view.caps_lock = caps_lock;
        let mut state = data.borrow_mut();
        if state.0.physical_input_epoch != view.physical_input_epoch {
            // Give physical typing priority immediately, not after a fade-out.
            state.1 = Animation::default();
        }
        state.0 = view;
        let view = state.0.clone();
        state.1.advance(&view, dt);
        let visible = state.1.opacity > 0. || view.visible;
        if visible != window_tick.is_visible() {
            window_tick.set_visible(visible);
        }
        if visible {
            area.queue_draw();
        }
        if let Ok(mut s) = shared.lock() {
            s.mapped = window_tick.surface().is_some_and(|w| w.is_mapped());
        }
        glib::ControlFlow::Continue
    });
    event_loop.run();
    source.remove();
    window.close();
    Ok(())
}
