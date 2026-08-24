# GPUIX

React bindings for [GPUI](https://github.com/zed-industries/zed/tree/main/crates/gpui) - Zed's GPU-accelerated UI framework.

Build native GPU-accelerated desktop apps with React and TypeScript. Your components render directly to the GPU via Metal, DirectX, or Vulkan. No Electron, no web views.

![A Waku-style app built with GPUIX](docs/images/chat-app.png)

Everything above is GPUIX: the sidebar, the scrolling list, the composer,
and native `<markdown>`. Start it with **`bun --hot`** so a save remounts React
on the same window:

```bash
cd examples && bun --hot chat.tsx
```

## Examples

| Example | Run | What it shows |
|---|---|---|
| **chat** | `bun --hot chat.tsx` | A Waku-style app: transparent titlebar, animated sidebar, message list, composer, `<markdown>` |
| **native-text** | `bun --hot native-text.tsx` | The three native text components with a tab switcher |
| **counter** | `bun --hot counter.tsx` | The smallest possible app: state, events, hover |
| **diff** | `bun --hot diff.tsx` | A diff viewer composed from `<div>` and `<text>` in JS, for comparison |

All of them live in [`examples/`](./examples) and use hardcoded data.

Markdown, code and a virtualized diff in one frame:

![Markdown, code and diff rendered together](docs/images/showcase.png)

## Architecture

GPUIX bridges React to GPUI using a **mutation-based protocol** over napi-rs FFI. React's reconciler sends individual DOM-like mutations (`createElement`, `appendChild`, `setStyle`, etc.) directly to Rust — no JSON tree serialization. Rust maintains a retained element tree that GPUI reads each frame.

```
┌─────────────────────────────────────────────────────────────────┐
│  React (JavaScript)                                             │
│                                                                 │
│  function App() {                                               │
│    const [count, setCount] = useState(0)                        │
│    return (                                                     │
│      <div style={{ display: 'flex', gap: 8 }}>                  │
│        <div onClick={() => setCount(c => c + 1)}>               │
│          Count: {count}                                         │
│        </div>                                                   │
│      </div>                                                     │
│    )                                                            │
│  }                                                              │
└─────────────────────────────────────────────────────────────────┘
                    │ napi FFI mutations
                    │ createElement(1, "div")
                    │ appendChild(0, 1)
                    │ setStyle(1, "{...}")
                    │ commitMutations()
                    ▼
┌─────────────────────────────────────────────────────────────────┐
│  Rust (napi-rs)                                                 │
│                                                                 │
│  RetainedTree ── stores elements, styles, event flags           │
│       │                                                         │
│       ▼  each GPUI frame                                        │
│  GpuixView::render() → build_element() → GPUI elements         │
└─────────────────────────────────────────────────────────────────┘
                    │
                    ▼
┌─────────────────────────────────────────────────────────────────┐
│  GPUI                                                           │
│                                                                 │
│  GPU rendering via Metal (macOS), DirectX (Windows), or Vulkan  │
│  Flexbox layout via Taffy                                       │
└─────────────────────────────────────────────────────────────────┘
```

## Why This Works

GPUI is an **immediate-mode** UI framework — it rebuilds the entire element tree every frame. Instead of fighting this, GPUIX embraces it:

1. React reconciler detects a state change and calls napi mutations (`createElement`, `setStyle`, `appendChild`, etc.)
2. Each mutation updates a **RetainedTree** on the Rust side — a HashMap of element nodes with styles, children, and event flags
3. On each GPUI frame, `GpuixView::render()` walks the RetainedTree and calls `build_element()` to produce ephemeral GPUI elements
4. GPUI lays them out (Taffy flexbox) and renders to the GPU
5. Only **changed elements** cross the FFI boundary — React's reconciler diffs the virtual tree and sends minimal mutations

This is the same protocol React uses for the DOM (`createElement`, `appendChild`, `removeChild`, `commitUpdate`), but targeting a GPU renderer instead of a browser.

## Mutation API

The FFI surface between JS and Rust is a set of direct napi calls — the `NativeRenderer` interface:

```ts
interface NativeRenderer {
  createElement(id: number, elementType: string): void
  destroyElement(id: number): Array<number>
  appendChild(parentId: number, childId: number): void
  removeChild(parentId: number, childId: number): void
  insertBefore(parentId: number, childId: number, beforeId: number): void
  setStyle(id: number, styleJson: string): void
  setText(id: number, content: string): void
  setEventListener(id: number, eventType: string, hasHandler: boolean): void
  setRoot(id: number): void
  commitMutations(): void
}
```

Element IDs are plain numbers generated by an incrementing counter in JS. `commitMutations()` signals the end of a batch — Rust marks the view dirty so GPUI re-renders on the next frame.

## Event Flow

Events travel from GPUI back to React through a `ThreadsafeFunction` callback:

```
User clicks element id=3
       │
       ▼
GPUI fires on_click on the element
       │
       ▼
Rust closure calls emit_event_full(callback, 3, "click", {x, y, ...})
       │
       ▼
ThreadsafeFunction queues EventPayload on Node.js event loop
       │
       ▼
JS event registry: eventHandlers.get(3)?.get("click")?.(payload)
       │
       ▼
React handler runs: onClick={() => setCount(c => c + 1)}
       │
       ▼
State update triggers re-render → reconciler sends mutations back to Rust
```

Event handlers are stored in a JS-side registry keyed by `(elementId, eventType)`. Rust only knows **whether** an element has a listener (via `setEventListener`), not the closure itself — the actual handler lives in JS.

## Packages

- **`@gpuix/native`** — Rust/napi-rs bindings to GPUI. Contains `GpuixRenderer`, `RetainedTree`, `build_element()`, `apply_styles()`, and the event wiring.
- **`@gpuix/react`** — React reconciler, event registry, and TypeScript types. Implements the `react-reconciler` host config using the mutation API.

## Building

### Prerequisites

1. Rust toolchain
2. Node.js 18+
3. Xcode with Metal Toolchain (macOS)

```bash
# Install Metal Toolchain if needed
xcodebuild -downloadComponent MetalToolchain

# Install dependencies
bun install

# Check out the pinned GPUI fork
git submodule update --init --recursive

# Build native package
cd packages/native
bun run build

# Build React package
cd ../react
bun run build

# Run example (use tmux for long-running sessions)
cd ../../examples
bun --hot counter.tsx
```

## Usage

```tsx
import React, { useState } from 'react'
import { render } from '@gpuix/react'

function App() {
  const [count, setCount] = useState(0)
  return (
    <div style={{ display: 'flex', gap: 8, padding: 16 }}>
      <div
        style={{ backgroundColor: '#3b82f6', borderRadius: 8, padding: 12, cursor: 'pointer' }}
        onClick={() => setCount(c => c + 1)}
      >
        <div style={{ color: '#ffffff' }}>Count: {count}</div>
      </div>
    </div>
  )
}

render(<App />, {
  title: 'My App',
  width: 800,
  height: 600,
  titlebarTransparent: true,
  windowBackground: 'blurred',
  trafficLightX: 16,
  trafficLightY: 17,
})
```

`render()` creates the native window, mounts React, and starts the frame loop.
The red traffic-light button quits the process. Start the app again from the
terminal.

| Option | Values | Purpose |
|---|---|---|
| `titlebarTransparent` | boolean | Hide the native titlebar so the app draws chrome under the traffic lights |
| `windowBackground` | `"opaque"` (default), `"transparent"`, `"blurred"` | Window fill. `"blurred"` is the macOS vibrancy backdrop |
| `trafficLightX` / `trafficLightY` | pixels | Traffic-light origin. Waku uses `(16, 17)` |
| `transparent` | boolean | Same as `windowBackground: "transparent"` when that option is unset |
Call it again after a save and it remounts the tree on the same window.

Use **`render()`**, not `createRenderer()`, in the app entry. `bun --hot`
re-runs the whole file on save. `createRenderer()` plus `init()` would then
build a second host. `render()` is idempotent: the first call owns the window,
later calls only remount React.

`createRenderer()`, `createRoot()`, and `startFrameLoop()` stay public for
tests and custom hosts. Pass `{ renderer }` into `render()` when you already
have one.

## Debug frame overlay

GPUI paints frame-time stats into the window after layout. The overlay is not
a React element. A React FPS label would update every frame and cause more work.

```tsx
render(<App />, { title: 'My App', debugFrameOverlay: 'full' })
```

| Mode | What you see |
|---|---|
| `hidden` | nothing (default) |
| `minimal` | last draw time, e.g. `8.3 MS` |
| `full` | `CUR`, `1%`, `10%`, `MAX`, `FRAMES` |

Or call the renderer:

```ts
renderer.setDebugFrameOverlay('full')
renderer.cycleDebugFrameOverlay()
renderer.resetDebugFrameOverlayStats()
renderer.getDebugFrameOverlay() // 'hidden' | 'minimal' | 'full'
```

The overlay shows **draw time**, not FPS. `8.3 MS` is about 120 Hz.

## Hot reload

### 1. End the file with `render()`

```tsx
import { render } from '@gpuix/react'

function App() {
  return <div style={{ padding: 16 }}>hello</div>
}

render(<App />, { title: 'My App', width: 800, height: 600 })
```

Do **not** call `createRenderer()` or `init()` in this file. `bun --hot` re-runs
the whole entry on save. A second `init()` would open a second window.

### 2. Start the app with `bun --hot`

Prefer **`bun --hot`** over a plain `bun` or `tsx` run. Without `--hot`, a
save starts a second process. With it, `render()` remounts React on the same
window.

```bash
bun --hot app.tsx
cd examples && bun --hot chat.tsx
```

### 3. Save the file

```
save .tsx  ►  bun re-evaluates the entry  ►  render() remounts React
                     │
                     ▼
              GpuixRenderer, window, GPU stay
```

The first `render()` creates the native host and stores it on `globalThis`.
Each save unmounts the React tree and mounts a new one on that same host.

**Stays:** window, GPU device, native `.node` addon, GPUI scroll physics.

**Resets:** `useState`, focus, React event handlers.

This is a remount, not React Refresh. Keeping hook state needs Bun to inject
`$RefreshReg$` during `--hot`. That transform exists on
`bun build --react-fast-refresh` only. Tracked in
[oven-sh/bun#40179](https://github.com/oven-sh/bun/issues/40179).

Native `.node` edits still need a rebuild. See [Developing the Rust side](#developing-the-rust-side).

On **macOS**, `startFrameLoop` calls `renderer.tick()` at a fixed rate (~125fps by
default). This pumps AppKit on the process main thread without blocking Node. Pass
`{ frameMs }` to change the rate, and call `.stop()` on the returned handle to end it.

On **Windows and Linux**, GPUI runs its normal blocking native event loop on one
dedicated Rust UI thread. Node sends in-process commands to that thread, so
`startFrameLoop` returns a no-op handle and does not create a JavaScript timer.
All platforms use GPUI's native platform, window, renderer, input, scroll,
clipboard, keyboard, and IME implementations. The embedded macOS run-loop
extension comes from the pinned GPUIX fork. Windows runtime validation is pending.

> [!IMPORTANT]
> On macOS, never drive `tick()` from a `setImmediate` loop. That spins at tens of thousands of
> ticks per second and burns **73% CPU on a completely idle app**, versus **1%** when
> paced.

## Native animations

Use **`motion.div`** to animate from an initial style to a target style. React
sends the target once. Rust calculates intermediate values and requests GPUI
frames until the transition finishes, without a React render or N-API call for
each frame.

### Animate a target

```tsx
import { motion } from '@gpuix/react'

function WelcomeCard() {
  return (
    <motion.div
      initial={{ width: 0, opacity: 0 }}
      animate={{ width: 320, opacity: 1 }}
      transition={{ duration: 0.25, ease: 'easeOut' }}
      style={{ overflow: 'hidden' }}
    >
      <text style={{ color: '#ffffff' }}>Welcome</text>
    </motion.div>
  )
}
```

Set **`initial={false}`** when the element must mount at its first `animate`
target. Later `animate` changes still transition normally. If a target changes
while motion is active, the next transition starts from the current visible
value, so reversing an animation does not jump.

### Targets and timing

Motion currently accepts these **numeric targets**:

| Target | Range or unit |
|---|---|
| `width`, `height` | pixels, zero or greater |
| `top`, `right`, `bottom`, `left` | pixels |
| `opacity` | `0` through `1` |
| `borderRadius` | pixels, zero or greater |

The **transition** uses seconds, like Motion for React:

| Option | Default | Values |
|---|---:|---|
| `duration` | `0.3` | Non-negative seconds |
| `delay` | `0` | Non-negative seconds |
| `ease` | `"easeOut"` | `"linear"`, `"ease"`, `"easeIn"`, `"easeOut"`, `"easeInOut"`, or `[x1, y1, x2, y2]` |

Springs, keyframes, variants, exit transitions, and shared layout animations
are not available yet.

### Animate a sidebar

Animate an **outer clipping container** and keep the inner sidebar at a fixed
width. This reveals or hides the content without reflowing its text on every
frame.

```tsx
import { motion } from '@gpuix/react'
import type { ReactNode } from 'react'

function SidebarFrame({
  collapsed,
  children,
}: {
  collapsed: boolean
  children: ReactNode
}) {
  const sidebarWidth = 252
  const dividerWidth = 1

  return (
    <motion.div
      initial={false}
      animate={{ width: collapsed ? 0 : sidebarWidth + dividerWidth }}
      transition={{ duration: 0.2, ease: 'easeOut' }}
      style={{
        display: 'flex',
        flexDirection: 'row',
        height: '100%',
        flexShrink: 0,
        overflow: 'hidden',
      }}
    >
      <div style={{ width: sidebarWidth, height: '100%', flexShrink: 0 }}>
        {children}
      </div>
      <div style={{ width: dividerWidth, height: '100%', flexShrink: 0 }} />
    </motion.div>
  )
}
```

The **chat example** uses this pattern. The sidebar remains mounted while its
outer width moves between `253` and `0` pixels.

### Capture exact frames

The [automation API](#automation) can freeze the native motion clock and render
specific timestamps. This avoids timer sleeps and gives CI the same frames on
every run.

```tsx
import { connectTest } from '@gpuix/react/automation'
import { createTestRoot } from '@gpuix/react/testing'
import { ChatApp } from './chat'

const { render, renderer } = createTestRoot()
render(<ChatApp />)
const app = await connectTest(renderer)

const startedAt = await app.clock.pause()
await app.getByTestId('sidebar-collapse').click()

await app.captureFrames('review/sidebar', [
  startedAt,
  startedAt + 50,
  startedAt + 100,
  startedAt + 150,
  startedAt + 200,
])

await app.clock.resume()
```

## Scrolling

Containers with `overflow: "scroll"` become natively scrollable. GPUI handles scroll physics, clipping, and offset persistence automatically.

Plain scroll containers still build every child. Use `<virtual-list>` below when the collection can grow large.

> [!IMPORTANT]
> **Nested scrolling is not supported.** One parent may scroll. An inner
> `overflow: "scroll"`, `<virtual-list>`, or `<diff>` must not. GPUI gives both
> hitboxes the same wheel event, so the inner list steals the gesture.
>
> Keep long inner content in that parent. Collapse it behind an **expandable**
> (preview plus Show more) instead of giving the child its own viewport.
>
> Horizontal overflow is the exception. `overflowX: "scroll"` on a wide child
> (a code row, a table) does not steal the vertical wheel. GPUIX lays that
> scroller out as a flex viewport with `minWidth: 0`. The wide child must not
> shrink: set `flexShrink: 0` or a definite width. Swipe on **X** to pan.
> A vertical wheel stays on the parent.

```tsx
function Expandable({
  preview,
  children,
}: {
  preview: React.ReactNode
  children: React.ReactNode
}) {
  const [open, setOpen] = useState(false)
  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 8 }}>
      {open ? children : preview}
      {!open && <div onClick={() => setOpen(true)}>Show more</div>}
    </div>
  )
}
```

```tsx
function ScrollableList() {
  return (
    <div style={{ height: 300, overflow: 'scroll' }}>
      {items.map((item, i) => (
        <div key={i} style={{ height: 60, padding: 12 }}>
          {item.name}
        </div>
      ))}
    </div>
  )
}
```

Per-axis scrolling: use `overflowX: "scroll"` or `overflowY: "scroll"`.

For programmatic scroll control, use a React ref to get the element's numeric ID, then call the renderer's scroll methods:

```tsx
function ProgrammaticScroll() {
  const listRef = useRef<any>(null)

  const jumpToBottom = () => {
    if (listRef.current) {
      renderer.scrollTo(listRef.current.id, 0, -999)
    }
  }

  return (
    <>
      <div ref={listRef} style={{ height: 200, overflow: 'scroll' }}>
        {items.map((item, i) => <div key={i}>{item}</div>)}
      </div>
      <div onClick={jumpToBottom}>Jump to bottom</div>
    </>
  )
}

// Available scroll methods on the renderer:
renderer.scrollTo(elementId, x, y)        // set offset directly
renderer.scrollToItem(elementId, index)   // scroll child into view
renderer.getScrollOffset(elementId)       // returns [x, y] or null
```

## Virtual lists

Use `<virtual-list>` for **long, variable-height collections** such as message lists. React and Rust retain every row, but GPUI only builds, lays out, and paints rows near the viewport.

```tsx
function MessageList({ messages }: { messages: Message[] }) {
  return (
    <virtual-list
      alignment="bottom"
      followTail
      estimatedItemHeight={180}
      style={{ flexGrow: 1, minHeight: 0 }}
    >
      {messages.map((message) => (
        <Message key={message.id} message={message} />
      ))}
    </virtual-list>
  )
}
```

The list needs a **bounded height** or bounded flex space. Its direct children are rows and can contain any GPUIX host or custom element.

| Prop | Default | Purpose |
|---|---:|---|
| `alignment` | `"top"` | Use `"bottom"` for chat-style initial positioning |
| `followTail` | `false` | Follow appended rows until the user scrolls away |
| `overdraw` | `512` | Extra pixels built outside the viewport |
| `estimatedItemHeight` | none | Gives unmeasured rows an initial height estimate |

### How virtualization works

**React reconciliation stays normal.** The complete keyed child list crosses the mutation protocol and remains in Rust's retained tree. GPUIX defers only the expensive GPUI element construction, layout, and paint work.

```text
React Fiber + Rust RetainedTree    all row IDs, props, text, and events
                 │
                 ▼
          GPUI ListState          row count and measured height cache
                 │
                 ▼ visible indexes plus overdraw
          cx.processor            re-enters GpuixView after root render
                 │
                 ▼
          fresh BuildCtx          builds only the requested React subtree
                 │
                 ▼
       GPUI layout and paint      visible rows only
```

GPUI measures a row when it enters the viewport. `estimatedItemHeight` gives unseen rows an approximate height so the scrollbar is useful before every row has been visited. The measured height replaces the estimate automatically.

When a retained descendant changes, GPUIX marks its direct row for remeasurement. Appending, removing, or reordering keyed rows keeps measurements for rows whose IDs did not change.

### Row boundaries

Each **direct host child** is one virtual row. Give every row a stable React key and one host root:

```tsx
<virtual-list style={{ height: 500 }}>
  {messages.map((message) => (
    <div key={message.id} style={{ paddingBottom: 24 }}>
      <Message message={message} />
    </div>
  ))}
</virtual-list>
```

A row can contain nested `<div>`, `<text>`, `<markdown>`, `<code>`, `<diff>`, `<input>`, and `<textarea>` elements. Focusable rows stay active when they move offscreen, so keyboard input and native editor state are preserved. Those children must not scroll. Nested scrolling is not supported; see [Scrolling](#scrolling).

### Chat tail behavior

Combine `alignment="bottom"` and `followTail` for a chat thread:

```tsx
<virtual-list
  alignment="bottom"
  followTail
  estimatedItemHeight={220}
  style={{ flexGrow: 1, minHeight: 0 }}
>
  {turns.map((turn) => (
    <ChatTurn key={turn.id} turn={turn} />
  ))}
</virtual-list>
```

The list follows new rows while the user is at the bottom. Scrolling upward pauses tail following. Returning to the bottom enables it again. A streaming final row is remeasured as its content grows.

### Programmatic scrolling

Use a ref to call the same renderer scroll methods as a plain scroll container:

```tsx
function Results({ rows }: { rows: Result[] }) {
  const renderer = useGpuixRequired()
  const listRef = useRef<{ id: number } | null>(null)

  const reveal = (index: number) => {
    if (listRef.current) {
      renderer.scrollToItem?.(listRef.current.id, index)
    }
  }

  return (
    <>
      <virtual-list ref={listRef} style={{ height: 400 }}>
        {rows.map((row) => (
          <ResultRow key={row.id} row={row} />
        ))}
      </virtual-list>
      <div onClick={() => reveal(rows.length - 1)}>Reveal latest</div>
    </>
  )
}
```

`scrollTo`, `scrollToItem`, and `getScrollOffset` all support virtual lists.

### Performance model

| Work | Plain scroll container | `<virtual-list>` |
|---|---|---|
| React Fiber nodes | All rows | All rows |
| Rust retained nodes | All rows | All rows |
| GPUI row construction | All rows | Visible rows plus overdraw |
| Layout and paint | All rows | Visible rows plus overdraw |
| Height metadata | None | One lightweight entry per row |

Virtualization removes the **per-render GPUI cost**, not the memory used by React and the retained tree. For normal chat histories this is the useful tradeoff. Collections with millions of rows still need application-level paging or a data-owning native element.

### Keep scroll fast

A wheel event notifies the window view. GPUI then rebuilds the **visible**
rows and Taffy lays them out again. Draw time is the cost of those rows, not
the length of the list.

Put a long list on `<virtual-list>`. Keep `overdraw` near one extra
viewport. Put fat content in one native node (`<markdown>`, `<code>`, `<diff>`),
not a tree of React spans.

The **first mount** still creates every React child and every retained node.
A 10,000-row list hitches once while those nodes cross FFI. After that
a wheel only rebuilds the visible rows. Collections that large need
application-level paging if the first paint must stay instant.

`overflowX: "scroll"` on a wide child must not steal the vertical wheel.
GPUIX sets `restrict_scroll_to_axis` on that path. Native
`overflow_x_scroll()` must call the same method.

Turn on `debugFrameOverlay: 'full'` while you scroll. The overlay is **draw
time**. `8.3 MS` is about 120 Hz.

## Text input

`<input>` and `<textarea>` use GPUI's platform input handler. They support a
native caret, text selection, IME composition, clipboard actions, undo/redo,
grapheme-safe deletion and mouse positioning.

```tsx
<textarea
  value={draft}
  placeholder="Ask anything"
  minRows={1}
  maxRows={8}
  onChange={(event) => setDraft(event.value ?? '')}
  onSubmit={send}
/>
```

`Enter` emits `onSubmit`. In a `<textarea>`, `Shift+Enter` inserts a newline.
The editor updates natively first, then reports the complete value to React.
`value` changes can replace the native content, but keeping the same prop value
does not reject an edit like a browser-controlled input.

The focused caret stays solid during edits and then blinks every 500ms while
idle. It stops scheduling repaint frames on blur or while the window is
inactive. Override its colour through the shared native theme:

```tsx
<input theme={{ caret: '#22c55e' }} />
```

## Focus and keyboard navigation

Focus is a **native GPUI concept**. GPUIX connects stable React element IDs to
persistent `gpui::FocusHandle` values, so focus survives React rerenders:

```text
React <div tabIndex={0}>
            │
            ▼
Retained element ID ► persistent gpui::FocusHandle ► keyboard/action dispatch
            ▲
            │
      React rerenders
```

Inputs and textareas join the normal tab order automatically. Add `tabIndex` to
a `div` when it should receive keyboard focus:

```tsx
<div
  tabIndex={0}
  onFocus={() => setActive(true)}
  onBlur={() => setActive(false)}
  onKeyDown={(event) => {
    if (event.key === 'enter') submit()
  }}
>
  Submit
</div>
```

| Prop | Behavior |
|---|---|
| `tabIndex={0}` | Joins the normal Tab order |
| `tabIndex={n}` | Uses `n` as its GPUI tab-order index |
| `tabIndex={-1}` | Skipped by Tab, but focusable by click or renderer API |
| `autoFocus` | Takes focus once, when its native focus handle is created |

`Tab` calls GPUI's `window.focus_next()`. `Shift+Tab` calls
`window.focus_prev()`. This navigation stays in Rust and does not make a
JavaScript round trip.

Use a ref for imperative focus:

```tsx
const buttonRef = useRef<{ id: number }>(null)

function focusButton() {
  if (buttonRef.current) renderer.focusElement(buttonRef.current.id)
}

<div ref={buttonRef} tabIndex={-1}>Focused on demand</div>
```

Adding `onKeyDown`, `onKeyUp`, `onFocus`, or `onBlur` creates a persistent focus
handle. Add `tabIndex` as well when the element must be reachable with Tab.
Removing `tabIndex` removes the element from the tab order.

## Headless controls

The built-in controls are **unstyled primitives**, not a fixed component
library. Use them like Radix primitives in shadcn: import a primitive namespace,
wrap and style it in a local file, then import those local components throughout
the app.

```text
@gpuix/react/select ► components/ui/select.tsx ► application screens
  native behavior       local styles/variants       product-specific use
```

Each primitive has a dedicated namespace entry point:

| Import | Main parts |
|---|---|
| `@gpuix/react/select` | `Root`, `Trigger`, `Value`, `Content`, `Item` |
| `@gpuix/react/combobox` | `Root`, `Input`, `Content`, `List`, `Item`, `Empty` |
| `@gpuix/react/tooltip` | `Provider`, `Root`, `Trigger`, `Content` |

### Build a local Select

Create `components/ui/select.tsx`. This file is application code, so it can be
copied and changed without waiting for GPUIX to add a theme option:

```tsx
import * as React from 'react'
import * as SelectPrimitive from '@gpuix/react/select'

export const Select = SelectPrimitive.Root
export const SelectValue = SelectPrimitive.Value
export const SelectGroup = SelectPrimitive.Group

export const SelectTrigger = React.forwardRef<
  React.ElementRef<typeof SelectPrimitive.Trigger>,
  SelectPrimitive.SelectTriggerProps
>(({ style, ...props }, ref) => (
  <SelectPrimitive.Trigger
    ref={ref}
    {...props}
    style={(state) => ({
      width: 220,
      height: 36,
      padding: 8,
      backgroundColor: state.open ? '#334155' : '#1e293b',
      borderRadius: 8,
      ...(typeof style === 'function' ? style(state) : style),
    })}
  />
))

export const SelectContent = React.forwardRef<
  React.ElementRef<typeof SelectPrimitive.Content>,
  SelectPrimitive.SelectContentProps
>(({ style, ...props }, ref) => (
  <SelectPrimitive.Content
    ref={ref}
    sideOffset={6}
    {...props}
    style={{
      width: 220,
      maxHeight: 240,
      overflowY: 'scroll',
      padding: 4,
      backgroundColor: '#0f172a',
      borderRadius: 8,
      ...style,
    }}
  />
))

export const SelectItem = React.forwardRef<
  React.ElementRef<typeof SelectPrimitive.Item>,
  SelectPrimitive.SelectItemProps
>(({ style, ...props }, ref) => (
  <SelectPrimitive.Item
    ref={ref}
    {...props}
    style={(state) => ({
      padding: 8,
      opacity: state.disabled ? 0.4 : 1,
      backgroundColor: state.highlighted
        ? '#334155'
        : state.selected
          ? '#1e3a5f'
          : '#0f172a',
      ...(typeof style === 'function' ? style(state) : style),
    })}
  />
))
```

Use the styled local file with the familiar shadcn shape:

```tsx
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from './components/ui/select'

<Select value={model} onValueChange={setModel}>
  <SelectTrigger>
    <SelectValue placeholder="Select a model" />
  </SelectTrigger>
  <SelectContent>
    <SelectGroup>
      <SelectItem value="sonnet">Sonnet</SelectItem>
      <SelectItem value="opus">Opus</SelectItem>
    </SelectGroup>
  </SelectContent>
</Select>
```

The trigger participates in normal tab navigation. Opening the Select focuses
its content. `Up`, `Down`, `Ctrl+P`, `Ctrl+N`, `Enter`, and `Escape` control the
menu. Closing it restores focus to the trigger. Disabled items are skipped.

### Style Combobox and Tooltip the same way

Start their local files from namespace imports too:

```tsx
// components/ui/combobox.tsx
import * as ComboboxPrimitive from '@gpuix/react/combobox'

// components/ui/tooltip.tsx
import * as TooltipPrimitive from '@gpuix/react/tooltip'
```

The application still uses compound components, not one large configuration
object:

```tsx
<ComboboxPrimitive.Root items={['Next.js', 'SvelteKit', 'Astro']}>
  <ComboboxPrimitive.Input style={{ width: 220, height: 36, padding: 8 }} />
  <ComboboxPrimitive.Content style={{ width: 220 }}>
    <ComboboxPrimitive.Empty>No frameworks found.</ComboboxPrimitive.Empty>
    <ComboboxPrimitive.List>
      {(item) => (
        <ComboboxPrimitive.Item key={item} value={item}>
          {item}
        </ComboboxPrimitive.Item>
      )}
    </ComboboxPrimitive.List>
  </ComboboxPrimitive.Content>
</ComboboxPrimitive.Root>
```

```tsx
<TooltipPrimitive.Provider delayDuration={350}>
  <TooltipPrimitive.Root>
    <TooltipPrimitive.Trigger asChild>
      <div tabIndex={0} style={{ padding: 8 }}>Copy</div>
    </TooltipPrimitive.Trigger>
    <TooltipPrimitive.Content side="top" sideOffset={6}>
      Copy message
    </TooltipPrimitive.Content>
  </TooltipPrimitive.Root>
</TooltipPrimitive.Provider>
```

Combobox uses the native input for text editing, IME, clipboard, and focus.
Tooltip `asChild` preserves the child ref and merges trigger behavior into that
host element. All floating content uses GPUI's deferred `anchored()` layer,
snaps inside the window, and occludes controls behind it.

### Overlay menus

Menus, tooltips, and dialogs must use **`SelectContent`**, **`ComboboxContent`**,
or `<anchored deferred>`. Those paint in a later pass, on top of
`<virtual-list>` and the rest of the page.

A `position: "absolute"` card that overflows out of the composer sits **under**
the virtual list. The list paints after the composer, so you still see the
markdown through the menu, and clicks hit the text behind it.

```tsx
<Select value={model} onValueChange={setModel}>
  <div style={{ position: 'relative' }}>
    <SelectTrigger>
      <SelectValue />
    </SelectTrigger>
    <SelectContent side="top" sideOffset={4} style={{ backgroundColor: '#232323' }}>
      <SelectItem value="flash">DeepSeek V4 Flash</SelectItem>
    </SelectContent>
  </div>
</Select>
```

Give every overlay an **opaque** fill (`#232323`, not `#23232399`).
`FloatingLayer` defaults to `#1A1A1A`. Item rows should use the same solid
color, or a solid hover color. A `#00000000` child on a blurred window punches
through Metal to the desktop.

A filled in-flow `div` blocks clicks and hovers behind it. The parent
scroller still gets the wheel. `position: "absolute"` / `"fixed"` or
`pointerEvents: "auto"` also steals the wheel. Set `pointerEvents: "none"`
to pass hits through.

## Text selection

Every text GPUIX paints is **selectable and copyable**, including text inside
`<code>`, `<diff>` and `<markdown>`. A drag that starts in a heading and ends
inside a fenced code block selects everything between; Cmd+C copies it joined in
document order.

There is nothing to opt into. To opt *out* — toolbars, buttons, line-number
gutters — set `userSelect: "none"`, which inherits like the CSS property:

```tsx
<div style={{ userSelect: 'none' }}>
  <text>toolbar label, never selected</text>
</div>
```

![Text selected across markdown blocks](docs/images/selection.png)

Read the selection from the renderer:

```tsx
renderer.getSelectedText()   // joined text, or null
renderer.clearSelection()
```

Selection works because each painted text element registers itself into a
per-frame registry in **paint order**, which is document order. A drag anchored
in one element resolves against that registry into per-element spans: partial in
the anchor and head, whole for everything between.

<details>
<summary>Why not one big text element, like Zed?</summary>

Zed's markdown selects continuously because its whole document is a single
element over one text model. GPUIX renders a *tree* of text elements, so it
rebuilds that continuity at paint time instead. The mechanism is ported from
[Comet](https://github.com/zeronsh/comet) (MIT), which faced the same problem.
</details>

## Native text components

Three elements render text with Tree-sitter syntax highlighting computed in
Rust. Colours come from a theme prop, so a late-arriving highlight recolours runs
without ever changing layout.

### `<code>`

A syntax-highlighted code block. One row per line at an exact line height, so the
block's height is known before highlighting runs.

```tsx
<code
  code={source}
  language="typescript"        // or path="src/app.ts" to detect from extension
  showLineNumbers
  showHeader={false}
/>
```

![A syntax-highlighted code block](docs/images/code.png)

### `<diff>`

A unified diff viewer. It **flows** with its parent by default, so a parent
list can be the only scroller. Collapsing a file removes its rows rather than hiding
them, so a collapsed 10k-line file costs one row.

Use `maxLines` to keep a long patch short. Show more fires `onShowMore`. Clear
`maxLines` in that handler to reveal the rest.

Pass `scroll` and a **bounded height** only for a dedicated full-window viewer.
That path uses GPUI's `list()` and virtualizes. Do not nest it inside another
scroller. See [Scrolling](#scrolling).

```tsx
<diff
  patch={unifiedPatch}
  wordDiff                     // highlight only the tokens that changed
  maxLines={open ? undefined : 24}
  collapsedPaths={['pnpm-lock.yaml']}
  onShowMore={() => setOpen(true)}
  onToggleFile={(e) => toggle(e.value)}
  onLineClick={(e) => console.log(e.oldLine, e.newLine, e.value)}
/>
```

![A unified diff with word-level highlights](docs/images/diff.png)

### `<markdown>`

GitHub-flavoured markdown: headings, lists, tables, block quotes, fenced code,
strikethrough, task lists, and autolinked bare URLs.

```tsx
<markdown source={readme} onLinkClick={(e) => open(e.value)} />
```

![Markdown with headings, lists, a table and a code fence](docs/images/markdown.png)

### Theming

All three take the same optional `theme` prop. Every field layers on top of the
built-in dark theme, so overriding one token leaves the rest alone.

```tsx
<code
  code={source}
  language="rust"
  theme={{
    appearance: 'dark',        // or 'light'
    accent: '#7c86ff',
    syntax: { keyword: '#f38ba8', string: '#a6e3a1' },
  }}
/>
```

**Layout numbers live in the theme too**, under `metrics`. Row heights, gutter
widths, paddings and the heading scale are props, not Rust constants, so tuning
the design is a React re-render and never a native rebuild.

```tsx
<diff
  patch={patch}
  theme={{
    metrics: {
      diffLineHeight: 26,
      diffGutterWidth: 48,
      mdHeadingSizes: [24, 19, 16, 14],
    },
  }}
/>
```

When `scroll` is on, `<diff>` virtualizes from these numbers without measuring,
so changing `diffLineHeight` also re-sizes the scroll model.

The same three components, retuned entirely from `metrics` with no rebuild:

![The components with enlarged metrics](docs/images/metrics.png)

Languages bundled: Rust, TypeScript, TSX, JavaScript, JSX, Python, Go, JSON,
Bash, TOML, YAML, Markdown, HTML, CSS, C.

## Supported Elements

| Element         | Description                                      |
|-----------------|--------------------------------------------------|
| `div`           | Container with flexbox layout                    |
| `text`          | Text content, selectable                         |
| `code`          | Syntax-highlighted code block                    |
| `diff`          | Unified diff viewer. Flows by default            |
| `markdown`      | GitHub-flavoured markdown                        |
| `input`         | Native single-line text editor                   |
| `textarea`      | Native multiline, auto-growing text editor       |
| `virtual-list`  | Long collections; only visible rows are built    |
| `img`           | Local raster or SVG images                       |
| `svg`           | Tintable monochrome SVG icons from local files   |
| `anchored`      | Positioned overlay                               |
| `canvas`        | Custom drawing (planned)                         |

## Images and icons

Both elements take a **filesystem path**, not a URL. Resolve the file with
`fileURLToPath` or `path.join` and pass that string as `src`.

### `<img>`

`<img>` paints through GPUI's image element. It loads **PNG, JPEG, WebP, GIF,
and SVG** from disk. SVG here is a full-colour image, not a tintable icon.

```tsx
<img
  src={fileURLToPath(new URL('./photo.png', import.meta.url))}
  objectFit="cover"
  style={{ width: 240, height: 140, borderRadius: 12 }}
/>
```

`objectFit` matches CSS: `"contain"` (default), `"cover"`, `"fill"`,
`"scaleDown"`, or `"none"`. An empty `src` or a failed load shows a fallback
placeholder instead of crashing.

### `<svg>`

`<svg>` uses GPUI's **monochrome icon renderer**. The file is drawn as a single
shape and tinted with `style.color`. Use this for toolbar icons, not for
full-colour artwork.

`src` is a filesystem path **or** a `data:image/svg+xml,…` URL. Vitest and some
Bun `import … with { type: 'file' }` bindings emit the data URL. GPUIX decodes
both.

`style.color` is required. Without it the icon does not paint. Prefer
`fill="#000"` or `stroke="#000"` in the file. `currentColor` in the SVG is not
the same as `style.color`.

```tsx
<svg
  src={fileURLToPath(new URL('./assets/icons/search.svg', import.meta.url))}
  style={{ width: 16, height: 16, color: '#b4b4b4' }}
/>
```

The chat example builds every sidebar and composer icon this way.

## Supported Events

| Event | Props | Payload fields |
|-------|-------|----------------|
| Click | `onClick` | `x`, `y`, `clickCount`, `isRightClick`, `modifiers` |
| Mouse down | `onMouseDown` | `x`, `y`, `button`, `clickCount`, `modifiers` |
| Mouse up | `onMouseUp` | `x`, `y`, `button`, `clickCount`, `modifiers` |
| Mouse enter | `onMouseEnter` | `hovered` |
| Mouse leave | `onMouseLeave` | `hovered` |
| Mouse move | `onMouseMove` | `x`, `y`, `pressedButton`, `modifiers` |
| Click outside | `onMouseDownOutside` | `x`, `y`, `button`, `modifiers` |
| Key down | `onKeyDown` | `key`, `keyChar`, `isHeld`, `modifiers` |
| Key up | `onKeyUp` | `key`, `keyChar`, `modifiers` |
| Focus | `onFocus` | — |
| Blur | `onBlur` | — |
| Scroll | `onScroll` | `deltaX`, `deltaY`, `precise`, `touchPhase`, `modifiers` |
| Change | `onChange` | `value` — `<input>` and `<textarea>` only |
| Submit | `onSubmit` | `value` — `<input>` and `<textarea>` only |
| Toggle file | `onToggleFile` | `value` (file path) — `<diff>` only |
| Show more | `onShowMore` | `value` (hidden line count) — `<diff>` only |
| Line click | `onLineClick` | `value`, `oldLine`, `newLine` — `<diff>` only |
| Link click | `onLinkClick` | `value` (URL) — `<markdown>` only |

Keyboard and focus listeners create a persistent GPUI `FocusHandle`
automatically. A listener alone does not put a `div` in the Tab order; add
`tabIndex={0}` for that. Inputs and textareas already use tab index `0`.

## Supported Styles

CSS-like styling via the `style` prop:

```tsx
<div style={{
  display: 'flex',
  flexDirection: 'column',
  gap: 8,
  padding: 16,
  backgroundColor: '#3b82f6',
  borderRadius: 8,
}}>
  <div style={{ color: '#ffffff', fontSize: 18 }}>
    Hello GPUI!
  </div>
</div>
```

**Layout:** `display` (`"flex"` | `"grid"`), `flexDirection`, `flexWrap`, `flexGrow`, `flexShrink`, `flexBasis`, `alignItems`, `alignSelf`, `alignContent`, `justifyContent`, `gap`, `rowGap`, `columnGap`, `gridTemplateColumns`, `gridTemplateRows`, `gridColumnMin`, `gridRowMin`

**Sizing:** `width`, `height`, `minWidth`, `minHeight`, `maxWidth`, `maxHeight` — accepts pixels (number) or percentages (string like `"100%"`)

**Spacing:** `padding`, `paddingTop/Right/Bottom/Left`, `margin`, `marginTop/Right/Bottom/Left`

**Position:** `position` (`"relative"` | `"absolute"`), `top`, `right`, `bottom`, `left`

**Visual:** `backgroundColor`, `color`, `opacity`, `cursor`, `pointerEvents`, `borderRadius`, `borderTopLeftRadius`, `borderTopRightRadius`, `borderBottomLeftRadius`, `borderBottomRightRadius`, `borderWidth`, `borderTopWidth`, `borderRightWidth`, `borderBottomWidth`, `borderLeftWidth`, `borderColor`, `boxShadow`

`boxShadow` accepts one structured shadow. Its fields are `offsetX`, `offsetY`,
`blurRadius`, `spreadRadius`, and `color`:

```tsx
<div
  style={{
    boxShadow: {
      offsetX: 0,
      offsetY: 4,
      blurRadius: 12,
      spreadRadius: 0,
      color: '#00000033',
    },
  }}
/>
```

**Overflow:** `overflow`, `overflowX`, `overflowY` — `"hidden"` clips content, `"scroll"` creates a native scrollable container with persistent scroll state

**Text:** `fontSize`, `fontFamily`, `fontWeight`, `textAlign`, `lineHeight`, `whiteSpace`, `textOverflow`, `lineClamp`

**Selection:** `userSelect` (`"text"` | `"none"`), `selectionColor` — both inherit down the tree

### Hover and active

`hover` and `active` are **nested style objects**. GPUI applies them natively
when the pointer is over the element or the mouse is down. There is no
JavaScript round trip.

```tsx
<div
  style={{
    backgroundColor: '#313244',
    borderRadius: 8,
    padding: 12,
    hover: { backgroundColor: '#45475a' },
    active: { backgroundColor: '#585b70' },
  }}
>
  Press
</div>
```

Nesting is one level deep. A `hover` object cannot contain another `hover` or
`active`.

> **Note: `white-space: pre` is not supported.** GPUI's text system only has `normal` (wraps) and `nowrap` (single line). To preserve newlines like HTML `<pre>`, split your text on `\n` in React and render each line as a separate `<text>` element in a flex column:
>
> ```tsx
> <div style={{ display: 'flex', flexDirection: 'column', fontFamily: 'Menlo' }}>
>   {code.split('\n').map((line, i) => (
>     <text key={i} style={{ whiteSpace: 'nowrap' }}>{line}</text>
>   ))}
> </div>
> ```

> **Note: GPUI defaults text color to black, not white.** Unlike CSS, GPUI does not inherit `color` from parent elements. Every `<text>` element that doesn't set an explicit `color` style will render as black — invisible on dark backgrounds. Always set `color` on your text elements or on a parent `<div>` (which applies `text_color` to all children in that subtree via GPUI's `Styled` trait).

## Automation

Mark elements with **`testId`**, then drive them like Playwright. The same
client works in vitest and against a child process.

```tsx
<div testId="sidebar-collapse" onClick={onCollapse}>‹</div>
<textarea testId="composer" value={draft} onChange={...} />
<div testId="send" onClick={onSend}>↑</div>
```

```ts
import { createTestRoot } from '@gpuix/react'
import { connectTest } from '@gpuix/react/automation'
import { ChatApp } from './chat'

const { render, renderer } = createTestRoot()
render(<ChatApp />)
const app = await connectTest(renderer)

await app.screenshot({ path: 'open.png' })

await app.clock.pause()
await app.getByTestId('sidebar-collapse').click()
await app.clock.fastForward(200)
await app.screenshot({ path: 'collapsed.png' })

await app.getByTestId('composer').fill('hello gpuix')
await app.getByTestId('send').click()
await app.screenshot({ path: 'sent.png' })
```

That is the chat example. The real test lives in
[`examples/chat.test.tsx`](./examples/chat.test.tsx).

```
createTestRoot()                 launch({ command, args })
       │                                    │
       ▼                                    ▼
 connectTest(renderer)              child stdin / stdout
       │                                    │
       └────────── App / Locator ───────────┘
                    click, fill, screenshot
```

### Locators

| Call | Matches |
|---|---|
| `app.getByTestId('send')` | The `testId` prop |
| `app.getByText('New chat')` | A node's own text |
| `app.getByType('textarea')` | The host element type |
| `locator.getByText('...')` | A descendant of another locator |

`click()` hits the center of the last painted bounds. `fill(text)` replaces the
focused editor contents. `press('enter')` sends one key. `waitFor()` polls until
exactly one match exists.

### Screenshots and clock

`app.screenshot({ path })` writes the current GPU frame as a PNG.

`app.clock.pause()`, `set(ms)`, and `fastForward(ms)` freeze native motion time.
Use that to capture a sidebar animation at known timestamps:

```ts
const startedAt = await app.clock.pause()
await app.getByTestId('sidebar-collapse').click()
await app.captureFrames('review/sidebar', [
  startedAt,
  startedAt + 100,
  startedAt + 200,
])
```

### Live apps

`launch({ command, args })` starts the app and speaks the same commands
over stdin as SSE `data:` lines. The app listens only when stdin is a **pipe**,
so a normal terminal run is unchanged. Lines without a `data:` prefix are
ignored; `console.log` cannot break a message.

```ts
import { launch } from '@gpuix/react/automation'

const app = await launch({ command: 'bun', args: ['examples/chat.tsx'] })
await app.getByTestId('composer').fill('hello')
await app.screenshot({ path: 'live.png' })
await app.close()
```

## Testing

The locators above sit on a **GPU-backed test renderer** (`TestGpuixRenderer`).
It runs the same `GpuixView`, `build_element()`, `apply_styles()`, and event
handlers as production. Windows are positioned offscreen but fully rendered by
Metal. The methods below are the lower-level API when a locator is not enough.

```ts
import { createTestRoot } from '@gpuix/react/testing'

const { root, renderer } = createTestRoot()

root.render(<MyComponent />)
renderer.flush()  // triggers GpuixView::render() via Metal

// Simulate events through GPUI's native input pipeline
renderer.nativeSimulateClick(50, 50)
renderer.nativeSimulateKeystrokes('enter')

// Inspect results
const events = renderer.drainNativeEvents()
const screenshot = renderer.captureScreenshot('/tmp/test.png')
const text = renderer.getAllText()
```

### Testing native elements

`getAllText()` only sees `<text>` nodes in the retained tree. `<code>`, `<diff>`
and `<markdown>` paint their text inside GPUI, so use `getPaintedText()`, which
returns every string painted in the last frame in paint order:

```ts
root.render(<code code={'a\nb'} language="ts" showHeader={false} />)
expect(renderer.getPaintedText()).toEqual(['a', 'b'])
```

Selection has its own helper. Listeners are registered during **paint**, so
`dragSelect` flushes between every step; calling `simulateMouseDown` / `Move` /
`Up` by hand without those flushes selects nothing:

```ts
expect(renderer.dragSelect(20, 30, 900, 300)).toBe('first line\nsecond line')
```

Screenshots land in `packages/react/screenshots/` and `examples/screenshots/`,
both gitignored, so they can be inspected after a run without adding a binary
diff to every commit. The curated set the README links to lives in
`docs/images/` and is regenerated with:

```bash
bun scripts/screenshots.ts
```

## Developing the Rust side

JS remount is covered above. There is **no hot reload for the native half**,
and there cannot be: `require()` of a `.node` file calls `process.dlopen`, Node
has no matching unload, and the live state (GPUI's platform, GPU device, open
window, UI thread, and selection registry) stays inside the loaded library. A
second load would create independent native state while the first library
remains loaded.

The rebuild is fast enough that it does not matter. Measured on an M-series Mac
after touching one file:

| Step | Time |
|---|---|
| `cargo check --lib` | 1.5s |
| `cargo build --lib` | 4.9s |
| `bun run build:debug` (napi) | ~2s |
| One vitest screenshot file | ~2s |

`bun run dev` wires that into a loop: it watches `packages/native/src`,
rebuilds, and re-renders the screenshot tests. **Rust edit to fresh PNGs is
about 4 seconds.**

```bash
bun run dev                      # rebuild, re-render the showcase screenshots
bun scripts/dev.ts --shots diff  # only tests matching "diff"
bun scripts/dev.ts --app native-text   # rebuild, restart an example app
```

Screenshot mode is the better default. Open
`packages/react/screenshots/showcase.png` in Preview.app, which reloads on
write, and unlike a live window the PNG can also be read by an agent.

Two things avoid the rebuild entirely:

- **Content** already lives in props. Change `patch` or `source` and the next
  frame shows it.
- **Design numbers** live in `theme.metrics`. Tuning a row height or heading
  scale is a React re-render.

The test renderer uses `VisualTestAppContext` with a `TestDispatcher` for deterministic scheduling. Event simulation goes through GPUI's coordinate-based hit testing and dispatch — not synthetic JS events.

## Status

- [x] React reconciler with mutation-based protocol
- [x] napi-rs FFI bindings (createElement, appendChild, setStyle, etc.)
- [x] RetainedTree (Rust-side element storage)
- [x] Style mapping (CSS properties → GPUI style methods)
- [x] Mouse events (click, mouseDown, mouseUp, mouseMove, mouseEnter, mouseLeave)
- [x] Click outside (`onMouseDownOutside`)
- [x] Scroll wheel events with delta and touch phase
- [x] Scrollable containers (`overflow: "scroll"`) with persistent scroll state
- [x] Programmatic scroll API (`scrollTo`, `scrollToItem`, `getScrollOffset`)
- [x] Keyboard events (keyDown, keyUp) with focus management
- [x] Focus/blur events with automatic FocusHandle creation
- [x] GPU-backed test renderer with screenshot capture
- [x] Standalone build (pinned GPUI platform dependencies)
- [x] Native text input and multiline textarea
- [x] Image and SVG elements (`<img>`, `<svg>`)
- [x] Virtual lists (`<virtual-list>`)
- [x] Native text components (`<code>`, `<diff>`, `<markdown>`)
- [x] Cross-element text selection
- [x] Headless Select, Combobox, and Tooltip
- [x] Native `hover` and `active` styles
- [x] Window title (`setWindowTitle`)
- [x] Window chrome (`titlebarTransparent`, `windowBackground`, traffic-light position)
- [x] Last window close quits the process
- [x] Debug frame overlay (`debugFrameOverlay` / `setDebugFrameOverlay`)
- [ ] Canvas element
- [ ] Multiple windows
- [x] JS remount under `bun --hot` (`render()` keeps the native window)
- [ ] React Refresh during `bun --hot` (needs a Bun runtime transform)
- [ ] Hot reload of the native `.node` addon. `bun run dev` rebuilds and restarts. Native modules cannot unload.
- [x] Native `motion.div` transitions with deterministic frame capture

## Documentation

See [AGENTS.md](./AGENTS.md) for detailed architecture, communication flow, and contributing guide.

## License

Apache-2.0
