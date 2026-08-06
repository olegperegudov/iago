// The panel the menu-bar icon drops down: settings on one side, the cheat sheet
// on the other, one segmented row between them.
//
// Two windows used to do this, reached through two menu items. They shared a
// stylesheet and nothing else, and neither was ever open at a moment when the
// other was not the thing wanted.

import { applyScale } from "./scale.js";
import { mountSettings } from "./settings.js";
import { mountShortcuts } from "./shortcuts.js";
import { sideFor, SIDES } from "./sides.js";

const { invoke } = window.__TAURI__.core;
const { getCurrentWindow } = window.__TAURI__.window;

const tab = (side) => document.querySelector(`#tab-${side}`);
const pane = (side) => document.querySelector(`#pane-${side}`);

let current = SIDES[0];

function show(side, { focusTab = false } = {}) {
  current = side;
  for (const s of SIDES) {
    const chosen = s === side;
    tab(s).setAttribute("aria-selected", String(chosen));
    // Roving tabindex: Tab enters the row once and lands on the live segment,
    // then walks into the pane rather than through every segment.
    tab(s).tabIndex = chosen ? 0 : -1;
    pane(s).hidden = !chosen;
  }
  if (focusTab) tab(side).focus();
}

for (const side of SIDES) {
  tab(side).addEventListener("click", () => show(side));
}

document.addEventListener("keydown", (e) => {
  if (e.key === "Escape") {
    getCurrentWindow().hide();
    return;
  }
  const wanted = sideFor(
    {
      code: e.code,
      meta: e.metaKey || e.ctrlKey,
      onSwitch: e.target.getAttribute?.("role") === "tab",
    },
    current
  );
  if (wanted && wanted !== current) {
    e.preventDefault();
    show(wanted, { focusTab: true });
  }
});

// One round trip for the whole panel: both sides are built from the same
// settings object rather than each fetching its own.
async function load() {
  const cfg = await invoke("get_settings");
  applyScale(cfg.ui_scale);
  mountSettings(cfg);
  mountShortcuts();
  show(current);
}

load();
