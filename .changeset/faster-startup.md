---
'@gpuix/native': minor
'@gpuix/react': minor
'@gpuix/solid': minor
---

Launch up to 2.6x faster on macOS, and never show an empty window. `render()` opens the window hidden, commits the tree, then presents its first frame synchronously through the new `presentWindow()`. `focus: false` and `show: false` launches keep their order.

Startup also stops paying for what it may not use. The HTTP client is built on the first remote image, the macOS menu bar is installed on the first tick, and the automation client, with `zod`, is imported only when automation starts.

Automation now listens only when stdin is a pipe or a file, as documented. A `/dev/null` stdin, which is what Finder and `launchd` give an app, no longer starts it.

`installBrowserAutomation()` now returns a `Promise`, so `globalThis.gpuix` appears shortly after `render()` in the browser. Concurrent calls share one install, and `resetRender()` in either adapter drops one still in flight.
