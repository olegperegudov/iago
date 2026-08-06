// The cheat-sheet side follows the popup: the same key does different things per
// zone (digits pick a card, or type a digit), so the zone you are standing in is
// the one lit up.

const { listen } = window.__TAURI__.event;

export function mountShortcuts() {
  const zones = document.querySelectorAll(".zone[data-zone]");

  function highlight(zone) {
    zones.forEach((node) => {
      node.classList.toggle("active", node.dataset.zone === zone);
    });
  }

  listen("zone-changed", (event) => highlight(event.payload));

  // The popup is gone — no zone is live, so nothing is lit.
  listen("popup-closed", () => highlight(null));
}
