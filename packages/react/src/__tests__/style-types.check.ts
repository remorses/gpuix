// Type-only checks for the `style` prop. `tsc --noEmit` runs them, and nothing
// imports this at runtime.
//
// A `@ts-expect-error` that stops being an error fails the build, so these
// pin the rejections as firmly as the acceptances.

import type { StyleDesc } from "../types/host.js"

const declares: StyleDesc = {
  "--brand": "#ff0000",
  "--pad": 8,
  color: "var(--brand)",
}

// @ts-expect-error one dash is not a custom property
const oneDash: StyleDesc = { "-pad": 8 }

// @ts-expect-error a custom property holds text, not an object
const notText: StyleDesc = { "--brand": { hue: 1 } }

// @ts-expect-error the index signature must not loosen the known fields
const wrongFieldType: StyleDesc = { color: 42 }

// @ts-expect-error a state has no cascade of its own to declare into
const declaredInHover: StyleDesc = { hover: { "--brand": "#ff0000" } }

// @ts-expect-error states do not nest
const nestedState: StyleDesc = { hover: { hover: {} } }

export type Checked = typeof declares &
  typeof oneDash &
  typeof notText &
  typeof wrongFieldType &
  typeof declaredInHover &
  typeof nestedState
