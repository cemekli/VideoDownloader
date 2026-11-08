// Applies the saved theme before the first render, so the window never
// flashes the wrong colours (SPEC §9.6). A plain script file because the
// CSP allows only 'self' scripts. Settings are the source of truth; this
// copy in localStorage only covers the first paint.
(function () {
  var theme = "dark";
  try {
    var v = localStorage.getItem("vv.theme");
    if (v === "light" || v === "system") theme = v;
  } catch (e) {}
  if (theme === "system") theme = window.matchMedia && window.matchMedia("(prefers-color-scheme: light)").matches ? "light" : "dark";
  document.documentElement.dataset.theme = theme;
})();
