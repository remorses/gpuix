import {
  BUILT_IN_TYPES,
  UNIVERSAL_PROPS,
  registerEventHandler,
  serializeCustomProp,
  unregisterEventHandler,
  unregisterEventHandlers,
} from "@gpuix/native/host"
import type {
  ElementType,
  EventHandlerMap,
  HostEventHandler,
  ImagePixelsOptions,
  MutationRenderer,
  NativeRenderer,
  StyleDesc,
} from "@gpuix/native/host"
import { bindEvents as bindIlhaEvents } from "ilha/renderer"
import type { FiberLocal, PaintOps, PropValue } from "ilha/renderer"

import { gpuixEventTypeFor, hostEventFor } from "./events.js"
import type { HostEvent } from "./events.js"
import { elementTypeForTag, parseStyleText } from "./props.js"

export type HostListener = (event: HostEvent) => void

/** One node of the GPUIX host tree. */
export type HostNode = HostElement | HostText
export type HostParent = HostRoot | HostElement

/** What a component reads from the host it is mounted in. */
export interface HostGpuixContext {
  renderer: NativeRenderer
  /** Subscribe to the window text selection; returns an unsubscribe function. */
  subscribeSelection: (callback: (text: string | null) => void) => () => void
}

type CustomPropValue = Parameters<typeof serializeCustomProp>[0]

/** What the native element currently holds, so a repaint sends only changes. */
interface SentState {
  style: StyleDesc
  props: Map<string, unknown>
  events: Set<string>
}

const emptySent = (): SentState => ({
  events: new Set(),
  props: new Map(),
  style: {},
})

/**
 * Renderer-wide state every node reaches through its root.
 *
 * ilha allows several root children and GPUIX paints one, so the root owns a
 * full-size flex-column `<div>` and the app tree sits inside it.
 */
export class HostRoot {
  readonly kind = "root" as const
  readonly transparent = false
  readonly mounted = true
  children: HostNode[] = []
  readonly mutations: MutationRenderer
  readonly nativeRenderer: NativeRenderer
  readonly eventHandlers: EventHandlerMap
  readonly allocateId: () => number
  /** Id of the native root element. */
  readonly id: number
  /** Ids of the native root element's children, in native order. */
  nativeChildren: number[] = []
  /**
   * Native elements to destroy once this commit's moves are sent. Native
   * destroy takes the whole subtree, so an element a paint moved out of a
   * dropped subtree has to leave it first.
   */
  readonly pendingDestroys: number[] = []
  destroyed = false
  /** Set by `createRoot`; read by `useGpuix()`. */
  gpuix: HostGpuixContext | null = null
  private readonly onError: (error: Error) => void

  constructor(options: {
    mutations: MutationRenderer
    nativeRenderer: NativeRenderer
    eventHandlers: EventHandlerMap
    allocateId: () => number
    onError?: (error: Error) => void
  }) {
    this.mutations = options.mutations
    this.nativeRenderer = options.nativeRenderer
    this.eventHandlers = options.eventHandlers
    this.allocateId = options.allocateId
    this.onError =
      options.onError ??
      ((error) => console.error("[gpuix-ilha] commit error", error))
    this.id = options.allocateId()
    this.mutations.createElement(this.id, "div")
    this.mutations.setStyle(this.id, {
      display: "flex",
      flexDirection: "column",
      height: "100%",
      width: "100%",
    })
    this.mutations.setRoot(this.id)
    ROOTS.add(this)
  }

  /** Bring the native tree up to date with the host tree and flush the batch. */
  commit(): void {
    try {
      syncNative(this)
      for (const id of this.pendingDestroys.splice(0)) {
        this.mutations.destroyElement(id)
      }
      this.mutations.flushMutations()
    } catch (error) {
      this.onError(error instanceof Error ? error : new Error(String(error)))
    }
  }

  /** Destroy the app tree and the native root element. */
  teardown(): void {
    if (this.destroyed) {
      return
    }
    // Native destroy is recursive, so the root element takes the tree with it.
    for (const child of this.children) {
      unmountNode(child, false)
    }
    this.children = []
    this.nativeChildren = []
    this.pendingDestroys.length = 0
    this.mutations.destroyElement(this.id)
    this.destroyed = true
    DIRTY.delete(this)
    ROOTS.delete(this)
  }
}

export class HostElement {
  readonly kind = "element" as const
  readonly type: ElementType
  readonly tagName: string
  children: HostNode[] = []
  parent: HostParent | null = null
  root: HostRoot | null = null
  id = 0
  mounted = false
  destroyed = false
  /**
   * A painter slot host carries no native element of its own: its children are
   * placed in the nearest native ancestor, so a component adds no box.
   */
  transparent = false
  key: string | null = null
  style: StyleDesc = {}
  attrs = new Map<string, string>()
  props = new Map<string, unknown>()
  listeners = new Map<string, Set<HostListener>>()
  dataset: Record<string, string> = {}
  /** Current `value` of a form control; handlers read it. */
  value: string | null = null
  /** Ids of this element's native children, in native order. */
  nativeChildren: number[] = []
  sent: SentState = emptySent()

  // ilha stamps these onto the element whose props it applies.
  __ilhaHandlers?: Map<string, PropValue>
  __ilhaFiber?: FiberLocal
  __ilhaEvents?: Set<string>
  __ilhaVal?: PropValue

  constructor(type: ElementType, tag: string) {
    this.type = type
    this.tagName = tag.toUpperCase()
  }

  get isConnected(): boolean {
    return isAttached(this)
  }

  getAttribute(name: string): string | null {
    return this.attrs.get(name) ?? null
  }

  setAttribute(name: string, value: string): void {
    this.attrs.set(name, value)
    if (name === "data-ilha-key") {
      this.key = value
    }
  }

  removeAttribute(name: string): void {
    this.attrs.delete(name)
  }

  appendChild(node: HostNode): void {
    node.parent = this
    this.children.push(node)
    if (this.mounted) {
      markDirty(this)
      scheduleCommit()
    }
  }

  append(...nodes: HostNode[]): void {
    for (const node of nodes) {
      this.appendChild(node)
    }
  }

  /**
   * ilha's event binder calls this once per event type. The listener it passes
   * reads the element's current handlers, so the native listener is bound once.
   */
  addEventListener(type: string, listener: HostListener): void {
    const existing = this.listeners.get(type)
    if (existing) {
      existing.add(listener)
    } else {
      this.listeners.set(type, new Set([listener]))
    }
    if (this.mounted) {
      patchElement(this)
      scheduleCommit()
    }
  }

  removeEventListener(type: string, listener: HostListener): void {
    const existing = this.listeners.get(type)
    if (!existing?.delete(listener)) {
      return
    }
    if (existing.size === 0) {
      this.listeners.delete(type)
    }
    if (this.mounted) {
      patchElement(this)
      scheduleCommit()
    }
  }

  scrollIntoView(): void {
    const id = nativeIdOf(this)
    if (id) {
      this.root?.nativeRenderer.scrollIntoView?.(id)
    }
  }

  /** Encoded image bytes onto an `<img>`. */
  setImage(bytes: Buffer | Uint8Array): void {
    const id = nativeIdOf(this)
    if (id) {
      this.root?.nativeRenderer.setImage?.(id, bytes as Buffer)
    }
  }

  /** Packed pixels onto an `<img>`. */
  setImagePixels(
    width: number,
    height: number,
    pixels: Buffer | Uint8Array,
    options?: ImagePixelsOptions
  ): void {
    const id = nativeIdOf(this)
    if (id) {
      this.root?.nativeRenderer.setImagePixels?.(
        id,
        width,
        height,
        pixels as Buffer,
        options?.format
      )
    }
  }
}

export class HostText {
  readonly kind = "text" as const
  readonly transparent = false
  readonly children: HostNode[] = []
  parent: HostParent | null = null
  root: HostRoot | null = null
  id = 0
  mounted = false
  destroyed = false

  constructor(public text: string) {}

  get isConnected(): boolean {
    return isAttached(this)
  }
}

/** Like DOM `isConnected`: true only while the node hangs off a live root. */
function isAttached(node: HostNode): boolean {
  let current: HostNode | HostRoot | null = node
  while (current && current.kind !== "root") {
    if (current.destroyed) {
      return false
    }
    current = current.parent
  }
  return current !== null && !current.destroyed
}

const warned = new Set<string>()

export function warnOnce(key: string, message: string): void {
  if (warned.has(key)) {
    return
  }
  warned.add(key)
  console.warn(message)
}

export function createHostElement(tag: string): HostElement {
  const type = elementTypeForTag(tag)
  if (!type) {
    warnOnce(
      `unsupported-tag:${tag}`,
      `@gpuix/ilha: <${tag}> has no native counterpart and renders as <div>. ` +
        "Use a GPUIX element: div, text, img, svg, input, textarea, code, diff, markdown, virtual-list, anchored."
    )
    return new HostElement("div", tag)
  }
  return new HostElement(type, tag)
}

export function createHostText(text: string): HostText {
  return new HostText(String(text))
}

// ── commit scheduling ─────────────────────────────────────────────────

/** Live roots; a scheduled commit syncs every one of them. */
const ROOTS = new Set<HostRoot>()
/** Native containers whose child list may differ from what was last sent. */
const DIRTY = new Set<HostParent>()

let commitScheduled = false

function scheduleCommit(): void {
  if (commitScheduled) {
    return
  }
  commitScheduled = true
  queueMicrotask(commitAll)
}

/** Commit every live root now. */
export function commitAll(): void {
  commitScheduled = false
  for (const root of [...ROOTS]) {
    root.commit()
  }
}

/**
 * A ref callback runs before the commit that mounts its element. Committing on
 * demand lets `ref={(el) => el.scrollIntoView()}` work on first paint.
 */
function nativeIdOf(node: HostElement): number {
  if (!node.mounted && !node.destroyed) {
    commitAll()
  }
  return node.id
}

// ── logical tree ──────────────────────────────────────────────────────

/** Empty text is ilha's placeholder for a skipped view; it paints nothing. */
const isNative = (node: HostNode): boolean =>
  node.kind === "text" ? node.text !== "" : !node.transparent

/** A node ilha handed back from an earlier paint; it is kept as it is. */
const isLive = (node: HostNode): boolean => node.mounted || node.transparent

/** The native element whose children hold `parent`'s children. */
function containerOf(parent: HostParent): HostParent | null {
  let current: HostParent | null = parent
  while (current?.kind === "element" && current.transparent) {
    current = current.parent
  }
  return current
}

function markDirty(parent: HostParent): void {
  const container = containerOf(parent)
  if (container) {
    DIRTY.add(container)
  }
}

/** Whether `fresh` can take over the native element of `old`. */
function canAdopt(fresh: HostNode, old: HostNode): boolean {
  if (old.destroyed || fresh.kind !== old.kind) {
    return false
  }
  if (fresh.kind !== "element" || old.kind !== "element") {
    return true
  }
  return (
    !fresh.transparent &&
    !old.transparent &&
    fresh.type === old.type &&
    fresh.key === old.key
  )
}

/**
 * Hand the native element of `old` to `fresh`.
 *
 * ilha rebuilds its host nodes on every paint. Keeping the id means the native
 * element is patched instead of recreated, so hover, focus, caret, scroll
 * offset and text selection all survive a repaint.
 */
function adopt(fresh: HostElement, old: HostElement): void {
  fresh.id = old.id
  fresh.root = old.root
  fresh.mounted = old.mounted
  fresh.sent = old.sent
  fresh.nativeChildren = old.nativeChildren
  if (DIRTY.delete(old)) {
    DIRTY.add(fresh)
  }
  const previous = old.children
  // The old object is dropped without a native destroy: `fresh` owns its id.
  old.children = []
  old.id = 0
  old.mounted = false
  old.destroyed = true
  if (fresh.mounted) {
    patchElement(fresh)
  }
  setChildren(fresh, fresh.children, previous, old)
  old.parent = null
  old.root = null
}

/**
 * Make `parent`'s children `next`, reusing native elements from `previous`.
 *
 * A keyed child matches the previous child with the same key wherever it sits,
 * so sorting a list moves elements. An unkeyed child matches the previous child
 * in the same position. Previous children nothing claimed are destroyed, unless
 * the paint already moved them under another parent.
 */
function setChildren(
  parent: HostParent,
  next: readonly HostNode[],
  previous: readonly HostNode[],
  owner: HostParent
): void {
  const claimed = new Set<HostNode>()
  for (const node of next) {
    if (isLive(node)) {
      claimed.add(node)
    }
  }
  let keyed: Map<string, HostNode> | undefined
  const matchFor = (fresh: HostNode, index: number): HostNode | undefined => {
    let candidate: HostNode | undefined = previous[index]
    if (fresh.kind === "element" && fresh.key !== null) {
      if (!keyed) {
        keyed = new Map()
        for (const node of previous) {
          if (node.kind === "element" && node.key !== null) {
            keyed.set(node.key, node)
          }
        }
      }
      candidate = keyed.get(fresh.key)
    }
    return candidate && !claimed.has(candidate) && canAdopt(fresh, candidate)
      ? candidate
      : undefined
  }

  const kept: HostNode[] = []
  for (let index = 0; index < next.length; index += 1) {
    const fresh = next[index]
    if (!fresh || fresh.destroyed) {
      continue
    }
    const old = isLive(fresh) ? undefined : matchFor(fresh, index)
    if (!old) {
      fresh.parent = parent
      kept.push(fresh)
      continue
    }
    claimed.add(old)
    if (fresh.kind === "text" && old.kind === "text") {
      setHostText(old, fresh.text)
      old.parent = parent
      kept.push(old)
      continue
    }
    adopt(fresh as HostElement, old as HostElement)
    fresh.parent = parent
    kept.push(fresh)
  }
  for (const old of previous) {
    if (!claimed.has(old) && old.parent === owner) {
      unmountNode(old, true)
      old.parent = null
    }
  }
  parent.children = kept
  markDirty(parent)
}

function setHostText(node: HostText, text: string): void {
  if (node.text === text) {
    return
  }
  node.text = text
  if (!node.mounted || !node.root) {
    return
  }
  if (text !== "") {
    node.root.mutations.setText(node.id, text)
    return
  }
  // Back to a placeholder: the native element goes, the host node stays.
  node.root.pendingDestroys.push(node.id)
  node.mounted = false
  node.id = 0
}

/**
 * Drop a subtree's native elements and keep its host nodes.
 *
 * ilha owns the lifetime of a hole and says so only for some of them
 * (`ops.disconnect`). So a subtree that left the view is unmounted, not
 * destroyed: it mounts again from its own props if ilha places it again, and
 * is collected with its fiber if it does not. Native destroy is
 * recursive, so only the topmost native element of the subtree is queued.
 */
function unmountNode(node: HostNode, emit: boolean): void {
  const root = node.root
  const native = node.mounted && !node.transparent
  if (root && node.id !== 0) {
    unregisterEventHandlers(root.eventHandlers, node.id)
    if (native && emit) {
      root.pendingDestroys.push(node.id)
    }
  }
  if (node.kind === "element") {
    DIRTY.delete(node)
    node.nativeChildren = []
    for (const child of node.children) {
      if (child.parent === node) {
        unmountNode(child, emit && !native)
      }
    }
  }
  node.mounted = false
  node.id = 0
  node.root = null
}

/** ilha closed the fiber that painted into `node`: it never comes back. */
export function destroyNode(node: HostNode | null | undefined): void {
  if (!node || node.destroyed) {
    return
  }
  const parent = node.parent
  unmountNode(node, true)
  node.destroyed = true
  node.parent = null
  if (node.kind === "element") {
    node.children = []
  }
  if (parent) {
    parent.children = parent.children.filter((child) => child !== node)
    markDirty(parent)
  }
  scheduleCommit()
}

// ── native tree ───────────────────────────────────────────────────────

/** A container's native children: slot hosts contribute theirs, not themselves. */
function flatten(parent: HostParent, into: HostNode[] = []): HostNode[] {
  for (const child of parent.children) {
    if (child.destroyed) {
      continue
    }
    if (child.kind === "element" && child.transparent) {
      flatten(child, into)
    } else if (isNative(child)) {
      into.push(child)
    }
  }
  return into
}

function customPropAllowed(node: HostElement, name: string): boolean {
  return !BUILT_IN_TYPES.has(node.type) || UNIVERSAL_PROPS.has(name)
}

/** GPUIX event types the element's listeners need, with a dev warning for the rest. */
function eventTypesOf(node: HostElement): Set<string> {
  const types = new Set<string>()
  for (const type of node.listeners.keys()) {
    const gpuixType = gpuixEventTypeFor(type)
    if (gpuixType) {
      types.add(gpuixType)
    } else {
      warnOnce(
        `unsupported-event:${type}`,
        `@gpuix/ilha: on${type} is not an event GPUIX emits, so it never fires.`
      )
    }
  }
  return types
}

/**
 * One native handler per GPUIX event type. Several ilha types can share one
 * (`oninput` and `onchange` are both `change`), so it fans out to all of them.
 */
function dispatcherFor(node: HostElement, gpuixType: string): HostEventHandler {
  return (payload) => {
    if (gpuixType === "change" && payload.value !== undefined) {
      // The native editor already holds this value; do not send it back.
      node.value = payload.value
      node.sent.props.set("value", payload.value)
    }
    for (const [type, listeners] of node.listeners) {
      if (gpuixEventTypeFor(type) !== gpuixType) {
        continue
      }
      if (type === "contextmenu" && payload.isRightClick !== true) {
        continue
      }
      const event = hostEventFor(type, node, payload)
      for (const listener of listeners) {
        listener(event)
      }
    }
  }
}

/** Send what differs between the element and what its native element holds. */
function patchElement(node: HostElement): void {
  const root = node.root
  if (!root || !node.mounted || node.destroyed || node.transparent) {
    return
  }
  const { sent } = node
  if (!sameValue(sent.style, node.style)) {
    root.mutations.setStyle(node.id, node.style)
    sent.style = node.style
  }

  const props = new Map<string, unknown>()
  for (const [name, value] of node.props) {
    if (customPropAllowed(node, name)) {
      props.set(name, value)
    }
  }
  // Form controls own `value`: it is bound through `setFormControl`, so an
  // atom handle never reaches the wire as a function.
  if (node.value !== null && customPropAllowed(node, "value")) {
    props.set("value", node.value)
  }
  for (const [name, value] of props) {
    if (!sent.props.has(name) || !sameValue(sent.props.get(name), value)) {
      root.mutations.setCustomProp(
        node.id,
        name,
        serializeCustomProp(value as CustomPropValue)
      )
    }
  }
  for (const name of sent.props.keys()) {
    if (!props.has(name)) {
      root.mutations.setCustomProp(node.id, name, null)
    }
  }
  sent.props = props

  const events = eventTypesOf(node)
  for (const type of events) {
    // Re-registered on every patch: the handler closes over this host node.
    registerEventHandler(
      root.eventHandlers,
      node.id,
      type,
      dispatcherFor(node, type)
    )
    if (!sent.events.has(type)) {
      root.mutations.setEventListener(node.id, type, true)
    }
  }
  for (const type of sent.events) {
    if (!events.has(type)) {
      unregisterEventHandler(root.eventHandlers, node.id, type)
      root.mutations.setEventListener(node.id, type, false)
    }
  }
  sent.events = events
}

/** Structural equality for the JSON values a style or custom prop holds. */
function sameValue(left: unknown, right: unknown): boolean {
  if (Object.is(left, right)) {
    return true
  }
  if (
    typeof left !== "object" ||
    typeof right !== "object" ||
    left === null ||
    right === null
  ) {
    return false
  }
  if (Array.isArray(left) || Array.isArray(right)) {
    return (
      Array.isArray(left) &&
      Array.isArray(right) &&
      left.length === right.length &&
      left.every((item, index) => sameValue(item, right[index]))
    )
  }
  const leftKeys = Object.keys(left)
  if (leftKeys.length !== Object.keys(right).length) {
    return false
  }
  return leftKeys.every(
    (key) =>
      Object.hasOwn(right, key) &&
      sameValue(
        (left as Record<string, unknown>)[key],
        (right as Record<string, unknown>)[key]
      )
  )
}

/** Create a node's native element, and its subtree, in document order. */
function mountNative(node: HostNode, root: HostRoot): void {
  node.root = root
  node.id = root.allocateId()
  node.mounted = true
  if (node.kind === "text") {
    root.mutations.createElement(node.id, "text")
    root.mutations.setText(node.id, node.text)
    return
  }
  root.mutations.createElement(node.id, node.type)
  node.sent = emptySent()
  patchElement(node)
  const children = flatten(node)
  for (const child of children) {
    if (!child.mounted) {
      mountNative(child, root)
    }
    root.mutations.appendChild(node.id, child.id)
  }
  node.nativeChildren = children.map((child) => child.id)
  DIRTY.delete(node)
}

/**
 * Make one container's native child order match the host tree.
 *
 * Walks backwards so every move is an `insertBefore` an already placed sibling.
 * A child that is already in front of the placed tail is left alone, so an
 * unchanged list sends nothing and an append sends one mutation.
 */
function syncChildren(container: HostParent, root: HostRoot): void {
  const children = flatten(container)
  for (const child of children) {
    if (!child.mounted) {
      mountNative(child, root)
    }
  }
  const ids = children.map((child) => child.id)
  const wanted = new Set(ids)
  const current = container.nativeChildren.filter((id) => wanted.has(id))
  const moved = new Set<number>()
  let cursor = current.length - 1
  let before: number | null = null
  for (let index = ids.length - 1; index >= 0; index -= 1) {
    const id = ids[index] as number
    while (cursor >= 0 && moved.has(current[cursor] as number)) {
      cursor -= 1
    }
    if (cursor >= 0 && current[cursor] === id) {
      cursor -= 1
    } else {
      moved.add(id)
      if (before === null) {
        root.mutations.appendChild(container.id, id)
      } else {
        root.mutations.insertBefore(container.id, id, before)
      }
    }
    before = id
  }
  container.nativeChildren = ids
}

function syncNative(root: HostRoot): void {
  for (const container of DIRTY) {
    if (container.kind === "root") {
      if (container !== root) {
        continue
      }
    } else if (!container.mounted) {
      // Mounted by its parent's sync below, or never attached.
      continue
    } else if (container.root !== root) {
      continue
    }
    DIRTY.delete(container)
    syncChildren(container, root)
  }
  // What is still unmounted belongs to a paint that never reached the tree.
  for (const container of DIRTY) {
    if (container.kind === "element" && !container.mounted) {
      DIRTY.delete(container)
    }
  }
}

// ── paint ops ─────────────────────────────────────────────────────────

/**
 * Make a parent's children exactly `nodes`.
 *
 * ilha hands over the complete child list of every root it paints, fresh host
 * nodes mixed with the hosts of holes that survived. The host tree is updated
 * here; the native tree follows at the next commit.
 */
export function placeChildrenNode(parent: HostParent, nodes: HostNode[]): void {
  setChildren(parent, nodes, parent.children, parent)
  scheduleCommit()
}

export function setHostStyle(element: HostElement, css: string): void {
  element.style = parseStyleText(css)
  if (element.mounted) {
    patchElement(element)
    scheduleCommit()
  }
}

export function setHostFormControl(
  element: HostElement,
  key: string,
  value: unknown
): void {
  // GPUIX has no `checked` / `selected`; only editors hold a value.
  if (key !== "value" || element.destroyed) {
    return
  }
  element.value = value === null || value === undefined ? "" : String(value)
  if (element.mounted) {
    patchElement(element)
    scheduleCommit()
  }
}

/** DOM-only props: GPUIX has no CSS classes and no `checked` / `selected`. */
const IGNORED_PROP_NAMES: ReadonlySet<string> = new Set([
  "checked",
  "class",
  "className",
  "htmlFor",
  "selected",
])

/**
 * Take one prop with the value the component wrote.
 *
 * ilha would otherwise write every prop as a string attribute and a `style`
 * object as CSS text. GPUIX props are objects (`highlight`, `motion`, `theme`,
 * `anchored` position), and a `StyleDesc` nests `hover` / `active` /
 * `focusVisible` styles that CSS text cannot hold. Returning `false` leaves the
 * prop to ilha: a `style` string is parsed in `setHostStyle`, and `value` goes
 * through `setHostFormControl` so an atom handle stays bound.
 */
export function setHostProp(
  element: HostElement,
  name: string,
  value: PropValue
): boolean {
  if (name === "value") {
    return false
  }
  if (name === "style") {
    if (value === null || typeof value !== "object") {
      return false
    }
    element.style = value as StyleDesc
  } else if (
    !IGNORED_PROP_NAMES.has(name) &&
    typeof value !== "function" &&
    value !== undefined
  ) {
    element.props.set(name, value)
  }
  if (element.mounted) {
    patchElement(element)
    scheduleCommit()
  }
  return true
}

/** The GPUIX painter contract ilha's `createRenderer` drives. */
export const gpuixOps: PaintOps<HostNode, HostElement> = {
  // `createRenderer` places with `placeChildren` only; this serves a caller
  // that drives `Painter.insert` by hand.
  appendRoot: (root, node) => {
    const parent = root as unknown as HostParent
    placeChildrenNode(parent, [...parent.children, node])
  },
  asElement: (element) => element as unknown as Element,
  asNode: (host) => host as unknown as HostNode,
  asRoot: (element) => element as unknown as ParentNode,
  bindEvents: (element, props, fiber) => {
    bindIlhaEvents(element as never, props, fiber)
  },
  committed: (element) => element,
  createElement: (tag) => createHostElement(tag),
  createRaw: () => {
    throw new Error(
      "@gpuix/ilha: unsafe() paints raw HTML, which has no GPUIX equivalent. " +
        "Render the content with GPUIX elements instead."
    )
  },
  createSlotHost: () => {
    const host = createHostElement("div")
    host.transparent = true
    return host
  },
  createText: (text) => createHostText(text),
  disconnect: (element) => {
    destroyNode(element)
  },
  placeChildren: (root, nodes) => {
    placeChildrenNode(root as unknown as HostParent, nodes)
  },
  reuseNode: (host) => host as unknown as HostNode,
  setFormControl: (element, key, value) => {
    setHostFormControl(element, key, value)
  },
  setProp: (element, key, value) => setHostProp(element, key, value),
  setStyle: (element, css) => {
    setHostStyle(element, css)
  },
}
