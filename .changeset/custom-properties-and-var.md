---
"@gpuix/native": minor
"@gpuix/react": minor
---

Support CSS custom properties in the `style` prop. A `--name` key declares a value for the element and everything below it, and `var(--name)` reads it, with fallbacks including the empty one Tailwind writes as `var(--tw-ring-inset,)`. A missing variable with no fallback drops the declaration, which is what CSS calls invalid at computed-value time.

Support `currentColor`. It reads the computed `color`, whether the element declares it or an ancestor does.

Take text in every numeric style field. `padding`, `borderWidth`, `fontSize` and the other 33 numeric fields now accept `number | string`, so `8`, `"8px"` and `"var(--pad)"` all mean the same thing. A unit the renderer cannot read, such as `2rem`, drops the declaration instead of painting the number as pixels.

Type custom properties with a pattern index signature, so `"-pad"` is a type error rather than a name that silently never resolves. `hover` and `active` reject them, because a state has no cascade of its own to declare into.

Keep the resolved-style cache correct without giving up on elements that use no variable. A resolution that reads nothing inherited holds under every cascade, so only a `var()` or `currentColor` reader is ever invalidated by an ancestor.

Stop re-resolving an element that has no `style` prop. The reconciler skips the call for an empty style at mount but sends `{}` on every update, so the first update on every unstyled element read as a change.

Typecheck the `style` prop rules. `src/__tests__` was excluded from every tsconfig, so nothing checked the type assertions. `bun run typecheck` covers them now, and CI runs it.
