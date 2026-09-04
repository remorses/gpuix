/// A class resolver, of the kind `@gpuix/tailwind` will be.
///
/// A root takes one through `createRoot(renderer, { resolveClassName })`. It
/// reads one token such as `p-4`, never a whole class string, so the root
/// caches each token once and reuses it for every element that carries it.
///
/// Every colour token points at a custom property rather than a literal, and
/// every spacing token folds a `calc()` over `--spacing`. So the class channel,
/// the variable channel and the length parser all run on the same declaration.

import type { StyleDesc } from "@gpuix/react"

type State = NonNullable<StyleDesc["hover"]>

/// Font sizes, so `text-lg` is a size and `text-brand` is a colour.
const SIZES: Record<string, number> = {
  xs: 11,
  sm: 13,
  base: 15,
  lg: 18,
  xl: 22,
  "2xl": 30,
  "3xl": 40,
}

const RADII: Record<string, number> = {
  none: 0,
  sm: 4,
  md: 8,
  lg: 12,
  xl: 18,
  full: 9999,
}

const WEIGHTS: Record<string, string> = {
  normal: "normal",
  medium: "500",
  semibold: "600",
  bold: "bold",
}

/// Tokens with no value part.
const EXACT: Record<string, StyleDesc> = {
  row: { display: "flex", flexDirection: "row" },
  col: { display: "flex", flexDirection: "column" },
  wrap: { flexWrap: "wrap" },
  grow: { flexGrow: 1 },
  "min-h-0": { minHeight: 0 },
  relative: { position: "relative" },
  absolute: { position: "absolute" },
  border: { borderWidth: 1, borderColor: "var(--color-line)" },
  rounded: { borderRadius: 8 },
  mono: { fontFamily: "var(--font-mono)" },
  pointer: { cursor: "pointer" },
  "select-none": { userSelect: "none" },
  truncate: { whiteSpace: "nowrap", textOverflow: "ellipsis", overflow: "hidden" },
  "scroll-y": { overflowY: "scroll" },
  "full": { width: "100%", height: "100%" },
  "w-full": { width: "100%" },
  "h-full": { height: "100%" },
}

/// A Tailwind arbitrary value, so `bg-[oklch(0.7_0.15_200)]` reaches the
/// parser as written. Tailwind spells a space as an underscore, because a
/// class attribute splits on whitespace.
function arbitrary(value: string): string | null {
  if (!value.startsWith("[") || !value.endsWith("]")) return null
  return value.slice(1, -1).replace(/_/g, " ")
}

/// A step of the spacing scale, as the arithmetic Tailwind itself writes.
function step(value: string): string | null {
  const raw = arbitrary(value)
  if (raw) return raw
  const count = Number(value)
  return Number.isFinite(count) ? `calc(var(--spacing) * ${count})` : null
}

/// A colour token, read through the palette rather than written in place.
function color(value: string): string | null {
  return arbitrary(value) ?? `var(--color-${value})`
}

function spacing(keys: string[], value: string): StyleDesc | null {
  const length = step(value)
  if (!length) return null
  const style: Record<string, string> = {}
  for (const key of keys) style[key] = length
  return style as StyleDesc
}

function base(token: string): StyleDesc | null {
  const exact = EXACT[token]
  if (exact) return exact

  const dash = token.indexOf("-")
  if (dash <= 0) return null
  const prefix = token.slice(0, dash)
  const value = token.slice(dash + 1)

  switch (prefix) {
    case "p":
      return spacing(["padding"], value)
    case "px":
      return spacing(["paddingLeft", "paddingRight"], value)
    case "py":
      return spacing(["paddingTop", "paddingBottom"], value)
    case "pt":
      return spacing(["paddingTop"], value)
    case "pb":
      return spacing(["paddingBottom"], value)
    case "m":
      return spacing(["margin"], value)
    case "mt":
      return spacing(["marginTop"], value)
    case "mb":
      return spacing(["marginBottom"], value)
    case "gap":
      return spacing(["gap"], value)
    case "w":
      return spacing(["width"], value)
    case "h":
      return spacing(["height"], value)
    case "items":
      return { alignItems: value }
    case "justify":
      return { justifyContent: value }
    case "self":
      return { alignSelf: value }
    case "bg": {
      const backgroundColor = color(value)
      return backgroundColor ? { backgroundColor } : null
    }
    case "text": {
      const size = SIZES[value]
      if (size !== undefined) return { fontSize: size }
      const textColor = color(value)
      return textColor ? { color: textColor } : null
    }
    case "font": {
      const weight = WEIGHTS[value]
      return weight ? { fontWeight: weight } : null
    }
    case "align":
      return { textAlign: value }
    case "leading": {
      const height = arbitrary(value) ?? value
      return { lineHeight: height }
    }
    case "rounded": {
      const radius = RADII[value]
      return radius === undefined ? null : { borderRadius: radius }
    }
    case "ring":
      return { borderWidth: 1, borderColor: color(value) ?? undefined }
    case "space": {
      // `space-y-2` puts a margin on every child except the last one.
      const at = value.indexOf("-")
      if (at !== 1) return null
      const length = step(value.slice(2))
      if (!length) return null
      const key = value[0] === "x" ? "marginRight" : value[0] === "y" ? "marginBottom" : null
      if (!key) return null
      return { selectors: [{ on: "& > :not(:last-child)", style: { [key]: length } }] }
    }
    case "divide": {
      // `divide-y` puts a line under every child except the last one.
      const key =
        value === "x" ? "borderRightWidth" : value === "y" ? "borderBottomWidth" : null
      if (!key) return null
      return {
        selectors: [
          {
            on: "& > :not(:last-child)",
            style: { [key]: 1, borderColor: "var(--color-line)" },
          },
        ],
      }
    }
    case "opacity": {
      const amount = Number(value)
      return Number.isFinite(amount) ? { opacity: amount / 100 } : null
    }
    default:
      return null
  }
}

const STATES = ["hover", "active"] as const

/// The selector each variant prefix stands for, in the canonical spelling
/// the engine reads.
const SELECTORS: Record<string, string> = {
  first: ":first-child",
  last: ":last-child",
  odd: ":nth-child(odd)",
  even: ":nth-child(even)",
  only: ":only-child",
  "*": "& > *",
  "**": "& *",
}

/// The style one token declares, or `null` for a token this does not know.
export function resolveClassName(token: string): StyleDesc | null {
  for (const state of STATES) {
    if (!token.startsWith(`${state}:`)) continue
    const inner = base(token.slice(state.length + 1))
    if (!inner || inner.selectors) return null
    // A state holds no nesting and no custom properties, and `base` never
    // returns either, so this reads the same object under the narrower type.
    return state === "hover" ? { hover: inner as State } : { active: inner as State }
  }
  const colon = token.indexOf(":")
  if (colon > 0) {
    const on = SELECTORS[token.slice(0, colon)]
    if (!on) return null
    const inner = base(token.slice(colon + 1))
    // One level deep, as with the states: a selector cannot hold a selector.
    if (!inner || inner.selectors) return null
    return { selectors: [{ on, style: inner as State }] }
  }
  return base(token)
}

/// How many tokens the resolver has been asked for since it loaded.
///
/// The root caches each token, so this stops climbing once the application has
/// painted every class it uses. The class panel reads it.
let asked = 0
export function resolverCalls(): number {
  return asked
}
export function countedResolveClassName(token: string): StyleDesc | null {
  asked += 1
  return resolveClassName(token)
}
