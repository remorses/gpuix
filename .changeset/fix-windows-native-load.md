---
"@gpuix/native": patch
---

Delay-load GPUI's prompt and jump-list DLL imports so Node and Bun can load the Windows native binding.

Fixes #1
