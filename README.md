# ControllerApp

ControllerApp turns an XInput-compatible gamepad into a mouse and virtual keyboard on Linux. Written in Rust, with a compact Xbox 360-inspired keyboard overlay.

![ControllerApp keyboard](https://github.com/R3DBOT00/ControllerApp)

## Features

- Analog-stick mouse movement, clicking, dragging and scrolling
- Compact on-screen keyboard with controller navigation
- A/B/X/Y button images and a classic green theme
- Portuguese accent input, Shift and FN keys
- Wayland overlay, virtual input via uinput and optional autostart

## Requirements

**Controller support: XInput-compatible gamepads only for now.** Other controller types and modes have not been validated.

Linux with a Wayland compositor supporting layer-shell (tested with KDE Plasma), Rust/Cargo, GTK4 4.18+, gtk4-layer-shell and pkg-config. Native X11 sessions are not supported.

## Build and install

```bash
cargo build --release --locked
./install.sh
```

The installer may request administrator access to configure uinput permissions. Run the application as a normal user.

## Controls

| Button | Action |
| --- | --- |
| Left stick | Mouse / keyboard navigation |
| Right stick | Scroll |
| A / B | Left / right click, or select / close keyboard |
| X / Y | Backspace / Space in keyboard |
| Back | Toggle keyboard |
| Start | Enter and close keyboard |
| Start + Back | Toggle desktop controls |
| D-pad | Navigate keyboard |
| LB / RB / R3 | Shift in keyboard |

## Status

**Work in progress.** Some features and compatibility details may still have bugs. Built primarily for KDE Plasma Wayland.

## Assets

The four A/B/X/Y images included in this repository came from an existing local asset collection. Their original authorship and redistribution terms have not been verified.
