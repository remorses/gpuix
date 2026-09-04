# AGENTS.md - GPUIX Codebase Guide

**Read [README.md](./README.md) first** to understand what GPUIX is, the architecture, mutation API, event flow, supported elements/events/styles, and the test renderer.

Not **remorses**? Do not open a pull request. Open an issue. See [External contributors](#external-contributors).

## README is the public API contract

Document every user-facing feature, element, prop, event, renderer option,
public method, and behavior change in `README.md` in the same change. A
changeset does not replace API documentation.

## GPUIX is a thin layer on GPUI

**Read the GPUI docs and the GPUI source before you write native code.** `zed/crates/gpui`
is checked out in this repository. `gpui::ListState`, `gpui::div`, `gpui::Window` and the
rest are the real API; GPUIX only translates a React tree into calls on them.

Do not invent behaviour on top of GPUI. If a GPUIX element needs something GPUI does not
do, the order is:

1. Find the GPUI API that already does it. Search `zed/crates/gpui` for the symbol
2. Search `zed-industries/zed` issues and PRs. Someone may have shipped it already
3. Fix it in the `remorses/zed` fork as a normal GPUI change, and bump the submodule
4. Only then, add GPUIX code

**Never paper over GPUI in `packages/native`.** A workaround that re-applies state after
GPUI computed it, patches a value GPUI owns, or reaches around a GPUI invariant will break
on the next submodule bump and is very hard to debug. When such a change is unavoidable,
it must state in a comment what GPUI does, why that is not what GPUIX needs, and which
GPUI call makes it safe.

Prefer the smallest translation. Fewer moving parts is more important than matching any
other framework's behaviour.

## Project Goal

GPUIX enables building **native GPU-accelerated desktop applications** using **React and TypeScript**, powered by [GPUI](https://github.com/zed-industries/zed/tree/main/crates/gpui) (Zed's rendering framework).

Instead of Electron/web rendering, your React components render directly to the GPU via Metal/Vulkan.

## Mouse capture is armed by the press

A `div` with `onMouseDown`, `onMouseMove`, and `onMouseUp` keeps move and up after the pointer leaves the hitbox. GPUIX arms that automatically when the same node listens for down and move, using GPUI's window-level mouse listeners.

Put all three on the element the user grabs. Capture is armed by the **press**, so an overlay mounted during that press never arms it, and a release past the window edge is lost. `examples/timeline.tsx` drags clips, trims edges, scrubs, and marquee-selects with no overlay at all.

```
React (TypeScript)  →  napi-rs  →  GPUI (Rust)  →  GPU
     Your code         Bridge      Native render    Metal/Vulkan
```

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────────┐
│  JavaScript / TypeScript                                        │
│                                                                 │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │  Your React App                                          │   │
│  │                                                          │   │
│  │  function App() {                                        │   │
│  │    const [count, setCount] = useState(0)                 │   │
│  │    return (                                              │   │
│  │      <div style={{ display: 'flex', bg: '#1e1e2e' }}>    │   │
│  │        <div onClick={() => setCount(c => c + 1)}>+</div> │   │
│  │      </div>                                              │   │
│  │    )                                                     │   │
│  │  }                                                       │   │
│  └─────────────────────────────────────────────────────────┘   │
│                              ↓                                  │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │  @gpuix/react (packages/react)                           │   │
│  │                                                          │   │
│  │  - React Reconciler (react-reconciler)                   │   │
│  │  - Builds element tree from React components             │   │
│  │  - Serializes to JSON ElementDesc                        │   │
│  │  - Manages event handler registry                        │   │
│  └─────────────────────────────────────────────────────────┘   │
│                              ↓ JSON                             │
└─────────────────────────────────────────────────────────────────┘
                               ↓ napi-rs FFI
┌─────────────────────────────────────────────────────────────────┐
│  Rust / Native                                                  │
│                                                                 │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │  @gpuix/native (packages/native)                         │   │
│  │                                                          │   │
│  │  - GpuixRenderer: receives JSON, triggers re-render      │   │
│  │  - build_element(): ElementDesc → GPUI elements          │   │
│  │  - apply_styles(): StyleDesc → GPUI style methods        │   │
│  │  - Event handlers → ThreadsafeFunction callbacks to JS   │   │
│  └─────────────────────────────────────────────────────────┘   │
│                              ↓                                  │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │  GPUI (from zed)                                         │   │
│  │                                                          │   │
│  │  - Immediate-mode UI framework                           │   │
│  │  - Flexbox layout via Taffy                              │   │
│  │  - GPU rendering via Metal (macOS) / Vulkan (Linux)      │   │
│  │  - Native window management                              │   │
│  └─────────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────────┘
```

## Key Insight: Immediate Mode Alignment

GPUI is **immediate-mode** - it rebuilds the entire UI tree every frame. This actually aligns perfectly with React's model:

| Traditional DOM Renderer | GPUIX |
|--------------------------|-------|
| `appendChild(node)` | Rebuild tree each render |
| `node.style.color = x` | Send full tree description |
| Mutation-based | Description-based |

We don't fight GPUI's architecture - we embrace it by sending a complete element description on every React render.

## Package Structure

```
gpuix/
├── cli/                         # `gpuix new` project scaffolder
│   ├── src/cli.ts               # Goke CLI and example-app extraction
│   └── package.json
│
├── packages/
│   ├── native/                 # Rust napi-rs bindings
│   │   ├── src/
│   │   │   ├── lib.rs          # Module exports
│   │   │   ├── renderer.rs     # GpuixRenderer, GpuixView, build_element()
│   │   │   ├── element_tree.rs # ElementDesc, EventPayload types
│   │   │   ├── style.rs        # StyleDesc, color parsing
│   │   │   ├── theme.rs        # Comet palette, oklch helpers, JS overrides
│   │   │   ├── text/           # Selection: state, paint registry, TextRuns
│   │   │   ├── syntax/         # Syntect highlighting + bounded cache
│   │   │   ├── markdown/       # pulldown-cmark parser + gpui renderer
│   │   │   ├── diff/           # Unified-patch parser + row flattening
│   │   │   └── custom_elements/# input, img, svg, anchored, code, diff, markdown, shimmer, terminal, browser
│   │   ├── examples/
│   │   │   └── hello.rs        # Pure GPUI test (no JS)
│   │   ├── Cargo.toml
│   │   └── build.rs
│   │
│   └── react/                  # React reconciler
│       ├── src/
│       │   ├── index.ts        # Public exports
│       │   ├── reconciler/
│       │   │   ├── host-config.ts  # React reconciler implementation
│       │   │   ├── reconciler.ts   # ReactReconciler instance
│       │   │   └── renderer.ts     # createRoot(), event bridge
│       │   ├── hooks/
│       │   │   ├── use-gpuix.ts    # Context access
│       │   │   └── use-window-size.ts
│       │   └── types/
│       │       └── host.ts     # TypeScript types
│       └── package.json
│
├── examples/
│   ├── package.json            # Workspace package for examples
│   └── counter.tsx             # Example React app
│
└── AGENTS.md                   # This file
```

## Text rendering: one funnel, no exceptions

Every string GPUIX paints goes through `crate::text`:

- `selectable_text(..)` for content — registers into the per-frame selection
  registry and installs the window mouse and key listeners
- `chrome_text(..)` for line numbers, language tags and file headers — painted
  and logged for tests, but never part of a selection

**Never call `div().child(some_string)` in a new element.** Doing so makes the
text invisible to selection AND to `getPaintedText()`, so it cannot be tested
except by screenshot.

The registry is rebuilt during **paint**, not during build, because paint order
is the only place document order is guaranteed: a `list()` decides at paint time
which rows exist. `selection_frame_reset()` must stay the first child of the
root, or stale entries from the previous frame leak into the next drag.

## Text is grouped, because React splits one line into many nodes

`shouldSetTextContent` is false, so `<text>Hello {name}!</text>` becomes **three**
host text nodes and three `TextLayout`s. Anything that reasons about a line, copy
and `highlight` both, must merge them first.

The rule is structural: **adjacent** primitive text children of the same parent
form one group. Never derive it from `display`; `apply_styles` only knows `flex`
and `grid`, and every text node already sits inside a `div`.

Copy and search must agree, so both call `search::group_id`. Using
`element.parent` for one and adjacency for the other is a silent divergence: a
non-text sibling between two leaves ends the run for search but not for copy.
`None` means a run that never merges, which is every native element line.

## `highlight` resolves per subtree, with two caches

`crate::text::search` deliberately builds **no joined document** for a query. It
matches per group, so a 5k-row chat is 5k small strings rather than one megabyte
string rebuilt on every keystroke. A joined document exists only when a spec
supplies explicit `ranges`, because only then does someone index into it.

`HighlightCacheEntry` has two levels and they must stay separate:

- the `GroupList` is keyed by **`search_revision`**
- the `MatchSet` is additionally keyed by `HighlightSet::matcher_hash()`, which
  **excludes** `activeIndex` and the colours; a cursor move swaps only the spec

`search_revision` exists because `highlight` is itself a custom prop, so
`subtree_revision` moves on every keystroke. Key the group list on that and a
find-bar keystroke re-walks and re-folds the whole subtree.

**A timing budget does not catch this.** On the 1000-turn chat the broken version
is 2.7ms against 1.9ms, because most of that text lives in native element props
rather than retained `<text>`. `highlight_cache_tests` in `renderer.rs` compares
`Arc` identity instead, and fails outright.

Native elements (`<code>`, `<markdown>`, `<diff>`) generate text inside
`render()`, so the build-time resolver cannot see it. They match the exact string
they are about to paint, through `washes_for_native_run`. Do **not** add a second
traversal that re-derives their text and `sub` values; markdown assigns `sub`
with a render-time counter and the two would drift.

**Ordinals are allocated during paint, inside `search::wash`, for retained and
native matches alike.** `activeIndex` means the nth match in the document, and
only paint knows that order: retained matches are located before the frame,
native text exists only during it, and a subtree can interleave them. Numbering
each kind separately made `activeIndex: 0` mark the `<text>` match even when a
`<code>` block came first.

Two things that look redundant there are not. `MatchId::Retained` carries the
build-time index so a match split over several interpolated runs takes exactly
one ordinal. The `assigned` memo makes a row gpui paints twice keep its numbers
rather than advance the cursor again.

Paint only sees what is mounted, so a **virtualized subtree must say where it
starts**. `matchIndexOffset` is the number of **matches** above the window, not
a row index, and the sequence begins there. Without it `activeIndex` silently
means "the nth visible match" and a find cursor lands on the wrong row.
`<virtual-list>` already takes `windowStart` and `itemCount` from the app for the
same reason. It is excluded from `matcher_hash`, like `activeIndex`, so scrolling
never rescans text. A malformed value **rejects the whole spec**: a bad offset
only shows up as a cursor on the wrong row, so silence is worse than nothing.

`useTextSearch` takes both numbers as one `matches: { total, indexOffset }`
object. They are never individually correct: native counts and numbers the same
window, so an app that overrides one must override the other.

`onHighlight` is queued during build and flushed **after** the root build
returns, keyed on `MatchSet::identity()` rather than the count. Emitting inline
lets a `setState` in the handler re-enter the build; keying on the count alone
misses a query swap that finds the same number of hits, and including the
colours makes a cursor move look like a new result. `reported` is written only
when an event is really queued, or adding `onHighlight` after the first render
would report nothing forever.

A `<virtual-list>` row is built from `cx.processor` **after** the root render
returned, so `build_virtual_child` re-resolves the declaration against the tree
as it is then. On Windows and Linux the Node thread can commit new text in
between, and a captured range would paint over the wrong glyphs.

Content is searchable even under `userSelect: "none"`, because a browser still
finds it. `chrome_text` cannot paint a wash, so it is only for real chrome:
gutters, language tags, diff file headers.

## Layout numbers live in `Theme::metrics`, not in Rust constants

Row heights, gutter widths, paddings, text sizes and the heading scale are all
fields on `crate::theme::Metrics`, reachable from JS as `theme.metrics`.

**Do not add a new `const` for anything that decides layout.** Put it on
`Metrics`, give it a default, add it to `MetricsOverride`, `hash_into`, and the
`GpuixMetrics` TypeScript interface. The whole point is that a design tweak is a
React re-render, not a native rebuild.

Two things stay constant, because they are paint geometry and cannot move a
glyph: the table hairline, and the inline-code wash overhang.

`<diff>` derives its virtualized height model from the metrics without
measuring, so `DiffElement` re-runs `reset_with_uniform_height` whenever
`Metrics::hash_into` changes. Forget that and the scrollbar drifts from the
content.

## Iterating on the Rust side

There is no hot reload and there cannot be: `require()` of a `.node` calls
`process.dlopen`, Node has no unload, and the event loop, GPU device, window and
selection registry all live in thread-locals of the loaded library.

Use `bun run dev` (see `scripts/dev.ts`). It watches `packages/native/src`,
rebuilds, and re-renders the screenshot tests. **A Rust edit reaches fresh PNGs
in about 4 seconds.** Prefer screenshot mode over `--app`: PNGs in
`packages/react/screenshots/` can be read by an agent, a live window cannot.

**Never ship or start the app on a debug native build.** `bun run build:debug`
and `cargo build` without `--release` produce an unoptimized `.node`. GPUI
paint is then many times slower, and that looks like an app bug. Always use
`bun run build` in `packages/native` (release). Use `build:debug` only when
the user asks, or when a debug-only tool (lldb, sanitizers) cannot run on
release. After any debug build, rebuild release before starting `chat.tsx`
or judging frame time.

## Two Bun modes, only one of them refreshes React

`bun --hot chat.tsx` is the **runtime**. It re-evaluates the module graph in the
same process, so `render()` finds its window on `globalThis.__gpuixRenderHost`
and remounts. There is no bundler, so there is no Fast Refresh transform and no
`import.meta.hot`. Every save loses `useState`. Tracked upstream as
[oven-sh/bun#40179](https://github.com/oven-sh/bun/issues/40179).

`bun scripts/web.ts` is the **bundler dev server**. Bun applies the Fast Refresh
transform and its HMR runtime calls `refreshRuntime.injectIntoGlobalHook(window)`,
which is the only thing our reconciler needs: `injectIntoDevTools()` in
`reconciler.ts` hands the hook `scheduleRefresh` and `setRefreshHandler`, and
`react-refresh` drives updates through them.

Delete that call and you get **no error and no page reload**. Bun still accepts
the update and still calls `performReactRefresh()`, which iterates zero mounted
roots and schedules nothing. The bundle changes and the painted UI stays stale.
`fast-refresh.test.tsx` is the only thing that catches it.

Do **not** assert on the return value of `injectIntoDevTools()`. It ends in
`hook.checkDCE ? true : false`, and `react-refresh` installs no `checkDCE`, so a
working injection still returns `false`. `fast-refresh.test.tsx` asserts the
behaviour instead: `_getMountedRootCount()`, then a component swap that keeps
state.

**Never add `import.meta.hot.accept("./app", ...)` to a browser entry.** Bun runs
an importer's dependency-accept callback even when the imported module already
self-accepted for Fast Refresh. The callback then remounts on top of a
successful refresh and wipes every hook. This looks exactly like Fast Refresh
being broken, and it is not.

**Never run `bun run clean` in `packages/react` while the web dev server is up.**
That folder is inside the dev server's module graph, so removing it under a
running server permanently corrupts the registry: every page load then fails
with `Failed to load bundled module 'packages/react/dist/index.js'`, even after
a hard reload, and only a server restart clears it. This is why `build` is plain
`tsc` and the wipe lives in a separate `clean` script. Clean first, then start
the server.

## A new element needs a host-derived GPUI id, or it has no state

`.id(..)` is not decoration. gpui keys `InteractiveElementState` off the
`GlobalElementId`, so an element without one silently loses **hover, active,
pointer capture, implicit scroll, its accessibility node, and any element state
gpui itself keeps**. `<img>` had no id, which is why an animated GIF never left
frame zero: `ImgState` holds the frame index.

`<div>` and `<text>` use `gpui::ElementId::Integer(host_id)`. Host ids are
already unique per renderer, and a formatted name cost a `SharedString`
allocation on every node on every frame. Custom elements use
`ElementId::Name("__gpuix_<kind>_<host id>")`; that is a different enum variant,
so the two namespaces cannot collide.

**Never call `.id(<index>)` in this crate.** `impl From<usize> for ElementId`
makes the idiomatic gpui row id an `Integer`, which is the same namespace as a
host id. Every per-row id here is a formatted name for that reason:
`__gpuix_diff_line_{ix}`, `__gpuix_md_table_{id}_{sub}`, and the rest.

**Never call `apply_styles` on a stateful root. Call `apply_interactive_styles`.**
`StyleDesc` carries `hover` and `active` for every element type, so a builder
that applies only the base styles type-checks the prop, serializes it, and drops
it. `custom_surface` in `custom_elements/mod.rs` does this for you.

## Bounds: a container uses a tracker, a leaf uses `on_painted`

`getByTestId(..).click()` needs a recorded box. Two mechanisms, both required:

- **Containers** (`<div>`, `<text>`, `<code>`, `<diff>`, `<markdown>`, `<input>`)
  add `crate::automation::bounds_tracker(id, selection_start)` as a child. It is
  `absolute().size_full()`, so the parent must be positioned. Pass
  `Some(selectable)` when the element also owns a selection-start region; the
  editor uses `Some(false)` so a drag moves the caret instead of starting a
  document selection. `custom_surface` attaches it.
- **Leaves** (`<img>`, `<svg>`) and **`<anchored>`** use
  `crate::automation::track_own_bounds(el, id)`, which is gpui's `on_painted`.
  Wrapping a leaf in a div instead would move the layout box: the wrapper
  becomes the flex item, and the image loses intrinsic sizing and corner
  clipping. `<anchored>` uses it because only gpui knows where the overlay
  landed after snapping.

Both record during **paint**, and `bounds_frame_reset` clears the registry
during paint too. Never move any of them to prepaint: `gpui::list()` prepaints a
speculative row range, then rolls the window back through `Window::transact` and
prepaints a different one, so a prepaint-recorded box can belong to a row that
never reached the screen.

## A macOS menu item owns its shortcut, so the window never sees it

`crate::app_menu` installs the App and Window menus during `init_macos`. GPUI
does not do this on its own: `NSApp.mainMenu` stays nil, macOS paints an empty
menu bar, and `⌘Q`, `⌘H`, `⌘M` and `⌘W` do not exist, because AppKit only
provides them through menu items.

**Never add an Edit menu carrying `⌘C` / `⌘V` / `⌘X` / `⌘A`.** AppKit consumes a
key equivalent before the window sees the key event, so those items would take
the keystroke away from the selection listener in `text::paint` and from the
per-focus clipboard handling in `custom_elements::input`. An Edit menu needs
those handlers moved into GPUI actions first.

`gpui::App::set_menus` reads each shortcut out of the keymap, so bind the keys
**before** you call it. Window-level items (`MinimizeWindow`, `ZoomWindow`,
`CloseWindow`) go through `with_window_menu_actions` on the root element in
`GpuixView::render`, because a `Window` exists nowhere else; app-level ones
(`Quit`, `Hide`, `HideOthers`, `ShowAll`) are `cx.on_action` globals.

Two things real AppKit decides for you. The **title of the application menu is
the executable name**, not the `Menu` name you pass, so `bun app.tsx` shows
`bun`; only a `.app` bundle changes it. And the menu named `Window` is handed to
`setWindowsMenu:`, which prepends AppKit's own tiling items, `Enter Full Screen`
included. Do not add that item yourself.

Verify with the accessibility tree, not a screenshot. The system menu bar is
outside the window, so GPUIX automation cannot see it.

```bash
osascript -e 'tell application "System Events" to tell (first process whose unix id is PID) \
  to get name of every menu bar item of menu bar 1'
```

## Browser keyboard input goes through GPUI's hidden element

A GPUI web app has two event surfaces. The `<canvas>` takes pointer events.
A hidden `[data-gpui-input]` element appended to `<body>` takes **every keyboard
and IME event**: `gpui_web`'s `listen_input` attaches `keydown` / `keyup` there,
not to the window or the canvas. Dispatching a synthetic `KeyboardEvent` at that
element is therefore the only way automation can type into a browser app.

**Match it by attribute only** (`IME_MIRROR_SELECTOR` in `automation/client.ts`).
It used to be an `<input>`; [zed-industries/zed#63201](https://github.com/zed-industries/zed/pull/63201)
replaced it with a `<textarea>` because a single-line input strips newlines from
an assigned value and desynchronises the mirror from the document. Our
tag-qualified selector was never updated, so after that submodule bump every
browser keystroke failed. A tag-qualified selector will do it again.

## Virtualized React children re-enter through `cx.processor`

`<virtual-list>` does not build its retained children during `GpuixView::render`.
Its `gpui::list()` callback uses `cx.processor` to re-enter the `GpuixView`
entity after the root render has returned, creates a fresh `BuildCtx`, and builds
only the rows GPUI requests. Never capture the root render's tree guard or
`BuildCtx` in that callback.

`<diff>` still owns its parsed Rust data because one native diff node is much
cheaper than retaining one React node per line.

## Nested scrolling is not supported

Never put a scroll container inside another scroll container. That includes
`overflow: "scroll"`, `<virtual-list>`, and `<diff>` (`gpui::list()` always
takes the wheel). GPUI delivers the same wheel event to both hitboxes. The
inner list steals the gesture. Nested scroll looks broken and there is no
GPUI API to turn list scroll off.

Keep **one** scroll parent. Long inner content must grow with that parent, or
collapse behind an expandable (file header, first N lines, Show more). `<diff>`
defaults to flow layout. Pass `scroll` plus a bounded height only for a
dedicated viewer. Do not give `<diff>` a bounded height inside a parent
scroller just so it can virtualize.

`overflow-x: scroll` is allowed inside a vertical scroller. GPUI remaps a
vertical wheel onto overflow-x unless `restrict_scroll_to_axis()` is set.
Every `overflow_x_scroll()` in native code must call that, or the parent
scroller jumps sideways when the pointer is over `<code>` or a markdown table.

A **two-axis** `overflow: "scroll"` sets `allow_concurrent_scroll`. GPUI's
default zeroes the smaller of the two deltas, so one diagonal wheel moved one
axis. A browser moves both, and a two-axis container is exactly where a user
expects that.

## A prepended row is only visible at the top

`gpui::ListState` anchors on a **logical item**, and `splice_focusable` shifts
that anchor by the number of rows inserted before it. So a prepend keeps the
rows already on screen and pushes the new one above the viewport. That is
correct for a history pane, and wrong for a feed.

A browser anchors the same way and suppresses it at `scrollTop: 0`. GPUIX copies
that: `VirtualListEntry::sync` remembers a top-aligned, non-`followTail` list
whose `logical_scroll_top()` is `{0, 0}` and calls `scroll_to(default)` after the
splice. Do not "simplify" that away.

**Do not trust a short list to prove a prepend works.** While the content is
shorter than the viewport, gpui's "does not fill" branch re-anchors to item 0 on
every layout, so the drift is invisible. It appears on the frame where the list
first overflows. `example-app` looked stuck on two rows for exactly that reason,
and the regression test in `virtual-list.test.tsx` grows a 160px list from 2 rows
to 12 rather than starting tall.

**A loading row is the anchor while the reader waits in it**, so an
infinite-scroll prepend splices the page in *under* it and replaces the screen
the reader was looking at. The splice-shift above only protects an anchor
*below* the insert point. The app owns the correction, because only it knows the
loading row stands for the arriving content: read `getListScrollTop`, commit,
then `scrollToItem(listId, indexOfTheMessageUnderTheVoid, offsetInVoid -
EDGE_HEIGHT)`. The negative offset anchors the viewport top above that row and
gpui resolves it at layout time against the freshly measured new rows, which is
what makes the restore pixel-exact; any pixel math done in JS would trust
`estimatedItemHeight` and still jump. The append twin: a reader waiting at a
trailing loading row usually rests on gpui's **at-end sentinel**
(`itemIndex == item count`, stored `logical_scroll_top` is `None`), not inside
the void, so the offset is meaningless there; convert with the viewport height
from the same tuple (`EDGE_HEIGHT - viewportHeight`). Traps that cost a
session each:

- virtual-list `scrollToItem` is **queued and applied after the next render's
  splice** (`PENDING_VIRTUAL_LIST_SCROLLS` in `renderer.rs`). Applying it
  eagerly let `splice_focusable` shift the just-restored anchor a second time
  on the live renderer, while the test renderer hid it because
  `TestRenderer.scrollToItem` flushes first
- a bottom-aligned list with a trailing loading row starts **scrolled to the
  end**, i.e. showing that loading row. In tests, wheel direction is therefore
  ambiguous at mount: the first wheel tick can trigger a `next` fetch even when
  the test means to scroll up. Start from the latest page (no trailing edge) or
  `scrollToItem` onto content first. `infinite-chat.test.tsx` does both

## A frozen header cannot use native scroll

GPUI moves a scroll container on the wheel frame. The `onScroll` callback that
would move a sibling pane arrives a frame later, so a ruler synced that way
tears away from its content during a fast pan.

When two panes must stay locked to the pixel, **React owns the offset**: one
`onScroll` listener on a non-scrolling parent, `scrollX` / `scrollY` in state,
and one absolutely positioned wrapper per pane carrying the translation. Zed
does the same; the editor owns its scroll position and paints the gutter and the
text from it. `examples/timeline.tsx` is the worked example.

Those wrappers must set `pointerEvents: "none"`. A positioned box takes hits
even with no fill, so otherwise it swallows every press meant for the surface
behind it. Its children keep their own hitboxes.

Keep the moving subtree in a `memo` component whose props do not change during a
pan. Then a wheel costs a handful of style mutations instead of one per row.
`examples/timeline.perf.test.tsx` measures both halves.

## Scroll cost

A wheel event calls `cx.notify` on the one `GpuixView`. That rebuilds the
tree. `gpui::list()` then re-renders every **visible** item. Cached heights
only skip overdraw items that are off screen.

```
wheel  ►  notify GpuixView  ►  render()  ►  Taffy on visible rows  ►  paint
```

If scroll is smooth on empty padding and slow or stuck on text, a filled
child is stealing the wheel. `occlude()` is **BlockMouse**. It stops the
hit test. The parent list never sees the event. Every painted or positioned
`div` must use `block_mouse_except_scroll()`. `occlude()` is reserved for
`pointerEvents: "auto"` and for `<anchored>`, which sets it itself.

The chat "jank" over code and tables was the Y-to-X remap above, not the
tick loop. After that fix, remaining cost is Taffy on fat visible rows.
`<code>` is one flex row per line. Safe-mdx is ~100 host nodes. Flatten
paint before changing the frame loop.

Keep `<virtual-list>` `overdraw` modest. 820px on a short chat kept almost
every row live. Profile with `debugFrameOverlay: 'full'`. The overlay is
draw time, not FPS. `8.3 MS` is about 120 Hz.

A long `{rows.map(...)}` is slow **at start**. `createInstance` runs in the
render phase. The host `<virtual-list>` children API retains every child. Pass
`itemCount` and `windowStart` and render only that slice so React mounts a
window too. After mount, scroll cost is visible Taffy only.

**There is no `VirtualList` wrapper component and there must not be one.** The
window is app state. A generic wrapper cannot know when to widen its own
window, so it silently dropped rows whenever `itemCount` grew without a scroll,
which is exactly what a filter does.

Keep chrome state out of the component that maps the list. `memo(Transcript)`
so a sidebar click or composer keystroke does not remap every row. A 5k-row
chat paid 250ms per click before that. Profile that path with
`INTERACT=1 bun profile-chat-scroll.tsx`. Do not treat a fast wheel flush as
proof that chrome updates are cheap.

## Profiling and optimizing

Load the **profano** skill first. Fetch its README. Do not guess CLI flags.

Separate **first mount**, **scroll**, and **chrome setState**. They are
different paths.

```
first mount
  React maps every child
    ►  createInstance / setStyle / setCustomProp  (queued)
    ►  one applyBatch JSON
    ►  Rust RetainedTree
    ►  first paint (list builds visible rows only)

scroll
  wheel  ►  notify GpuixView  ►  render()  ►  Taffy on visible rows  ►  paint

chrome setState
  sidebar click / composer key
    ►  parent re-render
    ►  {rows.map(...)} again unless memo(list)
    ►  same JS cost as mount if you forget
```

### JS / mount

Write a short script that mounts through `createTestRoot()` and exits. Profile
that, not the live window. The tick loop will drown the mount.

```ts
import React from 'react'
import { createTestRoot } from '@gpuix/react'
import { ChatApp } from './chat'

const root = createTestRoot()
const start = performance.now()
root.render(<ChatApp turnCount={10_000} />)
console.log(`mount ${(performance.now() - start).toFixed(1)}ms`)
```

```bash
cd examples
MOUNT_ONLY=1 bun --cpu-prof --cpu-prof-dir=../tmp/cpu-profiles profile-chat-scroll.tsx
INTERACT=1 bun profile-chat-scroll.tsx
npx profano ../tmp/cpu-profiles/CPU.*.cpuprofile -n 30
npx profano ../tmp/cpu-profiles/CPU.*.cpuprofile --sort total -n 20
```

Read **self** first. That is where the CPU sat. **Total** is the caller chain.

The 10k chat mount was 850ms. profano said:

| Function | Self | What it was |
|---|---|---|
| `applyBatch` | 626ms | Rust parsing the mutation JSON |
| `FiberNode` | 31ms | React |
| `stringify` | 26ms | `JSON.stringify(queue)` |

React was not the problem. The batch **stringified every style and theme**, then
stringified the queue, then Rust parsed each escaped string again.

Queue **raw objects**. `setStyle` and `setCustomProp` both carry raw JSON values.
Never encode either value before the outer batch is stringified. Doing so adds
a second parse and turns strings such as `"top"` into nested JSON.

```ts
queue.push(['setStyle', id, styleObject])
queue.push(['setCustomProp', id, 'side', 'top'])
```

After a JS reconciler change, **build `@gpuix/react`**. `examples/` and
`bun --hot chat.tsx` load `packages/react/dist`, not `src`. packages/react
vitest uses `src`. You will think the fix works in one suite and fail in the
app.

```bash
cd packages/react && bun run build
```

### Scroll / paint

Turn on `debugFrameOverlay: 'full'`. The number is **draw time**, not FPS.
`8.3 MS` is about 120 Hz.

The chat wheel jank was **not** the tick loop. GPUI remaps a vertical wheel
onto `overflow-x`. `<code>` and markdown tables stole the gesture. Fix is
`restrict_scroll_to_axis()` on every `overflow_x_scroll()`.

Keep `overdraw` modest. 820px on a short list keeps almost every row live.

Do not flatten the frame loop to hide fat rows. Flatten the rows
(`<markdown>` / `<code>` / `<diff>` as one native node).

### Native

For Rust time, `sample` the bun/node pid, or `samply`. GPUI also has
`ZED_MEASUREMENTS=1`. That is Zed's frame log, not our overlay.

A `.node` cannot unload. After a native rebuild, restart the app. `bun --hot`
only remounts React.

### Mutation wire format and tree memory

Do not swap the `applyBatch` codec before reading
[docs/serialization-benchmark.md](./docs/serialization-benchmark.md). It
measures both halves on the real `ChatApp` queue, and the answer is that the
codec is the **smallest** lever.

```bash
cd examples && TURNS=10000 SAFE_MDX=1 bun run bench:serde
cd packages/native && cargo run --release --example bench_serde
```

Four results that shaped the current code:

- `StyleDesc` is **1392 bytes**. Putting it in an enum variant makes the whole
  `Vec<BatchOp>` 1408 bytes wide, so a 221k-op mount reserved 312 MB before
  parsing anything. Never inline a style in an op
- **styles are hash-consed in Rust, not in the protocol.** `RetainedTree::intern_style`
  hashes the raw payload and shares one `Arc<StyleDesc>`. Do not move this into
  JS: `commitUpdate` resends the full style every commit, a dragged element
  produces a distinct style every frame, and a JS-owned table would grow forever
  while sending two ops where it sends one today
- `sweep_styles` runs after every batch and drops entries with
  `Arc::strong_count == 1`. That is the only thing keeping the table bounded
- `RetainedElement.style` is `Option<Arc<StyleDesc>>`. Read it with
  `.as_deref()`. The motion path must copy out before mutating, or one
  element's animation restyles every element that declared the same style

No mainstream JS↔Rust codec deduplicates repeated string **values**. msgpackr
`useRecords` deduplicates keys, and its output is not plain MessagePack, so
`rmp_serde` cannot read it. MessagePack measured 1.24x, which does not pay for a
new dependency on both sides.

## Overlays and icons

Menus, tooltips, and dialogs go through **`SelectContent` / `ComboboxContent` /
`<anchored deferred>`**. Never overflow a `position: "absolute"` card out of the
composer into a `<virtual-list>`. The list paints after the composer, so the
list shows through the menu and clicks hit the text behind it.

Do not paint `#00000000` over a blurred window. A transparent GPUI quad punches
through Metal to the desktop. Omit the fill, or use the parent color. Overlay
rows need a **solid** fill too, not a transparent idle state.

Any `div` that paints a fill, or that is positioned, uses
**BlockMouseExceptScroll**. Clicks and hovers stop, the wheel still reaches the
scroll hitboxes behind it. Only `pointerEvents: "auto"` uses **BlockMouse** and
steals the wheel.

That is not DOM bubbling. GPUI hitboxes are one flat painted list, so the wheel
reaches **any** scroller behind the element, not only an ancestor. An absolute
card over an unrelated scroller scrolls it. Give a real overlay
`pointerEvents: "auto"`.

`pointerEvents: "none"` means this element inserts **no hitbox**, so nothing
behind it is blocked. It does not disable the listeners on the element itself,
and it does not inherit.

Absolute used to steal the wheel too. That made a pannable canvas impossible:
every absolutely placed item ended the hit test before the ancestor's pan
listener ran, and HTML does not behave that way either. `<anchored>` has its own
`occlude` prop, so menus never depended on the old rule.

An absolutely positioned wrapper with **no** fill still takes hits, like an
empty positioned `div` in a browser. A translating wrapper that only carries a
scroll offset must set `pointerEvents: "none"`, or it swallows every press meant
for the surface behind it. Its children keep their own hitboxes:
`pointerEvents` does not inherit.

Text **selection** still uses window mouse events and text bounds, not hitboxes.
A drag on a menu over markdown can still start a selection. Do not skip
selection tests to hide that.

If `<svg>` icons are blank in vitest, `src` is probably a `data:image/svg+xml`
URL from `import … with { type: 'file' }`. Native decodes that URL. Do not write
a temp-file workaround. Prefer `fill="#000"` / `stroke="#000"` plus
`style.color`. `currentColor` in the file is not `style.color`.

macOS traffic-light clearance is **86px**. The test renderer does not draw
traffic lights, so that gap looks empty in PNGs.

## Ported code

`text/`, `syntax/`, `markdown/`, `diff/`, `theme.rs`, `custom_elements/code.rs`,
`custom_elements/diff.rs`, and the caret blink sections of
`custom_elements/input.rs` are ported from [Comet](https://github.com/zeronsh/comet)
(MIT). Each file names its original in
its header, and `THIRD_PARTY_NOTICES.md` has the full table. When fixing a bug in
one of them, read the Comet original first: it usually documents why the code is
shaped that way.

## Auto-generated files (do NOT edit manually)

The following files in `packages/native/` are auto-generated by napi-rs during `bun run build`. Never edit them by hand — they are regenerated from the Rust `#[napi]` annotations every build:

- `packages/native/index.d.ts` — TypeScript type declarations
- `packages/native/index.js` — Node.js loader/binding glue
- `packages/native/*.node` — compiled native binary

To update the TypeScript API surface, edit the Rust source files in `packages/native/src/` (add/modify `#[napi]` structs, methods, functions), then run `bun run build` in `packages/native` to regenerate.

## Changesets

**Always** add a `.changeset/*.md` file after a user-facing fix or feature. Do this before you consider the work done. Never skip it. Never edit CHANGELOG.md. Never bump `package.json` version by hand.

Load the `changesets` skill for format and rules. If the change fixes a GitHub issue or should close a PR, put `Fixes #N` / `Closes #N` on its own line. changepub copies those onto the release commit.

## Publishing

**Never publish from a local machine.** CI is the only release path.

`.github/workflows/ci.yml` builds `@gpuix/native` for **one target per OS**, uploads the `.node` artifacts, then the `publish` job downloads them, runs `napi create-npm-dirs` + `napi artifacts`, and publishes `@gpuix/native` and `@gpuix/react`. The independent `publish-cli` job tests and publishes `@gpuix/cli` without waiting for native platform tests.

| OS | Target | Renderer | Features |
|---|---|---|---|
| macOS | `aarch64-apple-darwin` | Metal | `test-support` |
| Linux | `x86_64-unknown-linux-gnu` | Vulkan / wgpu | `--no-default-features` |
| Windows | `x86_64-pc-windows-msvc` | Direct3D | `test-support` |

Every extra target is a full gpui build, and gpui is most of the wall clock here,
so the matrix carries the architecture each OS is mostly used on and nothing else.
**The matrix and `napi.targets` in `packages/native/package.json` must list the
same set.** A target in one and not the other means the published loader looks for
a platform package that CI never built. Add a target back when someone asks for it.

Each build job also compiles `examples/chat.tsx` with `bun build --compile` against
that target's `.node`. A release asset is served as raw bytes, so a download loses
the executable bit and, on macOS and Linux, arrives with no extension. The job packs
those two into `example-chat-<target>.tar.gz`, which keeps the mode and names itself.
Windows ships the `.exe` as it is. On `main`, the publish job attaches them to the
`@gpuix/react@x.y.z` GitHub release.

Publish order is required. `@gpuix/react` depends on `@gpuix/native` (`workspace:^`). If React publishes first, an install in that window cannot resolve native.

1. `napi pre-publish` publishes the per-platform packages (`darwin-arm64`, `linux-x64-gnu`, …)
2. `npm publish` publishes `@gpuix/native`
3. `npm publish` publishes `@gpuix/react`

`@gpuix/cli` publishes independently. It resolves the latest published React
version when `gpuix new` runs, so it has no package dependency on this sequence.

A local `npm publish` / `bun publish` would ship only the host binary and break every other platform. `prepublishOnly` exits if `CI` is unset.

### Create the GitHub release before CI gets to the publish job

The example binaries are attached by tag, and **the upload step gives up when that
release does not exist**:

```bash
if ! gh release view "$TAG" >/dev/null 2>&1; then
  echo "No GitHub release for ${TAG}, skipping example upload"
  exit 0
fi
```

So a release created afterwards gets **no binaries**, and the only way to add them
later is to download ~100 MB of artifacts to a laptop and push them back. There is
no API that attaches an artifact to a release: the assets endpoint wants the raw
bytes in the request body, and `gh release upload` cannot read stdin
([cli/cli#5820](https://github.com/cli/cli/issues/5820)). The bytes always move.
Do not let them move through your machine.

Release order:

1. Consume the changesets, bump both packages, write `CHANGELOG.md`, commit
2. `git push origin HEAD:main`. CI starts
3. Tag and create the release **now**, while the six native builds run:

```bash
git tag '@gpuix/native@0.5.0' && git tag '@gpuix/react@0.5.0'
git push origin '@gpuix/native@0.5.0' '@gpuix/react@0.5.0'
gh release create '@gpuix/react@0.5.0' --title '@gpuix/react@0.5.0' \
  --notes-file /tmp/notes.md --latest
```

4. CI publishes npm, then uploads `example-chat-*` to that release with `--clobber`

The `publish` job only runs after every build and both test jobs, which is more than
ten minutes, so step 3 has plenty of slack. Use the current `CHANGELOG.md` section
as the notes. Never `--draft`, never `--prerelease`.

**If CI fails and you push fixes, re-point the tags.** The tag then names an older
commit than the one npm was built from. Delete both tags locally and on the remote,
recreate them on the commit that published, and push again. The upload step matches
on the tag *name*, so the release itself keeps its notes and its assets.

## Communication Flow

### Render Flow (JS → Rust)

```
1. React state changes
         ↓
2. React reconciler builds Instance tree
         ↓
3. instanceToElementDesc() converts to JSON-serializable format:
   {
     type: "div",
     id: "btn-1", 
     style: { display: "flex", backgroundColor: "#ff0000" },
     events: ["click", "mouseEnter"],
     children: [...]
   }
         ↓
4. renderer.render(JSON.stringify(tree))
         ↓
5. Rust parses JSON into ElementDesc structs
         ↓
6. build_element() recursively builds GPUI elements:
   div().id("btn-1").flex().bg(rgba(0xff0000ff)).on_click(...)
         ↓
7. GPUI renders to GPU
```

### Event Flow (Rust → JS)

```
1. User clicks element with id="btn-1"
         ↓
2. GPUI fires click event on element
         ↓
3. Rust closure calls emit_event("btn-1", "click", position)
         ↓
4. ThreadsafeFunction calls into JS with EventPayload
         ↓
5. JS event registry looks up handler:
   eventHandlers.get("btn-1")?.click?.(event)
         ↓
6. React handler runs: onClick={() => setCount(c => c + 1)}
         ↓
7. State update triggers re-render → back to Render Flow
```

## Key Types

### ElementDesc (Rust ↔ JS)

```rust
pub struct ElementDesc {
    pub element_type: String,      // "div", "text", "img"
    pub id: Option<String>,        // For event handling
    pub style: Option<StyleDesc>,  // CSS-like styles
    pub content: Option<String>,   // Text content
    pub events: Option<Vec<String>>, // ["click", "mouseEnter"]
    pub children: Option<Vec<ElementDesc>>,
}
```

### StyleDesc (CSS-like properties)

```rust
pub struct StyleDesc {
    // Flexbox
    pub display: Option<String>,        // "flex"
    pub flex_direction: Option<String>, // "row", "column"
    pub align_items: Option<String>,    // "center", "start", "end"
    pub justify_content: Option<String>,
    pub gap: Option<f64>,
    
    // Sizing
    pub width: Option<DimensionValue>,
    pub height: Option<DimensionValue>,
    
    // Spacing
    pub padding: Option<f64>,
    pub margin: Option<f64>,
    
    // Colors (parsed centrally in src/color.rs with csscolorparser 0.8.3;
    // parser-version changes require running both absolute and relative matrices)
    pub background_color: Option<String>,
    pub color: Option<String>,
    
    // Border
    pub border_radius: Option<f64>,
    pub border_width: Option<f64>,
    pub border_color: Option<String>,
}
```

### EventPayload (Rust → JS)

```rust
pub struct EventPayload {
    pub element_id: String,
    pub event_type: String,  // "click", "mouseEnter", etc.
    pub x: Option<f64>,
    pub y: Option<f64>,
    pub key: Option<String>,
    pub modifiers: Option<EventModifiers>,
}
```

## Building

### Standalone Build

The `zed/` submodule tracks the `gpuix` branch of `remorses/zed`. Cargo uses path
dependencies from that submodule so the native addon and native platforms always
compile from the same source:

**Always keep `zed/` checked out on the local `gpuix` branch. Never
leave the submodule in detached HEAD state**, including after `git submodule update`
or a pointer update. If Git detaches it, immediately switch back to
`gpuix` before doing any other work.

- macOS uses `MacPlatform::new_embedded()` and pumps AppKit on Node's main thread
- Windows and Linux run `gpui_platform::application().run()` on a dedicated UI thread
- `gpui_platform` selects Metal or DirectX for the GPU-backed test renderer
- `core-text = 21.0.0`, `core-graphics = 0.24.0` for macOS

These avoid the core-graphics 0.24 vs 0.25 conflict between `core-text` and Zed's `font-kit` fork.

### Rust toolchain

`rust-toolchain.toml` pins the same channel as `zed/rust-toolchain.toml`. When the
submodule moves, update ours to match or GPUI may not compile.

### Metal toolchain (macOS)

`gpui_apple` compiles `shaders.metal` in its build script. Xcode 26 no longer ships the
Metal compiler by default, so a fresh machine fails with
`cannot execute tool 'metal' due to missing Metal Toolchain`. Install it once:

```bash
xcodebuild -downloadComponent MetalToolchain
```

### Bumping the gpui revision

1. Merge upstream Zed into the `gpuix` branch in `remorses/zed`.
2. Resolve any embedded `gpui_macos` conflicts in a new commit; do not rewrite history.
3. Fast-forward the `zed/` submodule to the updated `gpuix` branch.
4. Match `rust-toolchain.toml` to `zed/rust-toolchain.toml`.
5. Run `cargo check --all-targets`, `bun run build`, and the test suites.

### Search Zed before you touch GPUI

Before you debug a GPUI behaviour, add a GPUIX feature that needs a new GPUI API,
or patch the fork, **search `zed-industries/zed` first**. Zed is a large project
with an active roadmap. The answer is often one of:

- someone already reported the same bug
- an open PR already implements the API, so **wait and bump the submodule**
- a merged PR already added it, so **bump the submodule** instead of writing code
- a closed issue says the Zed team declined it, so plan a fork-only fix

Search issues and PRs together, then search code:

```bash
# issues + PRs, full text
gh search issues --repo zed-industries/zed --include-prs --limit 30 'TransformationMatrix' \
  --json number,title,url,state,isPullRequest \
  --jq '.[] | [.number, .isPullRequest, .state, .title, .url] | @tsv'

# title only, to find the feature rather than every mention
gh search issues --repo zed-industries/zed --include-prs --match title --limit 30 'transform'

# where the API already exists in the tree
gh search code --repo zed-industries/zed --language Rust --limit 30 'TransformationMatrix'
```

Then read the promising ones in full. A closed issue is the important signal, and
its `stateReason` and comments explain whether the idea was rejected or shipped:

```bash
gh issue view 53303 -R zed-industries/zed --json number,title,state,stateReason,body,comments
gh pr view 59413 -R zed-industries/zed --json title,state,body,files,comments,reviews
```

**Use the `--repo` and `--match` flags. Do not put `repo:` or `in:title` inside the
query string.** `gh search` mangles the inline form: `repo:` first fails with
`Invalid search query`, and `in:title ... repo:owner/name` silently drops the repo
filter and returns results from unrelated repositories.

Search the real symbol names, not concepts. `TransformationMatrix`,
`with_element_offset`, and `request_animation_frame` find the discussion.
"animation is slow" does not.

Record the outcome in the changeset or PR body, with issue and PR URLs, so the
next session does not repeat the search.

### Fixing GPUI for GPUIX

The `remorses/zed` fork is part of GPUIX's implementation boundary. Fix GPUI in
the fork when a reusable GPUI API or platform correction keeps GPUIX simpler
and avoids browser, embedded-platform, or event-routing workarounds. Do not keep
a hack in `packages/native` only because the required API is missing upstream.

Fork-only fixes must be normal commits on the `gpuix` branch and
must be pushed to `remorses/zed` before the GPUIX submodule points at them. Never
pin GPUIX to a detached commit that is not reachable from that remote branch.
Use a separate Zed worktree for the change; do not develop or commit inside the
`zed/` build checkout.

```bash
# from gpuixlocal/zed
git fetch origin gpuix
git worktree add /Users/morse/Documents/GitHub/zed-gpuix-<change> \
  -b gpuix-<change> origin/gpuix

# from the Zed worktree, after review
git push origin HEAD:gpuix

# then update this repository to that reachable commit
git fetch origin gpuix
git switch gpuix
git merge --ff-only origin/gpuix
```

Commit the resulting `zed` submodule pointer in GPUIX with the code that uses
the new API. The `.gitmodules` branch remains `gpuix`.

### PRs to Zed

A "PR to Zed" means **upstream** [`zed-industries/zed`](https://github.com/zed-industries/zed)
`main`. This is different from a GPUIX fork fix above. Never open an upstream
PR from this checkout. Never point that PR at `remorses/zed`.

`gpui-macos-embedded` is the head branch for the focused upstream macOS embedding
PR. Keep it limited to that PR. Never put general GPUIX fork changes there and
never point this repository's submodule at it.

Do **not** switch `zed/` to another branch, detach HEAD, commit review markers,
or reset it inside this checkout. That submodule is what GPUIX builds against.
A dirty or incorrectly switched `zed/` breaks the native addon and the test
renderer.

```bash
# from gpuixlocal/zed. leaves this submodule on its current commit
git remote add upstream https://github.com/zed-industries/zed.git  # once
git fetch upstream
git worktree add /Users/morse/Documents/GitHub/zed-<branch-name> -b <branch-name> upstream/main
```

Commit only in that worktree. Do not add comments to Zed source. Push the branch
to `remorses/zed`, then open the PR with `--repo zed-industries/zed --base main`.
After merge, cherry-pick onto `gpuix` and fast-forward the submodule
here. Never run `git reset` in `zed/` to "undo" PR work.

### PRs to GPUIX

When you open a PR with `gh pr create` against **this repo** (`remorses/gpuix`),
the body must name the **harness**, **agent**, and **model** that wrote the
change. Then put **every user prompt** from the session in a collapsed
`<details>` block. Reviewers use that to judge prompt quality and how much
the agent invented.

Do this for `gh pr create` and for later `gh pr edit` if the first body missed
it. Do not add this block to Zed PRs.

```md
**Harness:** OpenCode / Kimaki
**Agent:** build
**Model:** xai/grok-4.6

<details>
<summary>User prompts</summary>

1. first user message, verbatim

2. second user message, verbatim

</details>
```

- **Harness:** the product that ran the agent. Examples: OpenCode, Kimaki,
  Claude Code, Cursor, Codex.
- **Agent:** the named agent if the harness has one (`build`, `plan`, `opus`).
  Write `none` if there is no named agent.
- **Model:** the exact model id from the session (`xai/grok-4.6`,
  `anthropic/claude-opus-4.6`). Do not guess a shorter marketing name.
- **User prompts:** every user message that drove the PR, in order, verbatim.
  Skip system reminders, tool output, and your own replies. If a prompt is
  huge, keep the full text inside the details block; do not summarize it.

## Current Status

Keep this list in sync with the README **Status** section. User-facing APIs
belong in README. This list is only the remaining engineering work.

### Completed

- [x] React reconciler with mutation-based protocol
- [x] napi-rs FFI bindings and RetainedTree
- [x] Style mapping, including native `hover` / `active`
- [x] Mouse, keyboard, focus, scroll, and click-outside events
- [x] Atomic `applyBatch()` mutation transport
- [x] GPU-backed test renderer
- [x] Native `<input>` and `<textarea>`
- [x] `<img>` (local raster/SVG) and `<svg>` (tintable monochrome icons)
- [x] `<virtual-list>`
- [x] `<code>`, `<diff>`, `<markdown>` with Syntect
- [x] Cross-element text selection
- [x] `highlight` prop: search matches and explicit ranges
- [x] Headless Select, Combobox, Tooltip
- [x] `setWindowTitle`
- [x] Window chrome (`titlebarTransparent`, `windowBackground`, traffic-light position)
- [x] macOS menu bar (`crate::app_menu`, `appName`)
- [x] Background launch (`focus`, `show`, `activateWindow`)
- [x] Last window close quits the process
- [x] Debug frame overlay (`setDebugFrameOverlay`)

### TODO

#### High Priority

- [ ] **Background highlighting** - move Syntect off the frame thread once
      there is a way to request a repaint from a background task

#### Medium Priority

- [ ] **Canvas** - custom drawing element (`<canvas>` is typed, not implemented)

#### Low Priority

- [ ] **Window controls** - resize, minimize (title already works)
- [ ] **Multiple windows** - Support multiple GPUI windows
- [x] **JS remount** - `render()` plus `bun --hot` remounts the React tree on the same window
- [x] **React Refresh in the browser** - `bun run web` keeps `useState` across saves
- [ ] **React Refresh on desktop** - `bun --hot` is the runtime, not the bundler, so it runs no Fast Refresh transform. Tracked as [oven-sh/bun#40179](https://github.com/oven-sh/bun/issues/40179)
- [ ] **Native hot reload** - cannot unload a `.node`. `bun run dev` rebuilds and restarts
- [ ] **DevTools** - React DevTools integration
- [ ] **Animations** - Interpolated style transitions

## Testing

### Unit Tests

```bash
# Rust unit tests (selection, syntax, diff parser, markdown parser, theme)
cd packages/native && cargo test --lib

# React reconciler + GPU-backed test renderer
cd packages/react && bun run test

# Example app tests
cd examples && bun run test

# Starter todo app, driven through the automation client
cd example-app && bun run test

# Chat and timeline draw / chrome regressions (excluded from the default run)
cd examples && bun run test:perf

# macOS CPU clamp. E-cores, not Chrome 6x. Do not set in CI.
THROTTLE=utility bun run test:perf
THROTTLE=utility bun profile-chat-scroll.tsx
THROTTLE=utility bun --hot chat.tsx
```

`examples/chat.perf.test.tsx` and `examples/timeline.perf.test.tsx` are the
automated profiles. They use `createTestRoot()`, not the live window. Assert
**p95 draw / flush ms**, not a per-frame FPS floor.

Timeline drag samples are not comparable to timeline pan samples.
`nativeSimulateMouseMove` flushes before and after the event, so every drag
sample contains two complete GPUI paints. `dispatchScrollWheel` does not flush.

`THROTTLE` re-execs under `taskpolicy -c`. `utility` is an M1/M2 Air CPU proxy.
`background` is harsher, closer to a 2019 Intel Mac. GPU and RAM stay on this
machine. `taskpolicy -c` only works at launch. The vitest config wraps the main
process so workers inherit the clamp. A throttled run **logs** numbers and
skips the default budgets. Those budgets are for an unclamped M-series CPU.

Use `bun run test`, not `bun test`. The suites are vitest, so `bun test` picks the
wrong runner and fails on the `vitest` imports.

### Asserting on native elements

`getAllText()` reads the retained tree, so it only sees `<text>` nodes. `<code>`,
`<diff>` and `<markdown>` paint inside gpui and are invisible to it. Use
`renderer.getPaintedText()` (every string painted last frame, in paint order) and
`renderer.dragSelect(x1, y1, x2, y2)` instead.

`dragSelect` exists because selection listeners are registered during **paint**:
calling `simulateMouseDown` / `Move` / `Up` by hand without a flush between each
step silently selects nothing.

Screenshots go to `packages/react/screenshots/` (gitignored), not `/tmp`, so they
can be inspected after a run.

### Integration Test

```bash
cd examples && GPUIX_BACKGROUND=1 bun --hot chat.tsx
```

Use tuistory for the long-running process. Do not use `tsx` or raw `tmux`.
On macOS and Windows, the background flag keeps the real GPU window from taking
the user's keyboard; live paint, clicks, screenshots, and automation still
work. Linux currently ignores `focus`.

### Drive the live window

**Do not use `usecomputer`, `screencapture`, or desktop clicks.** GPUIX has a
Playwright-like automation API. Full docs are in the README **Automation**
section.

Mark targets with `testId`. Then either:

- `connectTest(renderer)` against `createTestRoot()` in vitest
- `launch({ command, args })` against a child process. The app serves commands
  on stdin only when stdin is a **pipe**

**Always pass `focus: false` when you start a window to check your own work.**
The user is doing something else. A window that activates on launch takes the
keyboard mid-sentence, once per iteration, and there is no reason for it:
`click()` and `screenshot()` never need focus. Wire the entry file so the flag
comes from the environment, then set it in `launch({ env })`, so a human run
still behaves normally.

```tsx
render(<App />, { focus: process.env.GPUIX_BACKGROUND !== '1' })
```

`fill()` and `press()` use the live GPUI window input pipeline and do not need
the desktop window to become active.

```ts
import { launch } from '@gpuix/react/automation'

const app = await launch({
  command: 'bun',
  args: ['chat.tsx'],
  cwd: 'examples',
  env: { GPUIX_BACKGROUND: '1' },
})
await app.getByTestId('sidebar-collapse').waitFor({ timeoutMs: 30_000 })
await app.screenshot({ path: 'tmp/chat.png' })

const startedAt = await app.clock.pause()
await app.getByTestId('sidebar-collapse').click()
await app.captureFrames('tmp/sidebar', [startedAt, startedAt + 100, startedAt + 200])
await app.clock.resume()
await app.close()
```

`click()` hits the last painted bounds. `clock.pause` / `set` / `fastForward`
freeze native motion. `captureFrames` writes one PNG per timestamp. That is how
you record a sidebar open/close, not a screen recorder.

## Related Projects

- [GPUI](https://github.com/zed-industries/zed/tree/main/crates/gpui) - Zed's GPU UI framework
- [opentui](https://github.com/anomalyco/opentui) - Terminal UI with React (reconciler reference)
- [create-gpui-app](https://github.com/zed-industries/create-gpui-app) - Official GPUI starter template
- [react-reconciler](https://github.com/facebook/react/tree/main/packages/react-reconciler) - React's custom renderer API

## External contributors

This section is for anyone who is not [remorses](https://github.com/remorses) (Tommy).

**Do not open a pull request.** Open a GitHub issue. Describe the bug or the idea. Wait.

Open a PR only after remorses says it is OK on that issue. Unsolicited PRs will be closed.

If remorses says OK, follow the rest of this file and these rules.

**How to work**

1. For Rust changes, work in `zed/crates/gpuix` (easier to build)
2. Copy changes to `packages/native/src/` when ready
3. TypeScript changes can be made directly in `packages/react/`

**Do not**

- Edit auto-generated files: `packages/native/index.d.ts`, `packages/native/index.js`, `packages/native/*.node`. Change the Rust `#[napi]` source and run `bun run build` in `packages/native`
- Edit `CHANGELOG.md` or bump `package.json` version by hand
- Publish from a local machine. CI is the only release path
- Branch, commit, or reset the `zed/` submodule in this checkout. Do not open a Zed PR from here
- Ship or start the app on a debug native build. Use `bun run build` in `packages/native` (release)
- Use `bun test`. The suites are Vitest. Use `bun run test`

**Must**

- Add a `.changeset/*.md` file for every user-facing fix or feature. Put `Fixes #N` on its own line when the work closes an issue
- Run the package test scripts: `packages/react` then build `@gpuix/react`, then `examples`
- Keep one scroll parent. Nested scrolling is not supported
- Send every painted string through `crate::text`. Never `div().child(some_string)`
- Put layout numbers on `Theme::metrics`, not new Rust constants

If an agent writes the change, the PR body must include **harness**, **agent**, **model**, and every user prompt. See **PRs to GPUIX**.


## Examples using same tech as ours. To unblock on issues and compare to our code

For example usage of projects depending on gpui in rust: opensrc https://github.com/zed-industries/create-gpui-app

For examples of NAPI rs native packages: https://github.com/napi-rs/package-template and https://github.com/Brooooooklyn/Image

For reading gpui source code: https://github.com/zed-industries/sed inside crates/gpui

For examples of a custom React renderer: https://github.com/anomalyco/opentui inside packages/react
