/// Turning a `className` into a style.
///
/// GPUIX ships no resolver. A root takes one through
/// `createRoot(renderer, { resolveClassName })`, and `@gpuix/tailwind` is the
/// one this repository plans to publish. Without a resolver a `className` does
/// nothing and warns once.
///
/// The resolver reads one token, such as `p-4`, and never a whole string. That
/// is what makes the cache work. `clsx("p-4", a && "bg-blue-500", b &&
/// "text-lg")` writes up to eight strings from three tokens, and five toggles
/// write thirty-two. A bounded cache over whole strings sits in front, because
/// the same string usually repeats between two frames and then neither the
/// split nor the merge runs.

import type {
  ClassNameCache,
  ClassNameResolver,
  SelectorRule,
  StyleDeclarations,
  StyleDesc,
} from "../types/host.js"

/// How many whole class strings a root remembers.
///
/// The token cache below it is unbounded, because the set of tokens an
/// application uses is fixed by its source code. The set of strings is not: it
/// grows with every combination of conditional classes.
const STRING_LIMIT = 256

export function createClassNameCache(resolve: ClassNameResolver): ClassNameCache {
  return { resolve, tokens: new Map(), strings: new Map() }
}

/// The style a class string declares, or `null` when it declares nothing.
///
/// The result is the cached object, shared by every element with this class
/// string. Callers read it and copy from it. None of them write to it.
export function styleForClassName(
  className: string | undefined,
  cache: ClassNameCache | null
): StyleDesc | null {
  if (!className) return null
  if (!cache) return null

  const hit = cache.strings.get(className)
  if (hit !== undefined) {
    // Least recently used goes out first, so a hit moves to the back.
    cache.strings.delete(className)
    cache.strings.set(className, hit)
    return hit
  }

  const merged: Mutable = {}
  let declared = false
  for (const token of className.split(/\s+/)) {
    if (!token) continue
    const style = tokenStyle(token, cache)
    if (!style) continue
    mergeInto(merged, style)
    declared = true
  }

  const style = declared ? (merged as StyleDesc) : null
  if (style) {
    if (cache.strings.size >= STRING_LIMIT) {
      const oldest = cache.strings.keys().next()
      if (!oldest.done) cache.strings.delete(oldest.value)
    }
    cache.strings.set(className, style)
  }
  return style
}

function tokenStyle(token: string, cache: ClassNameCache): StyleDesc | null {
  const cached = cache.tokens.get(token)
  if (cached !== undefined) return cached
  const style = cache.resolve(token) ?? null
  cache.tokens.set(token, style)
  return style
}

/// A style being built. `StyleDesc` has no index signature for its own keys, so
/// the merges below write through this instead of casting at each line.
type Mutable = Record<string, unknown>

function mergeInto(target: Mutable, source: StyleDesc): void {
  for (const [key, value] of Object.entries(source)) {
    if (key === "hover" || key === "active" || key === "selectors") continue
    target[key] = value
  }
  mergeState(target, "hover", source.hover)
  mergeState(target, "active", source.active)
  mergeSelectors(target, source.selectors)
}

function mergeState(
  target: Mutable,
  state: "hover" | "active",
  source: StyleDeclarations | undefined
): void {
  if (!source) return
  target[state] = { ...(target[state] as StyleDeclarations | undefined), ...source }
}

/// Two tokens on the same selector merge into one rule, the way two
/// declarations in one CSS rule do, so `first:p-2 first:bg-red` sends one
/// `:first-child` block.
function mergeSelectors(target: Mutable, source: SelectorRule[] | undefined): void {
  if (!source) return
  const merged = [...((target.selectors as SelectorRule[] | undefined) ?? [])]
  for (const rule of source) {
    const at = merged.findIndex((held) => held.on === rule.on)
    if (at === -1) {
      merged.push({ on: rule.on, style: { ...rule.style } })
    } else {
      merged[at] = { on: rule.on, style: { ...merged[at]!.style, ...rule.style } }
    }
  }
  target.selectors = merged
}

/// The style prop laid over the style a class string declared.
///
/// [CSS Style Attributes][spec] gives the attribute "a specificity higher than
/// any selector", so an inline declaration wins over a class in every state. A
/// key the style prop sets is therefore removed from `hover` and `active` as
/// well, or an element with `style={{ backgroundColor: "red" }}` would turn
/// blue under a `hover:bg-blue-500` class, where a browser keeps it red.
///
/// [spec]: https://www.w3.org/TR/css-style-attr/#cascading
export function withInlineStyle(
  fromClass: StyleDesc | null,
  inline: StyleDesc | undefined
): StyleDesc {
  if (!fromClass) return inline ?? {}
  if (!inline) return fromClass

  const merged: Mutable = { ...fromClass }
  const hover = fromClass.hover ? { ...(fromClass.hover as Mutable) } : undefined
  const active = fromClass.active ? { ...(fromClass.active as Mutable) } : undefined
  if (hover) merged.hover = hover
  if (active) merged.active = active
  const selectors = fromClass.selectors?.map((rule) => ({
    on: rule.on,
    style: { ...rule.style } as Mutable,
  }))
  if (selectors) merged.selectors = selectors

  for (const [key, value] of Object.entries(inline)) {
    if (key === "hover" || key === "active") continue
    merged[key] = value
    if (hover) delete hover[key]
    if (active) delete active[key]
    // The style prop outranks every selector, so a key it sets leaves the
    // conditioned blocks too. Only the element's own states though: a rule
    // on the children (`& > *`) styles other elements, and the inline style
    // of this one says nothing about those.
    if (selectors) {
      for (const rule of selectors) {
        if (!rule.on.startsWith("&")) delete rule.style[key]
      }
    }
  }
  mergeState(merged, "hover", inline.hover)
  mergeState(merged, "active", inline.active)
  return merged as StyleDesc
}
