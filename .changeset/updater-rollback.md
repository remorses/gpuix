---
"@gpuix/native": minor
---

Add `rollbackUpdate()` to put back the version the last update replaced.
`downloadAndInstall()` now keeps that bundle at
`<cache>/gpuix/updater/<bundle name>/previous` instead of a temp dir that
vanished with the installing process. Windows
installers cannot be reversed in place, so there the promise rejects saying that the
previous version must be reinstalled.

Fixes #107
