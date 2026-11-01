import type { EventPayload } from "../index.js"
import type {
  EdgeInsets,
  HighlightSpec,
  HostProps,
  NativeRenderer,
  NativeWindowInsets,
  SelectionHost,
  WindowInsetsHost,
  WindowSizeHost,
} from "./host.js"

export interface WindowSize {
  width: number
  height: number
}

export interface ObserverOptions {
  /** Poll interval in milliseconds. Defaults to 100. Set false for one read. */
  intervalMs?: number | false
}

export interface WindowInsets extends NativeWindowInsets {
  keyboardTop: number
  keyboardVisible: boolean
  visibleHeight: number
}

const DEFAULT_WINDOW_SIZE: WindowSize = { width: 800, height: 600 }
const ZERO_EDGES: EdgeInsets = { top: 0, right: 0, bottom: 0, left: 0 }

const lastWindowSizes = new WeakMap<object, WindowSize>()

export function readWindowSize(
  renderer: WindowSizeHost | NativeRenderer | null
): WindowSize {
  try {
    const size = renderer?.getWindowSize?.()
    if (renderer && size && size.width > 0 && size.height > 0) {
      lastWindowSizes.set(renderer, { ...size })
      return { ...size }
    }
  } catch {
    // A failed read says nothing about the size, so keep the last good one.
    // With none, the platform window can still be opening.
    const last = renderer ? lastWindowSizes.get(renderer) : undefined
    if (last) return { ...last }
  }
  return { ...DEFAULT_WINDOW_SIZE }
}

export function readWindowInsets(
  renderer: (WindowSizeHost & WindowInsetsHost) | NativeRenderer | null
): WindowInsets {
  const size = readWindowSize(renderer)
  let insets: NativeWindowInsets = {
    safeArea: ZERO_EDGES,
    ime: ZERO_EDGES,
    effective: ZERO_EDGES,
  }
  try {
    insets = renderer?.getWindowInsets?.() ?? insets
  } catch {
    // The platform window can still be opening.
  }
  return {
    safeArea: { ...insets.safeArea },
    ime: { ...insets.ime },
    effective: { ...insets.effective },
    keyboardTop: size.height - insets.ime.bottom,
    keyboardVisible: insets.ime.bottom > 0,
    visibleHeight: size.height - insets.effective.top - insets.effective.bottom,
  }
}

function sameWindowSize(left: WindowSize, right: WindowSize): boolean {
  return left.width === right.width && left.height === right.height
}

function sameEdges(left: EdgeInsets, right: EdgeInsets): boolean {
  return left.top === right.top && left.right === right.right &&
    left.bottom === right.bottom && left.left === right.left
}

function sameWindowInsets(left: WindowInsets, right: WindowInsets): boolean {
  return left.keyboardTop === right.keyboardTop &&
    left.keyboardVisible === right.keyboardVisible &&
    left.visibleHeight === right.visibleHeight &&
    sameEdges(left.safeArea, right.safeArea) &&
    sameEdges(left.ime, right.ime) &&
    sameEdges(left.effective, right.effective)
}

function observe<Value>(
  read: () => Value,
  same: (left: Value, right: Value) => boolean,
  callback: (value: Value) => void,
  options: ObserverOptions
): () => void {
  let current = read()
  callback(current)
  if (options.intervalMs === false) return () => {}
  const timer = setInterval(() => {
    const next = read()
    if (same(current, next)) return
    current = next
    callback(next)
  }, Math.max(16, options.intervalMs ?? 100))
  return () => clearInterval(timer)
}

export function observeWindowSize(
  renderer: WindowSizeHost | NativeRenderer | null,
  callback: (size: WindowSize) => void,
  options: ObserverOptions = {}
): () => void {
  return observe(() => readWindowSize(renderer), sameWindowSize, callback, options)
}

export function observeWindowInsets(
  renderer: (WindowSizeHost & WindowInsetsHost) | NativeRenderer | null,
  callback: (insets: WindowInsets) => void,
  options: ObserverOptions = {}
): () => void {
  return observe(() => readWindowInsets(renderer), sameWindowInsets, callback, options)
}

export function observeSelectedText(
  renderer: SelectionHost,
  subscribe: (listener: (event: EventPayload) => void) => () => void,
  callback: (text: string | null) => void
): () => void {
  const stop = subscribe((event) => callback(event.value ?? null))
  try {
    callback(renderer.getSelectedText() ?? null)
  } catch (error) {
    stop()
    throw error
  }
  return stop
}

export interface FindRangesOptions {
  text: string
  query: string
  caseSensitive?: boolean
  wholeWord?: boolean
}

const WORD_CHAR = /[\p{Alphabetic}\p{N}\p{M}_]/u

function wordCharBefore(text: string, end: number): boolean {
  if (end <= 0) return false
  const low = text.charCodeAt(end - 1)
  const start = low >= 0xdc00 && low <= 0xdfff && end >= 2 ? end - 2 : end - 1
  return WORD_CHAR.test(text.slice(start, end))
}

function wordCharAt(text: string, start: number): boolean {
  const codePoint = text.codePointAt(start)
  return codePoint === undefined ? false : WORD_CHAR.test(String.fromCodePoint(codePoint))
}

function fold(text: string): { folded: string; map: number[] } {
  const folded = text.toLowerCase()
  const map: number[] = []
  // Contextual lowercase changes sigma's value, not its length, so scalar lengths map both.
  for (let index = 0; index < text.length;) {
    const char = String.fromCodePoint(text.codePointAt(index) ?? 0)
    const lower = char.toLowerCase()
    for (let unit = 0; unit < lower.length; unit += 1) map.push(index)
    index += char.length
  }
  return { folded, map }
}

export function findRanges(options: FindRangesOptions): Array<[number, number]> {
  const { text, query, caseSensitive = false, wholeWord = false } = options
  if (query.length === 0) return []
  const { folded, map } = caseSensitive ? { folded: text, map: null } : fold(text)
  const needle = caseSensitive ? query : query.toLowerCase()
  const ranges: Array<[number, number]> = []
  let from = 0
  for (;;) {
    const at = folded.indexOf(needle, from)
    if (at < 0) break
    from = at + needle.length
    const start = map ? map[at]! : at
    // The final folded unit can be inside an expansion; include its complete source character.
    const last = map ? map[from - 1]! : from - 1
    const end = map ? last + (text.codePointAt(last)! > 0xffff ? 2 : 1) : from
    if (start >= end) continue
    if (start < (ranges.at(-1)?.[1] ?? 0)) continue
    if (wholeWord && (wordCharBefore(text, start) || wordCharAt(text, end))) continue
    ranges.push([start, end])
  }
  return ranges
}

export interface TextSearchOptions {
  query: string
  caseSensitive?: boolean
  wholeWord?: boolean
  color?: string
  activeColor?: string
  radius?: number
  matches?: { total: number; indexOffset: number }
}

export interface TextSearchSnapshot {
  props: Pick<HostProps, "highlight" | "onHighlight">
  total: number
  active: number
}

export interface TextSearchController {
  getSnapshot(options: TextSearchOptions): TextSearchSnapshot
  next(): void
  previous(): void
  goTo(index: number): void
  subscribe(callback: () => void): () => void
}

export function createTextSearchController(): TextSearchController {
  let reported = 0
  let requested = 0
  let latest: TextSearchOptions = { query: "" }
  const listeners = new Set<() => void>()
  const emit = () => listeners.forEach((listener) => listener())
  const total = () => latest.query.length === 0 ? 0 : (latest.matches?.total ?? reported)
  const active = () => total() === 0 ? 0 : Math.min(requested, total() - 1)
  const onHighlight = (event: EventPayload) => {
    const next = event.matchCount ?? 0
    if (reported === next) return
    reported = next
    emit()
  }

  return {
    getSnapshot(options) {
      latest = options
      const currentTotal = total()
      const currentActive = active()
      const highlight: HighlightSpec | null = options.query.length === 0 ? null : {
        query: options.query,
        caseSensitive: options.caseSensitive,
        wholeWord: options.wholeWord,
        color: options.color,
        activeColor: options.activeColor,
        radius: options.radius,
        activeIndex: currentActive,
        matchIndexOffset: options.matches?.indexOffset,
      }
      return {
        props: { highlight, onHighlight },
        total: currentTotal,
        active: currentActive,
      }
    },
    next() {
      const count = total()
      if (count === 0) return
      requested = (Math.min(requested, count - 1) + 1) % count
      emit()
    },
    previous() {
      const count = total()
      if (count === 0) return
      requested = (Math.min(requested, count - 1) + count - 1) % count
      emit()
    },
    goTo(index) {
      if (index < 0 || index >= total()) return
      requested = index
      emit()
    },
    subscribe(callback) {
      listeners.add(callback)
      return () => listeners.delete(callback)
    },
  }
}
