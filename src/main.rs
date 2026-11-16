use controllerapp_linux::{
    drawing,
    hardware::{self, Command, Shared},
    integration, ui, verification,
};
use std::{
    path::Path,
    sync::{Arc, Mutex, mpsc},
    time::Duration,
};
fn main() {
    if let Err(e) = run() {
        eprintln!("ControllerApp: {e}");
        std::process::exit(1);
    }
}
fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.first().is_some_and(|s| s == "--help") {
        println!(
            "ControllerApp Linux\n  --show | --hide | --toggle | --status | --quit\n  --no-auto-open         open manually only\n  --render PATH.png [WIDTH HEIGHT] [--fn] [--shift]\n  --render-public PATH.png [WIDTH HEIGHT] [--fn] [--shift]\n  --self-test            test Wayland and uinput in a separate test window\n  --show-on-start        open the overlay on startup\n  ./install.sh           install on the host (sudo only for permissions)"
        );
        return Ok(());
    }
    if args
        .first()
        .is_some_and(|s| s == "--render" || s == "--render-public")
    {
        let path = args.get(1).ok_or("Missing PNG output path")?;
        let width: i32 = args.get(2).map(|s| s.parse()).transpose()?.unwrap_or(850);
        let height: i32 = args.get(3).map(|s| s.parse()).transpose()?.unwrap_or(270);
        if !(320..=3840).contains(&width) || !(160..=2160).contains(&height) {
            return Err("Render size must be between 320×160 and 3840×2160".into());
        }
        return drawing::render_png(
            Path::new(path),
            width,
            height,
            args[0] == "--render-public",
            args.iter().any(|a| a == "--fn"),
            args.iter().any(|a| a == "--shift"),
        );
    }
    for (arg, method) in [
        ("--show", "Show"),
        ("--hide", "Hide"),
        ("--toggle", "Toggle"),
        ("--status", "Status"),
        ("--quit", "Quit"),
    ] {
        if args.first().is_some_and(|s| s == arg) {
            println!("{}", integration::remote(method)?);
            return Ok(());
        }
    }
    if std::env::var("WAYLAND_DISPLAY").is_err() {
        return Err("A Wayland session is required; native X11 sessions are not supported".into());
    }
    if std::env::var_os("SUDO_UID").is_some() {
        return Err("Run ./install.sh as your normal user; it uses sudo only for permissions. The UI uses your Wayland session.".into());
    }
    if args.iter().any(|s| s == "--self-test") {
        return verification::run();
    }
    if args.iter().any(|s| s == "--test-target") {
        return verification::target();
    }
    let runtime = std::env::var("XDG_RUNTIME_DIR")?;
    let lock = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(Path::new(&runtime).join("controllerapp-v2.lock"))?;
    fs2::FileExt::try_lock_exclusive(&lock)
        .map_err(|_| "An instance is already running; use --show or --quit")?;
    let state = Arc::new(Mutex::new(Shared::default()));
    let (sender, receiver) = mpsc::channel();
    let (ready_tx, ready_rx) = mpsc::channel();
    let signal_sender = sender.clone();
    ctrlc::set_handler(move || {
        let _ = signal_sender.send(Command::Stop);
    })?;
    let shared = state.clone();
    let show = args.iter().any(|s| s == "--show-on-start");
    let worker = std::thread::Builder::new()
        .name("controller-input".into())
        .spawn(move || hardware::run(shared, receiver, ready_tx, show))?;
    ready_rx.recv_timeout(Duration::from_secs(5))??;
    let bus = integration::start(sender.clone(), state.clone())?;
    if !args.iter().any(|s| s == "--no-auto-open") {
        integration::accessibility(sender.clone(), state.clone());
    }
    let result = ui::run(sender.clone(), state);
    let _ = sender.send(Command::Stop);
    let _ = worker.join();
    drop(bus);
    result
}
