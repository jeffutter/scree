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

// Scope picker and page filters: a change submits the GET form; the server
// renders the page for the chosen scope. Without script the Apply or Filter
// button, which is inside <noscript>, does the same.
document.addEventListener("change", (e) => {
  const form =
    e.target instanceof Element && e.target.closest("form[data-picker], form[data-autosubmit]");
  if (!form) return;
  // The Worktree select still holds the old Project's value; leave it out so
  // the server treats the new Project as Project-wide.
  if (e.target.name === "p") {
    const w = form.querySelector("select[data-picker-w]");
    if (w) w.disabled = true;
  }
  form.requestSubmit();
});

// List and Decisions keys: j/k move a selection among the rows, Enter opens
// it, Esc clears it, "/" focuses the search box (List only). The selection is remembered by ticket id and
// re-applied after every live patch (a patch rewrites the attributes). When
// the selected ticket is no longer in the list the page says so instead of
// quietly dropping the selection.
(() => {
  const list = document.getElementById("list") || document.getElementById("decisions");
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

// Two-step buttons: a button with data-confirm arms on the first click (its
// label becomes the data-confirm text) and acts on the second. Leaving the
// button or pressing Esc disarms it. Without script the first click acts.
(() => {
  const disarm = (b) => {
    if (b.dataset.armed === undefined) return;
    b.textContent = b.dataset.label;
    delete b.dataset.armed;
    delete b.dataset.label;
  };
  document.addEventListener(
    "click",
    (e) => {
      const b = e.target instanceof Element && e.target.closest("[data-confirm]");
      if (!b || b.dataset.armed !== undefined) return;
      e.preventDefault();
      e.stopPropagation();
      b.dataset.label = b.textContent;
      b.dataset.armed = "";
      b.textContent = b.dataset.confirm;
    },
    true,
  );
  document.addEventListener("focusout", (e) => {
    if (e.target instanceof Element && e.target.matches("[data-confirm]")) disarm(e.target);
  });
  document.addEventListener("keydown", (e) => {
    if (e.key === "Escape")
      document.querySelectorAll("[data-confirm][data-armed]").forEach(disarm);
  });
})();

// Modal dialogs (ticket detail): a button with data-dialog-open="ID" opens the
// <dialog>; Cancel (data-dialog-close) and Esc close it. A live patch rewrites
// the region and drops the `open` attribute of a dialog the user is typing in,
// so the ids of the open dialogs are remembered and re-opened after each
// patch. A form that was accepted (its err slot is empty once the request
// finishes) closes its dialog.
(() => {
  const openIds = new Set();
  const dlg = (id) => document.getElementById(id);

  document.addEventListener("click", (e) => {
    if (!(e.target instanceof Element)) return;
    const opener = e.target.closest("[data-dialog-open]");
    if (opener) {
      const d = dlg(opener.dataset.dialogOpen);
      if (d && !d.open) {
        d.showModal();
        openIds.add(d.id);
      }
      return;
    }
    if (e.target.closest("[data-dialog-close]")) {
      const d = e.target.closest("dialog");
      if (d) d.close();
    }
  });

  // Esc closes through the dialog itself; keep the remembered ids in step.
  document.addEventListener(
    "close",
    (e) => {
      if (e.target instanceof HTMLDialogElement) openIds.delete(e.target.id);
    },
    true,
  );
  document.addEventListener("click", (e) => {
    const d = e.target;
    if (!(d instanceof HTMLDialogElement) || !d.open) return;
    // The dialog is the target for its own padding too; only a click outside
    // its box is a backdrop click.
    const r = d.getBoundingClientRect();
    const inside = e.clientX >= r.left && e.clientX <= r.right && e.clientY >= r.top && e.clientY <= r.bottom;
    if (!inside) d.close();
  });

  const reopen = () => {
    for (const id of openIds) {
      const d = dlg(id);
      if (!d) openIds.delete(id);
      else if (!d.open) d.showModal();
    }
  };
  const live = document.getElementById("ticket-live");
  if (live) {
    new MutationObserver(reopen).observe(live, {
      childList: true,
      subtree: true,
      attributes: true,
      attributeFilter: ["open"],
    });
  }

  // A POSTed form closes its dialog when its error slot is empty once the
  // request finishes, and its fields are cleared for the next use. The slot is
  // the form's next sibling, or the id in data-err. A GET (the blocker search)
  // has neither, so it never closes anything.
  document.addEventListener("datastar-fetch", (e) => {
    if (!e.detail || e.detail.type !== "finished") return;
    const form = e.detail.el;
    const d = form instanceof HTMLFormElement && form.closest("dialog");
    if (!d) return;
    const slot = form.dataset.err
      ? document.getElementById(form.dataset.err)
      : form.nextElementSibling;
    if (!slot || !slot.id.startsWith("err-")) return;
    // The refusal fragment is patched in before "finished" is delivered.
    setTimeout(() => {
      if (slot.childElementCount !== 0 || slot.textContent.trim() !== "") return;
      d.close();
      d.querySelectorAll("form").forEach((f) => f.reset());
    }, 0);
  });
})();
