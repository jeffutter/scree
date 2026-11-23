// Keyboard help overlay: "?" opens it, Esc or the Close button closes it.
// Keys are ignored while typing and with modifiers held.
(() => {
  const help = document.getElementById("help");
  if (!help) return;
  const panel = help.querySelector("[role=dialog]");
  let opener = null;

  const typing = (el) =>
    el instanceof Element &&
    el.closest("input, textarea, select, [contenteditable]:not([contenteditable=false])");

  const open = () => {
    opener = document.activeElement;
    help.hidden = false;
    panel.focus();
  };
  const close = () => {
    help.hidden = true;
    if (opener instanceof HTMLElement) opener.focus();
    opener = null;
  };

  document.addEventListener("keydown", (e) => {
    if (e.ctrlKey || e.metaKey || e.altKey) return;
    if (!help.hidden) {
      if (e.key === "Escape") {
        e.preventDefault();
        close();
      } else if (e.key === "Tab") {
        // Keep focus inside the dialog: it has one button besides the panel.
        const btn = help.querySelector("button");
        e.preventDefault();
        (document.activeElement === btn ? panel : btn).focus();
      }
      return;
    }
    if (e.key === "?" && !typing(e.target)) {
      e.preventDefault();
      open();
    }
  });
  help.addEventListener("click", (e) => {
    if (e.target === help || e.target.closest("[data-help-close]")) close();
  });
})();

// Scope picker: a change submits the GET form; the server renders the page for
// the chosen scope. Without script the Apply button does the same.
document.addEventListener("change", (e) => {
  const form = e.target instanceof Element && e.target.closest("form[data-picker]");
  if (form) form.requestSubmit();
});
