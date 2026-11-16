use controllerapp_linux::{
    hardware::{key_code, normalize},
    layout::{self, Action, Key},
    model::{Button, Effect, Engine, Pad},
};

fn connect(e: &mut Engine) -> Pad {
    let p = Pad {
        connected: true,
        ..Pad::default()
    };
    assert!(e.step(&p, 0, 0.016).is_empty());
    p
}
fn toggle(e: &mut Engine, p: &mut Pad, t: u64) -> Vec<Effect> {
    p.buttons[Button::Back as usize] = true;
    e.step(p, t, 0.016);
    p.buttons[Button::Back as usize] = false;
    e.step(p, t + 16, 0.016)
}

#[test]
fn connection_is_silent_even_with_a_button_held() {
    let mut e = Engine::default();
    let mut p = Pad {
        connected: true,
        ..Pad::default()
    };
    p.buttons[Button::Back as usize] = true;
    assert!(e.step(&p, 0, 0.016).is_empty());
    p.buttons[Button::Back as usize] = false;
    assert!(e.step(&p, 16, 0.016).is_empty());
    assert!(!e.view.visible);
}
#[test]
fn hidden_keyboard_does_not_stop_mouse() {
    let mut e = Engine::default();
    let mut p = connect(&mut e);
    p.axes[0] = 32767.;
    for t in [16, 32, 48, 64] {
        assert!(e.step(&p, t, 0.016).contains(&Effect::Move(18, 0)));
    }
    assert!(!e.view.visible);
}
#[test]
fn chord_consumes_both_single_actions_in_both_orders() {
    for first in [Button::Back, Button::Start] {
        let mut e = Engine::default();
        let mut p = connect(&mut e);
        p.buttons[first as usize] = true;
        e.step(&p, 16, 0.016);
        p.buttons[Button::Back as usize] = true;
        p.buttons[Button::Start as usize] = true;
        let result = e.step(&p, 32, 0.016);
        assert!(!e.view.enabled);
        assert!(!e.view.visible);
        assert!(!result.contains(&Effect::Key(Key::Enter)));
        p.buttons = [false; 13];
        e.step(&p, 48, 0.016);
        assert!(!e.view.visible);
    }
}
#[test]
fn open_close_open_is_repeatable() {
    let mut e = Engine::default();
    let mut p = connect(&mut e);
    for i in 0..6 {
        toggle(&mut e, &mut p, i * 32);
        assert_eq!(e.view.visible, i % 2 == 0);
    }
}
#[test]
fn mouse_released_when_overlay_opens() {
    let mut e = Engine::default();
    let mut p = connect(&mut e);
    p.buttons[Button::A as usize] = true;
    assert!(e.step(&p, 16, 0.016).contains(&Effect::Mouse(0, true)));
    assert!(toggle(&mut e, &mut p, 32).contains(&Effect::ReleaseAll));
    p.buttons[Button::A as usize] = false;
    e.step(&p, 64, 0.016);
}
#[test]
fn unplug_releases_buttons_and_closes_keyboard() {
    let mut e = Engine::default();
    let mut p = connect(&mut e);
    p.buttons[Button::A as usize] = true;
    e.step(&p, 16, 0.016);
    let result = e.step(&Pad::default(), 32, 0.016);
    assert!(result.contains(&Effect::ReleaseAll));
    assert!(!e.view.connected);
    assert!(!e.view.visible);
}
#[test]
fn game_mode_releases_and_suppresses_until_return() {
    let mut e = Engine::default();
    let mut p = connect(&mut e);
    assert!(e.fullscreen(true).contains(&Effect::ReleaseAll));
    p.axes[0] = 32767.;
    p.buttons[Button::A as usize] = true;
    assert!(e.step(&p, 16, 0.016).is_empty());
    e.fullscreen(false);
    assert!(e.step(&p, 32, 0.016).contains(&Effect::Move(18, 0)));
}
#[test]
fn analog_radial_deadzone_and_no_diagonal_overflow() {
    let mut e = Engine::default();
    let mut p = connect(&mut e);
    p.axes = [500., 500., 0., 0.];
    assert!(e.step(&p, 16, 0.016).is_empty());
    p.axes = [32767., 32767., 0., 0.];
    for effect in e.step(&p, 32, 0.016) {
        if let Effect::Move(x, y) = effect {
            assert!((12..=13).contains(&x));
            assert!((12..=13).contains(&y));
        }
    }
}
#[test]
fn wheel_uses_original_fractional_detents() {
    let mut e = Engine::default();
    let mut p = connect(&mut e);
    p.axes[3] = -32767.;
    let result = e.step(&p, 16, 0.016);
    assert!(result.contains(&Effect::Scroll(29, 0)));
}
#[test]
fn calibration_signed_and_unsigned() {
    assert!(normalize(32768, (0, 65535)).abs() < 1.);
    assert_eq!(normalize(0, (0, 65535)), -32767.);
    assert_eq!(normalize(65535, (0, 65535)), 32767.);
    assert!(normalize(0, (-32768, 32767)).abs() < 1.);
}
#[test]
fn navigation_wraps_and_clamps_short_rows() {
    let mut e = Engine::default();
    e.navigate(-1, 0);
    assert_eq!(e.view.col, 12);
    e.navigate(0, 3);
    assert_eq!(e.view.row, 3);
    assert_eq!(e.view.col, 10);
    e.navigate(1, 0);
    assert_eq!(e.view.col, 0);
}
#[test]
fn accent_composition_shift_and_cancel() {
    let mut e = Engine::default();
    assert!(
        !e.activate(Action::Character('´', '`'))
            .iter()
            .any(|a| matches!(a, Effect::Text(_)))
    );
    e.view.shift = true;
    assert!(
        e.activate(Action::Character('a', 'A'))
            .contains(&Effect::Text('Á'))
    );
    assert!(!e.view.shift);
    assert_eq!(e.view.preview, "Á");
    e.activate(Action::Character('~', '^'));
    assert!(
        !e.activate(Action::Key(Key::Backspace))
            .contains(&Effect::Key(Key::Backspace))
    );
    assert_eq!(e.view.preview, "Á");
}
#[test]
fn invalid_accent_combination_keeps_both_characters() {
    let mut e = Engine::default();
    e.activate(Action::Character('~', '^'));
    let result = e.activate(Action::Character('x', 'X'));
    assert!(result.contains(&Effect::Text('~')));
    assert!(result.contains(&Effect::Text('x')));
    assert_eq!(e.view.preview, "~x");
}
#[test]
fn opening_clears_pending_composition() {
    let mut e = Engine::default();
    e.show(true);
    e.activate(Action::Character('´', '`'));
    e.show(false);
    e.show(true);
    assert!(e.view.preview.is_empty());
    assert!(
        e.activate(Action::Character('a', 'A'))
            .contains(&Effect::Text('a'))
    );
}
#[test]
fn auto_close_does_not_override_manual_open() {
    let mut e = Engine::default();
    let mut p = connect(&mut e);
    e.show(true);
    e.focus(true);
    e.focus(false);
    assert!(e.view.visible);
    e.show(false);
    p.axes[0] = 8000.;
    e.step(&p, 16, 0.016);
    e.focus(true);
    assert!(e.view.visible);
    e.focus(false);
    assert!(!e.view.visible);
}

#[test]
fn connected_idle_gamepad_does_not_open_on_physical_text_focus() {
    let mut e = Engine::default();
    connect(&mut e);
    assert!(e.focus(true).is_empty());
    assert!(!e.view.visible);
    assert!(!e.view.controller_active);
}

#[test]
fn gamepad_click_allows_auto_open_but_physical_input_takes_over() {
    let mut e = Engine::default();
    let mut p = connect(&mut e);
    p.buttons[Button::A as usize] = true;
    e.step(&p, 16, 0.016);
    assert!(e.focus(true).is_empty());
    assert!(!e.view.visible); // do not interrupt the mouse-down
    p.buttons[Button::A as usize] = false;
    assert!(e.step(&p, 32, 0.016).contains(&Effect::Mouse(0, false)));
    assert!(e.step(&p, 152, 0.016).contains(&Effect::ReleaseAll));
    assert!(e.view.visible);
    assert!(e.view.controller_active);
    let epoch = e.view.physical_input_epoch;
    assert_eq!(e.physical_activity(), vec![Effect::ReleaseAll]);
    assert!(!e.view.visible);
    assert!(!e.view.controller_active);
    assert_ne!(e.view.physical_input_epoch, epoch);
    // Firefox may emit a new focus pair on keyboard input/layout changes.
    e.focus(false);
    e.focus(true);
    assert!(!e.view.visible);
    assert!(
        !e.step(&p, 168, 0.016)
            .iter()
            .any(|effect| matches!(effect, Effect::Text(_) | Effect::Key(_)))
    );
}

#[test]
fn controller_click_reopens_the_same_field_after_physical_typing() {
    let mut e = Engine::default();
    let mut p = connect(&mut e);
    e.focus(true);
    e.physical_activity();
    // Moving the pointer must remain possible without opening the keyboard.
    p.axes[0] = 16000.;
    e.step(&p, 16, 0.016);
    p.axes[0] = 0.;
    assert!(!e.view.visible);
    p.buttons[Button::A as usize] = true;
    let down = e.step(&p, 32, 0.016);
    assert!(down.contains(&Effect::Mouse(0, true)));
    assert!(
        !down
            .iter()
            .any(|effect| matches!(effect, Effect::Text(_) | Effect::Key(_)))
    );
    assert!(!e.view.visible);
    p.buttons[Button::A as usize] = false;
    let up = e.step(&p, 48, 0.016);
    assert!(up.contains(&Effect::Mouse(0, false)));
    assert!(!e.view.visible);
    // No new AT-SPI focus event: the field never lost focus.
    e.step(&p, 168, 0.016);
    assert!(e.view.visible);
    e.focus(false);
    assert!(!e.view.visible); // reopened automatically, not pinned manually
}

#[test]
fn pending_controller_click_respects_blur_physical_input_and_manual_close() {
    for cancel in 0..4 {
        let mut e = Engine::default();
        let mut p = connect(&mut e);
        e.focus(true);
        p.buttons[Button::A as usize] = true;
        e.step(&p, 16, 0.016);
        p.buttons[Button::A as usize] = false;
        e.step(&p, 32, 0.016);
        match cancel {
            0 => {
                e.focus(false);
            }
            1 => {
                e.physical_activity();
            }
            2 => {
                e.show(false);
            }
            _ => {
                e.fullscreen(true);
            }
        }
        e.step(&p, 152, 0.016);
        assert!(!e.view.visible, "cancellation case {cancel}");
    }
}

#[test]
fn dismissing_auto_keyboard_does_not_immediately_reopen_it() {
    let mut e = Engine::default();
    let mut p = connect(&mut e);
    e.focus(true);
    p.buttons[Button::A as usize] = true;
    e.step(&p, 16, 0.016);
    p.buttons[Button::A as usize] = false;
    e.step(&p, 32, 0.016);
    e.step(&p, 152, 0.016);
    assert!(e.view.visible);
    p.buttons[Button::B as usize] = true;
    e.step(&p, 168, 0.016);
    p.buttons[Button::B as usize] = false;
    e.step(&p, 184, 0.016);
    e.focus(true);
    p.axes[0] = 16000.;
    e.step(&p, 320, 0.016);
    assert!(!e.view.visible);
    toggle(&mut e, &mut p, 336);
    assert!(e.view.visible);
}

#[test]
fn back_shortcut_wins_over_a_pending_click_open() {
    let mut e = Engine::default();
    let mut p = connect(&mut e);
    e.focus(true);
    p.buttons[Button::A as usize] = true;
    e.step(&p, 16, 0.016);
    p.buttons[Button::A as usize] = false;
    e.step(&p, 32, 0.016);
    p.buttons[Button::Back as usize] = true;
    e.step(&p, 64, 0.016);
    e.focus(false);
    e.focus(true);
    e.step(&p, 152, 0.016);
    assert!(!e.view.visible);
    p.buttons[Button::Back as usize] = false;
    e.step(&p, 168, 0.016);
    assert!(e.view.visible);
}

#[test]
fn physical_takeover_during_a_held_click_does_not_reopen_on_release() {
    let mut e = Engine::default();
    let mut p = connect(&mut e);
    e.focus(true);
    p.buttons[Button::A as usize] = true;
    e.step(&p, 16, 0.016);
    e.physical_activity();
    p.buttons[Button::A as usize] = false;
    e.step(&p, 32, 0.016);
    e.step(&p, 152, 0.016);
    assert!(!e.view.visible);
    assert!(!e.view.controller_active);
}

#[test]
fn physical_input_closes_manual_overlay_and_back_can_reopen() {
    let mut e = Engine::default();
    let mut p = connect(&mut e);
    e.show(true);
    e.physical_activity();
    assert!(!e.view.visible);
    toggle(&mut e, &mut p, 16);
    assert!(e.view.visible);
}

#[test]
fn analog_noise_does_not_reopen_after_physical_typing() {
    let mut e = Engine::default();
    let mut p = connect(&mut e);
    p.axes[0] = 18000.;
    e.step(&p, 16, 0.016);
    e.physical_activity();
    for (n, value) in [18010., 17990., 18500., 17500.].into_iter().enumerate() {
        p.axes[0] = value;
        let effects = e.step(&p, (n as u64 + 2) * 16, 0.016);
        assert!(
            !effects
                .iter()
                .any(|effect| matches!(effect, Effect::Move(..)))
        );
        e.focus(false);
        e.focus(true);
        assert!(!e.view.visible);
    }
    p.axes[0] = 0.;
    e.step(&p, 100, 0.016);
    assert!(!e.view.controller_active);
    p.axes[0] = -16000.;
    e.step(&p, 120, 0.016);
    e.focus(false);
    e.focus(true);
    assert!(e.view.visible);
}

#[test]
fn desktop_activity_discards_key_identity_and_ignores_leds() {
    use controllerapp_linux::activity::is_activity;
    use evdev::{EventType, InputEvent};
    for code in [2, 30, 42, 48, 57] {
        assert!(is_activity(&InputEvent::new(EventType::KEY.0, code, 1)));
        assert!(is_activity(&InputEvent::new(EventType::KEY.0, code, 2)));
        assert!(!is_activity(&InputEvent::new(EventType::KEY.0, code, 0)));
    }
    assert!(!is_activity(&InputEvent::new(EventType::LED.0, 1, 1)));
    assert!(!is_activity(&InputEvent::new(
        EventType::SYNCHRONIZATION.0,
        0,
        0
    )));
    assert!(is_activity(&InputEvent::new(EventType::RELATIVE.0, 0, 1)));
}

#[test]
fn recentering_rearms_gamepad_takeover_in_the_same_direction() {
    let mut e = Engine::default();
    let mut p = connect(&mut e);
    p.axes[0] = 18000.;
    e.step(&p, 16, 0.016);
    e.physical_activity();
    assert!(e.step(&p, 32, 0.016).is_empty());
    p.axes[0] = 0.;
    e.step(&p, 48, 0.016);
    p.axes[0] = 18000.;
    assert!(
        e.step(&p, 64, 0.016)
            .iter()
            .any(|effect| matches!(effect, Effect::Move(..)))
    );
    assert!(e.view.controller_active);
}
#[test]
fn altgr_is_not_left_alt() {
    assert_ne!(key_code(Key::Alt), key_code(Key::AltGr));
}
#[test]
fn compact_geometry_preserves_rows() {
    let layout = layout::main_layout();
    assert_eq!(
        layout.iter().map(Vec::len).collect::<Vec<_>>(),
        vec![13, 13, 12, 11]
    );
    for row in controllerapp_linux::drawing::key_rects(850., 270., &layout) {
        for r in row {
            assert!(r.x >= 0. && r.y >= 0. && r.w > 0. && r.h > 0.);
            assert!(r.x + r.w <= 850. && r.y + r.h <= 270.);
        }
    }
}

#[test]
fn slower_pointer_preserves_fractional_precision_and_curve() {
    for axis in [1500., 2500., 8000., 16000., 32767.] {
        for direction in [-1., 1.] {
            let mut e = Engine::default();
            let mut p = connect(&mut e);
            p.axes[0] = axis * direction;
            let mut distance = 0;
            for n in 1..=100 {
                for effect in e.step(&p, n * 16, 0.016) {
                    if let Effect::Move(x, y) = effect {
                        distance += x;
                        assert_eq!(y, 0);
                    }
                }
            }
            let original = 2. + 28. * ((axis - 1500.) / (32767. - 1500.)).powf(1.4);
            assert!((distance as f32 - original * 0.6 * 100. * direction).abs() < 1.1);
            assert_ne!(distance, 0);
            p.axes = [0.; 4];
            assert!(e.step(&p, 1616, 0.016).is_empty());
        }
    }
}

#[test]
fn reduced_pointer_speed_is_independent_of_poll_interval() {
    for (interval, steps) in [(0.008, 200), (0.016, 100)] {
        let mut e = Engine::default();
        let mut p = connect(&mut e);
        p.axes[0] = 32767.;
        let mut total = 0;
        for n in 1..=steps {
            for effect in e.step(&p, n * 8, interval) {
                if let Effect::Move(x, _) = effect {
                    total += x;
                }
            }
        }
        assert_eq!(total, 1800);
    }
}

#[test]
fn slower_pointer_does_not_change_keyboard_navigation() {
    let mut e = Engine::default();
    let mut p = connect(&mut e);
    e.show(true);
    p.axes[0] = 32767.;
    e.step(&p, 135, 0.016);
    assert_eq!(e.view.col, 1);
    e.step(&p, 200, 0.016);
    assert_eq!(e.view.col, 1);
    e.step(&p, 270, 0.016);
    assert_eq!(e.view.col, 2);
}

#[test]
fn responsive_layout_fits_small_and_scaled_logical_monitors() {
    use controllerapp_linux::{
        drawing::key_rects,
        theme::{Metrics, overlay_size},
    };
    assert_eq!(overlay_size(1920, 1080), (850, 270));
    assert_eq!(overlay_size(1366, 768), (850, 270));
    for (mw, mh) in [
        (640, 480),
        (800, 600),
        (960, 540),
        (1024, 768),
        (1366, 768),
        (1920, 1080),
        (3840, 2160),
    ] {
        let (w, h) = overlay_size(mw, mh);
        assert!(w < mw && h < mh);
        let m = Metrics::new(w as f64, h as f64);
        for layer in [layout::main_layout(), layout::function_layout()] {
            for row in key_rects(w as f64, h as f64, &layer) {
                for r in &row {
                    assert!(r.w > 20. && r.h > 20.);
                    assert!(r.x >= m.inset && r.y >= m.grid_top);
                    assert!(r.x + r.w <= w as f64 - m.inset + 0.01);
                    assert!(r.y + r.h < m.footer_top);
                }
                for pair in row.windows(2) {
                    assert!(pair[0].x + pair[0].w < pair[1].x);
                }
            }
        }
    }
}

#[test]
fn english_labels_keep_portuguese_characters_and_accents() {
    let labels: Vec<_> = layout::main_layout()
        .into_iter()
        .flatten()
        .map(|k| k.label)
        .collect();
    for name in ["Backspace", "Space", "Enter", "FN", "ç", "´", "~"] {
        assert!(labels.iter().any(|s| s == name));
    }
    for (accent, letters, expected) in [
        ('´', "aeiouc", "áéíóúç"),
        ('~', "aon", "ãõñ"),
        ('^', "aeiou", "âêîôû"),
        ('`', "aeiou", "àèìòù"),
    ] {
        for (letter, result) in letters.chars().zip(expected.chars()) {
            assert_eq!(layout::compose(accent, letter), Some(result));
            assert_eq!(layout::decompose(result), Some((accent, letter)));
        }
    }
}

#[test]
fn compact_animation_opens_and_closes_completely() {
    use controllerapp_linux::{drawing::Animation, model::View};
    let mut animation = Animation::default();
    let mut view = View {
        visible: true,
        ..View::default()
    };
    for _ in 0..20 {
        animation.advance(&view, 0.016);
    }
    assert_eq!(animation.opacity, 1.);
    assert!(animation.slide < 0.05);
    view.visible = false;
    for _ in 0..20 {
        animation.advance(&view, 0.016);
    }
    assert_eq!(animation.opacity, 0.);
}
#[test]
fn fn_layer_has_all_original_keys() {
    let mut e = Engine::default();
    e.activate(Action::Layer);
    assert!(e.view.function);
    let actions: Vec<_> = e.layouts[1].iter().flatten().map(|k| k.action).collect();
    for n in 1..=12 {
        assert!(actions.contains(&Action::Key(Key::F(n))));
    }
    assert!(actions.contains(&Action::Key(Key::AltGr)));
}

#[test]
fn held_connection_button_cannot_complete_mode_chord() {
    let mut e = Engine::default();
    let mut p = Pad {
        connected: true,
        ..Pad::default()
    };
    p.buttons[Button::Back as usize] = true;
    e.step(&p, 0, 0.016);
    p.buttons[Button::Start as usize] = true;
    assert!(
        !e.step(&p, 16, 0.016)
            .iter()
            .any(|e| matches!(e, Effect::Rumble(..)))
    );
    assert!(e.view.enabled);
}

#[test]
fn preview_is_bounded_for_y_button_and_unicode() {
    let mut e = Engine::default();
    let mut p = connect(&mut e);
    e.show(true);
    for n in 0..400 {
        e.activate(Action::Character('ç', 'Ç'));
        p.buttons[Button::Y as usize] = true;
        e.step(&p, n * 32 + 16, 0.016);
        p.buttons[Button::Y as usize] = false;
        e.step(&p, n * 32 + 32, 0.016);
    }
    assert_eq!(e.view.preview.chars().count(), 256);
    e.show(false);
    assert!(e.view.preview.is_empty());
}

#[test]
fn scroll_continues_while_keyboard_is_open() {
    let mut e = Engine::default();
    let mut p = connect(&mut e);
    e.show(true);
    p.axes = [32767., 0., 0., -32767.];
    let effects = e.step(&p, 16, 0.016);
    assert!(effects.contains(&Effect::Scroll(29, 0)));
    assert!(!effects.iter().any(|e| matches!(e, Effect::Move(..))));
}

#[test]
fn desktop_clicks_do_not_repeat_while_held() {
    let mut e = Engine::default();
    let mut p = connect(&mut e);
    p.buttons[Button::A as usize] = true;
    assert!(e.step(&p, 16, 0.016).contains(&Effect::Mouse(0, true)));
    for n in 2..20 {
        assert!(e.step(&p, n * 16, 0.016).is_empty());
    }
    p.buttons[Button::A as usize] = false;
    assert_eq!(e.step(&p, 320, 0.016), vec![Effect::Mouse(0, false)]);
}
