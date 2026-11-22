// Theme nach RFC-0006: gespeicherte Wahl je Browser, sonst Systempräferenz, ohne Präferenz Dark.
(function () {
  var choice = null;
  try { choice = localStorage.getItem("trackcraft.theme"); } catch (e) { /* ohne Speicher */ }
  var dark = choice === "dark" || (choice !== "light" && !window.matchMedia("(prefers-color-scheme: light)").matches);
  document.documentElement.classList.toggle("dark", dark);
})();
