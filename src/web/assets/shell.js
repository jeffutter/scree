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

// List keys: j/k move a selection among the rows, Enter opens it, Esc clears
// it, "/" focuses the search box. The selection is remembered by ticket id and
// re-applied after every live patch (a patch rewrites the attributes). When
// the selected ticket is no longer in the list the page says so instead of
// quietly dropping the selection.
(() => {
  const list = document.getElementById("list");
  if (!list) return;
  const notice = document.getElementById("list-changed");
  const help = document.getElementById("help");
  let selected = null;

  const rows = () => Array.from(list.querySelectorAll("[data-id]"));
  const typing = (el) =>
    el instanceof Element &&
    el.closest("input, textarea, select, [contenteditable]:not([contenteditable=false])");

  const apply = () => {
    let found = false;
    for (const row of rows()) {
      const on = row.dataset.id === selected;
      found = found || on;
      if (row.hasAttribute("data-selected") !== on) {
        if (on) row.setAttribute("data-selected", "");
        else row.removeAttribute("data-selected");
      }
    }
    if (notice) notice.hidden = selected === null || found;
  };

  const select = (id, row) => {
    selected = id;
    apply();
    if (row) row.scrollIntoView({ block: "nearest" });
  };

  const move = (delta) => {
    const all = rows();
    if (all.length === 0) return;
    const at = all.findIndex((r) => r.dataset.id === selected);
    let next;
    if (at < 0) next = delta > 0 ? 0 : all.length - 1;
    else next = Math.min(all.length - 1, Math.max(0, at + delta));
    select(all[next].dataset.id, all[next]);
  };

  new MutationObserver(apply).observe(list, {
    childList: true,
    subtree: true,
    characterData: true,
    attributes: true,
    attributeFilter: ["data-selected", "data-id"],
  });

  document.addEventListener("keydown", (e) => {
    if (e.ctrlKey || e.metaKey || e.altKey) return;
    if (help && !help.hidden) return;
    if (typing(e.target)) {
      if (e.key === "Escape" && e.target instanceof HTMLElement) e.target.blur();
      return;
    }
    if (e.key === "j") {
      e.preventDefault();
      move(1);
    } else if (e.key === "k") {
      e.preventDefault();
      move(-1);
    } else if (e.key === "Enter") {
      const row = rows().find((r) => r.dataset.id === selected);
      const link = row && row.querySelector("a[href]");
      if (link) {
        e.preventDefault();
        link.click();
      }
    } else if (e.key === "Escape") {
      select(null, null);
    } else if (e.key === "/") {
      const q = document.getElementById("q");
      if (q) {
        e.preventDefault();
        q.focus();
      }
    }
  });
})();
