---
"@gpuix/native": minor
"@gpuix/react": minor
---

Add the tray API and an explicit quit lifecycle:

- `WindowOptions.quitMode`: `"lastWindowClosed"` (default, unchanged) or
  `"explicit"`, which keeps the process running after the last window closes.
- `renderer.quit()` exits the process whatever the `quitMode` is.
- `renderer.createTray({ icon, title, tooltip })` creates the process-wide
  tray item and returns `false` when one already exists or the platform has
  no tray support. GPUI tray events carry no tray identity, so there is one
  tray per process; multi-tray is out of scope.
- `renderer.setTrayMenu(items)` replaces the flat menu (id, label, enabled,
  checked; no submenus, separators, or accelerators). Menu item ids are
  strings; native code never holds a JS callback.
- `renderer.setTrayTooltip(tooltip)` works on all tray platforms;
  `renderer.setTrayTitle(title)` is macOS-only and a no-op elsewhere.
- `renderer.removeTray()` drops the item; a later `createTray` may create a
  new one.
- `renderer.onTrayEvent((event))` reports `{ type: "click" }` (only while the
  item has no menu) and `{ type: "menuItem", id }`. A later call replaces the
  handler.

The TrayHandle is only touched on the GPUI app/UI thread, through the
existing `update_app` channel; the Node thread never holds it.

macOS Dock hiding (`activationPolicy` / Accessory mode) is not included and
is planned as a follow-up.

Fixes #109
