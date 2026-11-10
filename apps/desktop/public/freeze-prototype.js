// SPEC §20.1, ADR-0025: the app's own pages run with Object.prototype frozen
// (prototype-pollution guard). This used to be Tauri's `freezePrototype`,
// but Tauri injects that into every webview, including the in-app sign-in
// window, where it broke the sites' own scripts (VK stayed blank, Reddit's
// robot check never drew). Loaded first, before any other script.
Object.freeze(Object.prototype);
