---
title: Bun AOT chat measurements
description: macOS ARM64 measurements of source JIT, standalone JIT, and experimental AOT, with rejected TypeScript experiments.
---

# Bun AOT chat measurements

**AOT helps the 1,000-turn chat.** Compared with standalone JIT, idle footprint falls
from **121.8 to 94.0 MiB**, first paint from **282.6 to 232.6 ms**, and the executable
from **140.9 to 87.3 MiB**. `--trim` makes it **79.0 MiB**.

**The large chat changes the result.** At 10,000 turns, source JIT and AOT have almost
the same footprint after interaction. AOT's automation workload is slower. None of the
four application/runtime TypeScript experiments proved an AOT improvement, so all were removed.

## Results

**Medians of five interleaved runs per variant**, on October 26, 2026. Memory is MiB,
not decimal MB. Footprint is macOS's physical footprint, not virtual address space.

| Turns | Variant | First paint, ms | Peak RSS | Idle footprint | After footprint | Executable |
| ---: | --- | ---: | ---: | ---: | ---: | ---: |
| 1,000 | Source JIT | 289.6 | 290.0 | 116.8 | 140.7 | n/a |
| 1,000 | Standalone JIT | 282.6 | 310.8 | 121.8 | 143.1 | 140.9 |
| 1,000 | AOT | 232.6 | 276.0 | 94.0 | 122.0 | 87.3 |
| 10,000 | Source JIT | 396.9 | 691.3 | 270.8 | 334.9 | n/a |
| 10,000 | Standalone JIT | 423.4 | 687.5 | 309.1 | 337.9 | 140.9 |
| 10,000 | AOT | 378.8 | 662.7 | 276.0 | 333.0 | 87.3 |

**Do not compare a source runtime's file size with a shipped executable.** The Bun
runtime alone is 115.5 MiB; source mode also needs the application, dependencies, and addon.

### Resident memory and interaction

**RSS snapshots** are from `ps`; peak RSS is the kernel high-water mark from
`/usr/bin/time -lp`. Peak includes the final screenshot, which allocates additional memory.

| Turns | Variant | Idle RSS | After RSS | Scripted interaction, ms |
| ---: | --- | ---: | ---: | ---: |
| 1,000 | Source JIT | 215.2 | 259.9 | 156.5 |
| 1,000 | Standalone JIT | 237.9 | 280.6 | 159.0 |
| 1,000 | AOT | 198.7 | 245.8 | 164.7 |
| 10,000 | Source JIT | 457.5 | 661.2 | 570.0 |
| 10,000 | Standalone JIT | 514.1 | 657.3 | 561.0 |
| 10,000 | AOT | 450.1 | 632.5 | 700.8 |

**Interaction time is automation round-trip time**, not frame time or input latency.
Locators fetch and validate the whole retained tree. At 10,000 turns this is a large
serialization/validation workload, so the 25% AOT loss must not be called a 25% rendering loss.

### Spread and first-run effects

**Ranges are min to max** across the same five runs. These are filesystem-warm medians,
not cold-boot guarantees. The first launch of a new large binary was much slower.

| Turns | Variant | First paint range, ms | Peak RSS range | Idle footprint range | After footprint range |
| ---: | --- | ---: | ---: | ---: | ---: |
| 1,000 | Source JIT | 288.7–327.1 | 286.3–298.5 | 114.0–117.9 | 136.4–142.9 |
| 1,000 | Standalone JIT | 276.2–286.9 | 308.5–315.4 | 120.6–122.0 | 138.7–148.4 |
| 1,000 | AOT | 231.9–234.6 | 275.3–276.4 | 94.0–94.1 | 121.3–122.4 |
| 10,000 | Source JIT | 387.9–1,829.0 | 679.7–700.2 | 265.3–271.5 | 322.6–341.5 |
| 10,000 | Standalone JIT | 418.0–2,045.6 | 677.0–703.6 | 308.0–311.1 | 306.7–349.0 |
| 10,000 | AOT | 376.7–1,184.0 | 662.4–676.0 | 275.6–279.7 | 325.4–342.6 |

## Build and native addon support

**The PR has a working macOS ARM64 CI binary.** No canary fallback or local engine build
was needed. All measured application variants use that exact binary's runtime.

```sh
bunx bun-pr 45631
bun-45631 --revision
# 1.4.4-canary.1+a640bf2cf
```

**The addon works under AOT**, including GPU paint, native input, callbacks, and screenshots.
`lsof` confirmed that Bun extracted and loaded the embedded 24,326,336-byte `.node` from
the macOS temporary directory. No `NAPI_RS_NATIVE_LIBRARY_PATH` or external package was needed.

```sh
cd examples

# Explicit entry and original name are needed for this generated loader in plain --compile.
bun-45631 build --compile chat.tsx ../packages/native/gpuix-native.darwin-arm64.node \
  --asset-naming='[name].[ext]' --outfile ../tmp/bun-aot/chat-compile

# AOT finds and embeds the addon through the generated loader without that extra entry.
bun-45631 build --compile --aot chat.tsx --outfile ../tmp/bun-aot/chat-aot

bun-45631 build --compile --aot --trim chat.tsx --outfile ../tmp/bun-aot/chat-aot-trim
```

**Plain `--compile chat.tsx` alone did not load the addon.** It built successfully, but
startup reported missing `./gpuix-native.darwin-arm64.node`. Minification alone did not
fix it. The explicit asset entry above produced the working standalone JIT comparison.

**Build cost is higher with AOT.** Initial timed builds took 0.15 s / 331.4 MiB peak RSS
for plain compile without the addon entry, and 2.29 s / 1,263.4 MiB for AOT with
`--explain=json`. These are single diagnostic builds, not matched build-time benchmarks.

## What was tested and discarded

**One change at a time.** The first four rows use a real GPU-backed `createTestRoot()`
mount of 10,000 turns. Seven fresh processes per mode and experiment, rotating mode order.
Source JIT used `NODE_ENV=production`, as did both compiled modes.

| Change | Source JIT mount, ms | Standalone JIT mount, ms | AOT mount, ms | Decision |
| --- | ---: | ---: | ---: | --- |
| Baseline | 150.3 | 206.1 | 185.7 | Reference |
| `HostNodeState` class, all fields initialized | 152.5 | 211.1 | 188.7 | Remove. No speed or memory win |
| Return `null` instead of allocating unused host context | 148.3 | 298.4 | 285.5 | Remove. Compiled modes regress |
| `Object.keys` instead of entry-pair arrays in custom-prop sync | 144.6 | 204.6 | 186.5 | Remove. No AOT gain in the live app |
| Switch on `Turn.kind`, two JSX children instead of six | 149.3 | 206.9 | 188.0 | Remove. No AOT gain |

**Mount ranges** keep small differences in perspective. Source / standalone JIT / AOT:

| Change | Source JIT range, ms | Standalone JIT range, ms | AOT range, ms |
| --- | ---: | ---: | ---: |
| Baseline | 145.2–160.2 | 204.8–227.2 | 183.2–187.0 |
| Class state | 147.8–153.5 | 209.8–224.4 | 186.2–189.3 |
| Null context | 144.8–153.4 | 293.4–305.0 | 281.3–303.9 |
| Key enumeration | 142.9–144.8 | 203.9–211.1 | 185.9–201.2 |
| Turn switch | 145.7–150.7 | 205.5–208.8 | 186.3–208.2 |

**Key enumeration reduced JIT peak RSS**, 429.5 → 420.3 MiB in that mount benchmark.
AOT peak RSS was 430.1 → 427.6 MiB, with overlapping ranges. A separate five-run
interleaved live comparison did not confirm an AOT memory or startup benefit, so it stayed out.

### Typed sources and compiler flags

**Typed-source resolution** used a scratch `Bun.build` plugin to resolve React's entry
and the shared host/runtime imports to their original `.ts` files. It changed no package
exports and needed no Rust change. Its output was slightly larger, without a clear win.

| 1,000-turn live AOT experiment | First paint, ms | Peak RSS | Idle footprint | After footprint | Executable |
| --- | ---: | ---: | ---: | ---: | ---: |
| Baseline, paired with typed/keys | 237.1 | 275.5 | 94.8 | 122.7 | 87.3 |
| Local typed React/shared runtime sources | 235.3 | 281.1 | 95.5 | 123.4 | 87.4 |
| Key enumeration | 236.3 | 276.6 | 95.0 | 123.3 | 87.3 |
| Baseline, paired with flags | 238.1 | 276.5 | 94.9 | 122.9 | 87.3 |
| `--trim` | 237.6 | 276.5 | 94.7 | 123.2 | 79.0 |
| `--disallow-code-generation-from-strings` | 237.4 | 274.2 | 94.8 | 127.5 | 87.3 |

**Each group has its own interleaved baseline.** Do not treat the first table's 232.6 ms
and these later 237–238 ms runs as a source-code regression. `--trim` is a repeatable disk
size win, not a proven memory win. Disabling generated code increased after-workload footprint.

## Profiles and limits

**The measured JIT mount is mostly outside the proposed TS edits.** A separate CPU
profile of the real 10,000-turn test mount recorded about 115 ms creating the native test
renderer, 40 ms in `applyBatch`, 24 ms in native flush, and 12 ms in `JSON.stringify`.
`createInstance` accounted for about 9 ms and `Object.entries` about 7 ms.

```sh
cd examples
NODE_ENV=production MOUNT_ONLY=1 TURNS=10000 bun-45631 \
  --cpu-prof --cpu-prof-dir=../tmp/bun-aot --cpu-prof-name=mount.cpuprofile \
  profile-chat-scroll.tsx
```

**The native half limits JS-only improvements.** Even deleting all of the measured
`createInstance` self time cannot halve startup. These profile times are diagnostic,
not additions to the separate timing table.

**The compiler also reports untyped dependency work.** Baseline `--explain=json` counted:

| Code | Free accesses | Tested accesses | By-name accesses |
| --- | ---: | ---: | ---: |
| Application | 270 | 274 | 30 |
| Dependencies, including built adapter JS | 2,763 | 16,694 | 7,161 |

**Do not infer hotness from those totals.** React/reconciler and Safe MDX ship untyped JS.
The host has dynamic prop keys, `Object.entries`, WeakMaps, and public-instance methods
attached after creation. Portal containers use `Object.create`. The compiler's top
diagnostics also name Zod proxies and values passed through unknown code.

**Generated code is a real dependency constraint**, not a measured diagnosis of every loss.
Zod object parsers use `new Function`; these stay interpreted under AOT. Acorn and Markdown
dependencies also emit interpreter-only dynamic-RegExp warnings. The no-generated-code
experiment above did not produce a usable end-to-end memory win.

**AOT symbolication was incomplete.** A three-second native `sample` capture while
repeatedly querying the 10,000-turn tree completed. The PR's `name_sample.py` could not
decode this stripped executable: no `BUN_COMPILED` symbol, then a frame-table `KeyError`
when trying section-address mapping. No per-function AOT timings are claimed.

## Reproduce the measurements

**The benchmark scripts and raw evidence are in `tmp/bun-aot/`**, which is gitignored.
They were run in this checkout; only this report remains as a tracked change.

| File | Purpose |
| --- | --- |
| `measure.ts` | Launch real windows, drive input, record memory and time |
| `prepare.ts` | Copy chat to a scratch entry with 10,000 turns; remove entry side effects for mount profiling |
| `build-typed.ts` | Test resolving local packages to typed source |
| `mount.tsx`, `compare-mount.ts` | Real GPU-backed mount measurements |
| `baseline-{1000,10000}-results.json` | Main raw runs |
| `experiments-1000-results.json`, `flags-1000-results.json` | Paired live experiments |
| `mount-{baseline,class,no-context,keys,switch}.json` | Per-change mount runs |
| `*-vmmap.txt`, `*-time.txt`, `*.png` | Memory summaries, kernel high-water marks, final paints |
| `mount.cpuprofile`, `sample-aot.txt` | Diagnostic CPU artifacts |

```sh
RUNS=5 bun tmp/bun-aot/measure.ts
TURNS=10000 RUNS=5 bun tmp/bun-aot/measure.ts
```

**The large entry is a scratch copy**, because `chat.tsx` fixes its live entry at 1,000
turns. It changes only the entry's count, title, and asset import paths. It uses the same
`ChatApp`, retained children, dimensions, and Safe MDX content. It adds no windowing optimization.

### Measurement method

**Hardware:** macOS 26.7.1, ARM64, 18 logical CPUs, 48 GiB RAM. Initial load average was
3.14 / 2.98 / 3.14. This was a shared, unpinned machine, not a dedicated benchmark host.

**Launch:** `launch()` runs `/usr/bin/time -lp -o <file> <variant>`. `GPUIX_BACKGROUND=1`
sets `focus: false`; no app was activated. All runs set `NODE_ENV=production`. The same
pre-existing release `.node` was used, with no native rebuild.

**First paint:** parent monotonic time from launch until `getPaintedText()` contains both
`New Task` and `DeepSeek V4 Flash`. This includes spawn, IPC, app initialization, and one
observable completed paint. It is not an instrumented display-present timestamp.

**Idle:** blur the caret, wait two seconds for transient allocations to settle, then read
`ps -o rss` and `vmmap -summary`. No forced GC and no memory-pressure or cache manipulation.

**Workload:** send twenty downward 160 px wheel events over the transcript; verify that
painted content changes; open and close Inspector; type and clear `AOT benchmark`; blur;
wait two seconds; measure again; capture a final screenshot and terminate only that child.

**Accounting:** report physical footprint as the macOS private/owned-memory proxy, including
charged graphics allocations. It is not Linux `Private_Dirty`. The scratch JSON field
`privateDirtyMiB` is the `vmmap` TOTAL DIRTY column and is deliberately not reported as
strict private memory here. Shared-library residency and virtual reservations are different metrics.

## Verification and remaining blockers

**Examples pass:** 55/55 tests. **React baseline is not green:** 540/541 pass. The existing
`drives the frame loop from real display-link wakes without blocking I/O` test times out
in its Bun subprocess. It also failed alone, before any application/runtime edits.

**No workaround was kept.** The failing test was not changed or skipped. Its relevant
recent history is `515df198fef2685f5ed7dd120bf0db638eb5c360`, which introduced display-link
pacing and the three-wake assertion. A fix requires a separate investigation.

**Final checks reproduce the same result:** examples 55/55, React 540/541. Example lint
passes. React lint reports two existing errors in untouched `components/button.tsx`
(redundant assertion) and `reconciler/portal.ts` (chained assertion). The React build passes.

**No shipping TS/API changes remain**, so there is no release changeset or README API
change. No Rust changes, commits, pushes, package version edits, or PRs were made.
Unrelated `website/` and `zed` work was left alone.

## Upstream references

**Read revision `a640bf2cfc48869be1b6744fa3436129c82734a2`**, not just the PR body. Its
docs moved from `docs/bundler/aot.mdx` to `docs/compiler/`. The implemented library
condition is **`"aot"`**, with `.aot.ts` output, not the PR body's older `"full-types"` text.

- [Bun PR #45631](https://github.com/oven-sh/bun/pull/45631)
- [Engine PR #757](https://github.com/oven-sh/WebKit/pull/757)
- [AOT overview](https://github.com/oven-sh/bun/blob/a640bf2cfc48869be1b6744fa3436129c82734a2/docs/compiler/aot.mdx)
- [Types and fixed layouts](https://github.com/oven-sh/bun/blob/a640bf2cfc48869be1b6744fa3436129c82734a2/docs/compiler/types.mdx)
- [Typed library condition](https://github.com/oven-sh/bun/blob/a640bf2cfc48869be1b6744fa3436129c82734a2/docs/compiler/libraries.mdx)
- [Smaller executables](https://github.com/oven-sh/bun/blob/a640bf2cfc48869be1b6744fa3436129c82734a2/docs/compiler/size.mdx)
- [Compiler test suite](https://github.com/oven-sh/bun/blob/a640bf2cfc48869be1b6744fa3436129c82734a2/test/aot/README.md)
- [Native addon/plugin tests](https://github.com/oven-sh/bun/blob/a640bf2cfc48869be1b6744fa3436129c82734a2/test/aot/build-plugins-native.test.ts)

**Source-fetch caveat:** the `opensrc` checkout advertised this revision but lacked the
AOT tree. Documentation and tests were therefore read from a commit-pinned GitHub archive,
cross-checked against GitHub's tree API. The downloaded runtime reports the matching revision.
