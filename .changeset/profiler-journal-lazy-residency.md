---
'@gpuix/native': patch
---

The foreground profiler journal's 4 MiB ring is allocated zeroed, so it is resident only as far as entries are recorded rather than written up front when the app starts. Nothing reads the journal outside the host's hang detection, so an app that does not use that holds about 4 MB less.
