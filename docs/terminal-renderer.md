---
title: GPUI terminal renderer
description: >
  How gpui_terminal (packages/gpui-terminal) runs GPUI apps inside a
  kitty-graphics terminal. Frame path, input protocols, research notes from
  terminal-browser, opentui and the kitty spec, and what is left to do.
---

# GPUI terminal renderer

`gpui_terminal` is a GPUI **platform**, like `gpui_web`. The window is the terminal grid.
Each frame renders offscreen on the GPU and is shown with the kitty graphics protocol.

```rust
gpui_terminal::application()?.run(|cx| {
    cx.open_window(WindowOptions::default(), |_, cx| cx.new(|_| MyView)).ok();
});
```

Code: `packages/gpui-terminal`, a standalone crate that only uses public GPUI APIs from
the `zed` submodule. The offscreen renderers it calls (`render_scene_to_pixels`,
`new_offscreen`) are changes in the `remorses/zed` fork. macOS and Linux.

```bash
cd packages/gpui-terminal
cargo test
cargo build --release --example terminal_hello
```

| OS | Renderer | Text |
|---|---|---|
| macOS | `MetalRenderer::new_headless(RGBA8Unorm)` + `render_scene_to_pixels` | CoreText |
| Linux | `WgpuRenderer::new_offscreen` + `render_to_pixels` (Vulkan, GL, or lavapipe) | cosmic-text |

## Frame path

```
GPUI Scene ──► Metal or wgpu offscreen RGBA texture
                                                              │ readback, one copy
                                                              ▼
                                         mmap of POSIX shm "/gpui<pid>.<n>"
                                                              │
  CSI ?2026h  CSI H  ESC_G a=T,i=1,p=1,f=32,t=s,s=W,v=H,C=1,q=2;<b64 name> ESC\  CSI ?2026l
                                                              │ ~100 bytes on the pty
                                                              ▼
                                  terminal maps, reads, unlinks the object
```

- **`t=s` shared memory**: the pty carries only the name. Base64 direct transfer of a
  1600x1000 RGBA frame is 8.5 MB, too slow for 60 fps
- **RGBA render target**: kitty has no BGRA format. Rendering RGBA avoids a CPU swizzle
- **Same `i=1,p=1` every frame**: re-transmitting replaces the image and its placement in
  one command. The 2026 brackets make it atomic, so there is no flicker
- **Leak bound**: we unlink frames older than 4 ourselves, in case the terminal stops reading
- **Demand driven**: frames only render when GPUI invalidates, capped at 60 Hz
- **Cost**: about 4 ms per 1600x960 frame on M-series (render, wait, readback, shm)

## Over SSH: inline frames

The terminal on another machine cannot open our shm. At window open the presenter asks,
the same way kitty's `icat` and yazi do:

```
ESC_G a=q,i=<shm probe>,s=1,v=1,f=24,t=s,S=3;<b64 name of a 3 byte shm object> ESC\
ESC_G a=t,i=<edit probe>,s=1,v=1,f=24,t=d,q=2;AAAA ESC\          1x1 image
ESC_G a=f,i=<edit probe>,r=1,s=1,v=1,X=1,f=24,t=d;AAAA ESC\      edit its root frame
ESC_G a=d,d=I,i=<edit probe>,q=2 ESC\  CSI c

ESC_G i=<shm probe>;OK          → shared memory (same machine)
ESC_G i=<shm probe>;ENOENT:...  → inline
ESC_G i=<edit probe>;OK         → inline frames send only the changed regions
CSI ?62;...c                    → end of replies (they come in order)
nothing in 2 s                  → inline, whole frames
```

The first inline frame, and every frame after a resize, is the full RGBA frame, zlib
level 1, base64, in 4096 byte chunks:

```
CSI ?2026h CSI H  ESC_G a=T,i=1,p=1,f=32,o=z,t=d,s=W,v=H,C=1,q=2,m=1;<chunk> ESC\
                  ESC_G m=1,q=2;<chunk> ESC\ ... ESC_G m=0,q=2;<chunk> ESC\  CSI ?2026l
ESC_G a=q,i=<fence n>,s=1,v=1,f=24,t=d;AAAA ESC\     ← terminal answers OK when it got here
```

When the terminal can edit frames, later frames are diffed against the last one sent and
only the changed regions go out, as edits of the shown frame (kitty 0.20+, Ghostty 1.4+):

```
CSI ?2026h  ESC_G a=f,i=1,r=1,x=X,y=Y,s=W,v=H,X=1,f=32,o=z,t=d,q=2,m=1;<chunk> ESC\
            ESC_G a=f,r=1,m=0,q=2;<chunk> ESC\  ← one edit per changed rect
            ...                          CSI ?2026l  ESC_G a=q,i=<fence n>,... ESC\
```

- **Diff**: 32 px tiles. Changed tiles next to each other in a row of tiles are one rect,
  and a rect below one with the same columns extends it, so a scrolled list is one edit.
  A frame with no change sends nothing
- **`X=1`** overwrites the pixels. The default alpha blends them onto the old frame
- **Continuation chunks repeat `a=f,r=1`**. The spec requires `a=f`. kitty up to at least
  0.42 reads the frame number from the last chunk, so without `r=1` the edit is appended as
  a new animation frame and the screen goes stale
- **Fence gate**: no new frame renders while 2 fences wait for their reply. Without it
  SSH and TCP buffers hold seconds of stale frames and input lags behind. A fence with no
  reply for 2 s stops counting
- **Size**: a full frame is about 120 KB at 1600x960 for `terminal_hello`, about 470 KB
  for a dense 2360x1640 chat. A hover with frame edits is 8 to 30 KB. In the harness, 120
  mouse moves sent 0.98 MB with edits and 9.1 MB without
- **The write blocks the main thread** while the link drains. Fine for SSH, a writer
  thread would keep input responsive on very slow links
- **Not done**: `f=24`, tmux. Under tmux replies are not routed back to the pane, so the
  probe falls back to inline whole frames

## Input

| Terminal feature | Enable | Becomes |
|---|---|---|
| SGR mouse, any motion, pixel coords | `?1003h ?1006h ?1016h` | `MouseMove/Down/Up`, hover |
| Wheel | same | `ScrollWheel`, one cell per tick |
| Kitty keyboard flags 31 | `CSI >31u` | `KeyDown/KeyUp`, `ModifiersChanged`, text |
| Bracketed paste | `?2004h` | `replace_text_in_range` |
| Focus | `?1004h` | active and hover status |
| In-band resize | `?2048h` + SIGWINCH | `on_resize` |

Output only: OSC 2 title, OSC 22 pointer shape, OSC 52 clipboard write.

Ctrl-c quits only when the app did not handle it. Raw mode turns off the terminal's SIGINT.

### Traps

- **macOS `poll()` returns POLLNVAL for `/dev/tty`**. The input thread uses `select()`
- **macOS refuses `write()` on shm** and caps names at 31 bytes. Always `mmap`
- **Terminals report physical pixels and no DPI**. Scale is 2 when a cell is at least 24
  device px tall. `GPUI_TERMINAL_SCALE` overrides it
- **A press can arrive at a position with no prior move**. The window sends a synthetic
  `MouseMove` first, or GPUI hover goes stale
- **The loop has no Cocoa run loop**. Each frame runs in an autorelease pool, or Metal
  command buffers leak about 2.4 KB per frame
- **wgpu readback rows are padded to 256 bytes**. `render_to_pixels` unpads them into the
  shm slice
- **Linux shm uses `posix_fallocate`**, so a full `/dev/shm` (64 MB in Docker) fails with
  ENOSPC instead of a SIGBUS during the readback write
- **Without `XDG_RUNTIME_DIR`**, the wgpu GL probe prints a Wayland error to the terminal.
  Real sessions set it; a bare Docker shell does not

## GPUIX integration

`render(<App />, { terminal: true })` or `GPUIX_TERMINAL=1`. `terminal_requested()` in
`renderer.rs` reads both.

```
            macOS                                   Linux
renderer.init ─► init_macos                 renderer.init ─► init_threaded
  EmbeddedPlatform::Terminal                  gpuix-ui thread
  TerminalPlatform::new_embedded()            gpui_terminal::application().run()
  run() returns after launch                  blocking loop, like the X11 path
  tick() ─► pump_events()                     tick() reports ui_running
  wake hook ─► setFrameWakeCallback
```

- **macOS reuses the embedded path.** Every macOS napi method already goes through
  `update_window` on the Node thread, so only the platform behind it changes. The
  threaded path would need a runtime branch in about 40 methods
- **Wakes**: the hook fires after every main-thread message, on the first frame request,
  and from the timer thread when a throttled frame becomes due. `pump_events` stops after
  8 ms and wakes itself again, so a self-waking task cannot starve the JS loop
- **ctrl-c**: `input::init_terminal` binds `ctrl-c` to `NoAction` in the text field
  contexts, so the keystroke stays unhandled and the platform quits
- **Signals**: only SIGWINCH is forwarded to the handler installed before ours. Bun's
  SIGTERM handler exits on the spot, before the terminal is restored
- **`process.exit()`**: `atexit` restores the terminal
- **Logs**: fd 1 and 2, when they are the terminal, are `dup2`'d to an unlinked temp file
  and replayed to stderr after restore. Node's libuv reopens a tty on its own descriptor
  when `process.stdout` is first used, so a stream that printed before `init()` bypasses
  this. Bun writes to fd 1 and 2, so it is always captured

## Testing without a terminal window

`tmp/fake-kitty.py` (gitignored) runs a binary in a pty with a fake pixel size, reads every
shm or inline frame into a PNG, answers `a=q` and `CSI c`, applies frame edits, and injects
mouse, keys, resize and ctrl-c. `REMOTE=1` makes it refuse shm, like a terminal on another
machine. `NO_EDITS=1` refuses frame edits like Ghostty 1.3. A frame is counted at each
fence, so the PNGs of both modes must be identical (dedupe repeats in the `NO_EDITS` run).

```bash
uv run --no-project --python 3.13 tmp/fake-kitty.py tmp/out \
  packages/gpui-terminal/target/release/examples/terminal_hello

# Linux, in Docker with mesa lavapipe (needs lld, libfontconfig-dev, mesa-vulkan-drivers)
docker exec gpui-term-linux python3 /harness/fake-kitty.py /harness/out \
  /target/debug/examples/terminal_hello

# Real remote: the app in Docker, the fake terminal on the host. Different /dev/shm,
# so the probe fails for real. docker exec forwards no pixel size (8x16 cell fallback)
uv run --no-project --python 3.13 tmp/fake-kitty.py tmp/out \
  docker exec -it -e XDG_RUNTIME_DIR=/tmp gpui-term-linux /target/debug/examples/terminal_hello
```

**In a real terminal**, `tmp/relay.py` (gitignored) sits between the terminal and the app.
It renames the shm probe object so the terminal answers ENOENT, like over SSH, moves the
mouse on a script, and logs the terminal's replies and the bytes sent. A Ghostty tip build
from the `tip` release runs next to the installed one:

```bash
open -g -n Ghostty.app --args --quit-after-last-window-closed=true \
  --command="/usr/bin/python3 $PWD/tmp/relay.py /tmp/relay.log $PWD/packages/gpui-terminal/target/release/examples/terminal_hello"
```

## Research notes

| Source | What we took |
|---|---|
| [terminal-browser](https://github.com/zenbu-labs/terminal-browser) `pixel-core` | shm then file then inline transport, dirty patches with `a=f`, 2026 sync, 1016 probe |
| [opentui](https://github.com/anomalyco/opentui) | kitty keyboard flags, SGR bit layout, drag tracking, ESC timeout |
| [kitty spec](https://sw.kovidgoyal.net/kitty/graphics-protocol/) | `t=s` lifecycle, id replacement, `q=2`, chunked `t=d`, `a=q` + DA1 support query |
| [kitty icat](https://github.com/kovidgoyal/kitty/blob/master/kittens/icat/detect.go), [yazi](https://github.com/sxyazi/yazi/blob/main/yazi-tty/src/sequence/query.rs), [ratatui-image](https://github.com/ratatui/ratatui-image/blob/master/src/picker/cap_parser.rs) | one pixel `a=q,t=s` probe ended by `CSI c` |
| [mpv vo_kitty](https://github.com/mpv-player/mpv/blob/master/video/out/vo_kitty.c), [awrit](https://github.com/chase/awrit) | per-frame shm, writer thread |

No Rust crate covers both kitty graphics with shm and an input parser that handles APC
replies, SGR pixel mouse and kitty keyboard flag 16. crossterm and termina misparse APC.
The encoder and parser here are hand written, about 700 lines.

| Terminal | `t=s` | `a=f` frame edits | 2048 resize |
|---|---|---|---|
| kitty | yes | yes | yes |
| Ghostty 1.3 | yes | no ([1.4](https://github.com/ghostty-org/ghostty/pull/13943), tested on tip) | yes |
| WezTerm 20240203 | likely broken on macOS (`read()` on shm) | no reply to the probe | no |
| VS Code 1.110+ | no, `t=d` only ([xterm.js#5714](https://github.com/xtermjs/xterm.js/issues/5714)) | no | no |
| Zed | no graphics (alacritty_terminal) | no | no |

VS Code needs `terminal.integrated.enableImages: true` (default false). Without it
xterm.js drops every `ESC_G`, the probe sees only `CSI c`, and frames go inline into
nothing. xterm.js sizes kitty images in CSS pixels, so the right scale there is 1.

## Not done

- **Copy shortcut**: Ghostty takes cmd-c for itself and ctrl-c quits, so a text field has
  no copy key in terminal mode
- **Linux GPUIX run**: `gpuix-native` type-checks on Linux, but no app ran there yet
