---
"@gpuix/native": patch
"@gpuix/react": patch
---

Push `process.env` overrides through to the Rust side under Bun.

Rust reads overrides such as `GPUIX_SCROLLBARS` at paint. Node writes a
`process.env` assignment through to `setenv`, but Bun only updates its JS
snapshot, so a test that set the variable after start had no effect under
`bun test`. The native module now exports `syncEnvVar`, and the test
renderer copies the known overrides across before every frame flush. The
values land in an override map, not in the real environment, because
`setenv` races `getenv` on the dedicated UI thread of Windows and Linux.
