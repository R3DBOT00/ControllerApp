//! Explicit opt-in: creates a temporary virtual gamepad, not a keyboard/mouse.
use controllerapp_linux::{
    hardware::Gamepad,
    model::{Button, Effect, Engine},
};
use evdev::{
    AbsInfo, AbsoluteAxisCode as Abs, AttributeSet, EventType, InputEvent, KeyCode as Key,
    UinputAbsSetup, uinput::VirtualDevice,
};
use std::{thread, time::Duration};

#[test]
#[ignore = "requires host /dev/uinput; run only with ControllerApp stopped"]
fn kernel_gamepad_events_and_disconnect() -> Result<(), Box<dyn std::error::Error>> {
    let mut virtual_pad = VirtualDevice::builder()?
        .name("Rust gamepad regression fixture")
        .with_keys(&AttributeSet::from_iter([
            Key::BTN_SOUTH,
            Key::BTN_EAST,
            Key::BTN_WEST,
            Key::BTN_NORTH,
            Key::BTN_SELECT,
            Key::BTN_START,
        ]))?
        .with_absolute_axis(&UinputAbsSetup::new(
            Abs::ABS_X,
            AbsInfo::new(0, -32768, 32767, 0, 0, 0),
        ))?
        .with_absolute_axis(&UinputAbsSetup::new(
            Abs::ABS_Y,
            AbsInfo::new(0, -32768, 32767, 0, 0, 0),
        ))?
        .with_absolute_axis(&UinputAbsSetup::new(
            Abs::ABS_HAT0X,
            AbsInfo::new(0, -1, 1, 0, 0, 0),
        ))?
        .build()?;
    let path = virtual_pad
        .enumerate_dev_nodes_blocking()?
        .next()
        .ok_or("no event node")??;
    thread::sleep(Duration::from_millis(100));
    let mut pad = Gamepad::open(&path)?;
    let mut engine = Engine::default();
    assert!(engine.step(&pad.state, 0, 0.016).is_empty());
    virtual_pad.emit(&[
        InputEvent::new(EventType::ABSOLUTE.0, Abs::ABS_X.0, 32767),
        InputEvent::new(EventType::KEY.0, Key::BTN_SOUTH.0, 1),
    ])?;
    pad.poll()?;
    let effects = engine.step(&pad.state, 16, 0.016);
    assert!(effects.contains(&Effect::Move(18, 0)));
    assert!(effects.contains(&Effect::Mouse(0, true)));
    virtual_pad.emit(&[
        InputEvent::new(EventType::KEY.0, Key::BTN_SOUTH.0, 0),
        InputEvent::new(EventType::KEY.0, Key::BTN_WEST.0, 1),
        InputEvent::new(EventType::ABSOLUTE.0, Abs::ABS_HAT0X.0, -1),
    ])?;
    pad.poll()?;
    assert!(
        engine
            .step(&pad.state, 32, 0.016)
            .contains(&Effect::Mouse(0, false))
    );
    assert!(pad.state.buttons[Button::X as usize]);
    assert!(!pad.state.buttons[Button::Y as usize]);
    assert!(pad.state.buttons[Button::Left as usize]);
    assert!(pad.events >= 5);
    drop(virtual_pad);
    thread::sleep(Duration::from_millis(50));
    assert!(pad.poll().is_err());
    Ok(())
}
