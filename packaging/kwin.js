// KWin is the authority for native Wayland and XWayland window state.
// No window titles or typed text leave this script.
function report() {
    const window = workspace.activeWindow;
    const app = window ? String(window.resourceClass).toLowerCase() : "";
    const excluded = /firefox|chromium|chrome|brave|vivaldi|opera|navigator|dolphin|nautilus|thunar|plasmashell|controllerapp/;
    const game = !!window && window.fullScreen && !excluded.test(app);
    callDBus("io.github.controllerapp.ControllerApp", "/io/github/controllerapp/ControllerApp",
             "io.github.controllerapp.ControllerApp", "Fullscreen", game);
}
function watch(window) {
    if (window && window.fullScreenChanged) window.fullScreenChanged.connect(report);
}
workspace.windowList().forEach(watch);
workspace.windowAdded.connect(watch);
workspace.windowActivated.connect(report);
workspace.windowRemoved.connect(report);
report();
