// Which side of the panel a keypress asks for.
//
// Tab is deliberately not a switch key: the settings side has a checkbox, a
// slider and four buttons, and stealing Tab would leave no way to walk between
// them. So the arrows switch while the cursor is on the segmented row itself
// (the tab pattern every toolkit uses), and Cmd+1 / Cmd+2 reach a side from
// anywhere in the panel.

export const SIDES = ["settings", "shortcuts"];

/// `press.code` is the physical key: a Russian layout prints other letters, and
/// on the digits it prints nothing different — but matching by code keeps this
/// consistent with the popup's own key handling.
export function sideFor(press, current) {
  const { code, meta, onSwitch } = press;

  if (meta && code === "Digit1") return SIDES[0];
  if (meta && code === "Digit2") return SIDES[1];

  if (onSwitch && (code === "ArrowLeft" || code === "ArrowRight")) {
    const step = code === "ArrowRight" ? 1 : -1;
    const i = SIDES.indexOf(current);
    // Wraps: with two sides either arrow simply means "the other one", which is
    // what a hand on the keyboard expects from a two-item row.
    return SIDES[(i + step + SIDES.length) % SIDES.length];
  }

  return null;
}
