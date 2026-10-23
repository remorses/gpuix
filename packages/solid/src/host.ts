// Host-tree shape is based on jhomra21/gpuix-solid (MIT), stripped to GPUIX's
// native elements and event contract.
import {
  BUILT_IN_TYPES,
  handlerEventType,
  UNIVERSAL_PROPS,
  registerEventHandler,
  serializeCustomProp,
  unregisterEventHandler,
  unregisterEventHandlers,
} from "@gpuix/native/host"
import type {
  ElementType,
  EventHandlerMap,
  HostProps,
  ImagePixelsOptions,
  MutationRenderer,
  NativeRenderer,
  StyleDesc,
} from "@gpuix/native/host"
import type { EventPayload } from "@gpuix/native"

export type HostNode = HostElement | HostText
export type HostParent = HostRoot | HostElement
export type HostPropertyValue =
  | object
  | string
  | number
  | boolean
  | ((event: EventPayload) => void)
  | null
  | undefined

export function isHostPropertyValue<Value>(
  value: Value
): value is Value & HostPropertyValue {
  const type = typeof value
  return value == null || type === "object" || type === "string" ||
    type === "number" || type === "boolean" || type === "function"
}

export class HostRoot {
  readonly kind = "root" as const
  readonly children: HostNode[] = []

  readonly mutations: MutationRenderer
  readonly nativeRenderer: NativeRenderer
  readonly eventHandlers: EventHandlerMap
  readonly allocateId: () => number

  constructor(options: {
    mutations: MutationRenderer
    nativeRenderer: NativeRenderer
    eventHandlers: EventHandlerMap
    allocateId: () => number
  }) {
    this.mutations = options.mutations
    this.nativeRenderer = options.nativeRenderer
    this.eventHandlers = options.eventHandlers
    this.allocateId = options.allocateId
  }
}

export class HostElement {
  readonly kind = "element" as const
  readonly children: HostNode[] = []
  readonly props = new Map<string, HostPropertyValue>()
  parent: HostParent | null = null
  root: HostRoot | null = null
  id = 0
  mounted = false
  style: StyleDesc = {}

  constructor(readonly type: ElementType) {}

  scrollIntoView(): void {
    this.root?.nativeRenderer.scrollIntoView?.(this.id)
  }

  setImage(bytes: Buffer | Uint8Array): void {
    this.root?.nativeRenderer.setImage?.(this.id, bytes as Buffer)
  }

  setImagePixels(
    width: number,
    height: number,
    pixels: Buffer | Uint8Array,
    options?: ImagePixelsOptions
  ): void {
    this.root?.nativeRenderer.setImagePixels?.(
      this.id,
      width,
      height,
      pixels as Buffer,
      options?.format
    )
  }
}

export class HostText {
  readonly kind = "text" as const
  readonly children: HostNode[] = []
  parent: HostParent | null = null
  root: HostRoot | null = null
  id = 0
  mounted = false

  constructor(public text: string) {}
}

export function createHostElement(type: string): HostElement {
  if (!isElementType(type)) throw new Error(`Unsupported GPUIX element <${type}>`)
  return new HostElement(type)
}

function isElementType(type: string): type is ElementType {
  switch (type) {
    case "div":
    case "text":
    case "img":
    case "svg":
    case "canvas":
    case "input":
    case "textarea":
    case "anchored":
    case "code":
    case "diff":
    case "markdown":
    case "virtual-list":
      return true
    default:
      // Elements registered from Rust with `register_custom_element` always
      // have a `-`. Rust warns and renders nothing for a name it doesn't know.
      return type.includes("-")
  }
}

export function createHostText(value: string): HostText {
  return new HostText(String(value))
}

export function replaceHostText(node: HostText, value: string): void {
  const next = String(value)
  if (node.text === next) return
  node.text = next
  if (node.root && node.mounted) node.root.mutations.setText(node.id, next)
}

function customPropAllowed(node: HostElement, name: string): boolean {
  return !BUILT_IN_TYPES.has(node.type) || UNIVERSAL_PROPS.has(name)
}

export function setHostProperty(
  node: HostNode,
  name: string,
  value: HostPropertyValue,
  previous?: HostPropertyValue
): void {
  if (node.kind === "text" || name === "children" || name === "ref" || name === "key") {
    return
  }
  if (name === "style") {
    node.style = isStyle(value) ? value : {}
    if (node.root && node.mounted) node.root.mutations.setStyle(node.id, node.style)
    return
  }
  if (value === undefined) node.props.delete(name)
  else node.props.set(name, value)
  if (!node.root || !node.mounted) return

  // `onTop` can switch between a handler and data. Rust only holds the data
  // form, so the handler form counts as absent there.
  const eventType = handlerEventType(name, value)
  const previousType = handlerEventType(name, previous)
  if (previousType) {
    unregisterEventHandler(node.root.eventHandlers, node.id, previousType)
  }
  if (eventType) {
    registerEventHandler(
      node.root.eventHandlers,
      node.id,
      eventType,
      value as (event: EventPayload) => void
    )
  }
  if (!previousType !== !eventType) {
    node.root.mutations.setEventListener(node.id, (eventType ?? previousType)!, !!eventType)
  }

  if (!customPropAllowed(node, name)) return
  if (eventType) {
    if (previous !== undefined && !previousType) {
      node.root.mutations.setCustomProp(node.id, name, null)
    }
    return
  }
  if (previousType && value === undefined) return
  node.root.mutations.setCustomProp(
    node.id,
    name,
    serializeCustomProp(value)
  )
}

function mount(root: HostRoot, node: HostNode): void {
  if (node.root && node.root !== root) {
    throw new Error("Cannot move a GPUIX host node between roots")
  }
  node.root = root
  if (!node.id) node.id = root.allocateId()
  if (node.mounted) return
  node.mounted = true
  root.mutations.createElement(node.id, node.kind === "text" ? "text" : node.type)
  if (node.kind === "text") {
    root.mutations.setText(node.id, node.text)
  } else {
    if (Object.keys(node.style).length > 0) root.mutations.setStyle(node.id, node.style)
    for (const [name, value] of node.props) {
      const eventType = handlerEventType(name, value)
      if (eventType) {
        registerEventHandler(
          root.eventHandlers,
          node.id,
          eventType,
          value as (event: EventPayload) => void
        )
        root.mutations.setEventListener(node.id, eventType, true)
      } else if (customPropAllowed(node, name)) {
        root.mutations.setCustomProp(
          node.id,
          name,
          serializeCustomProp(value)
        )
      }
    }
  }
  for (const child of node.children) {
    mount(root, child)
    root.mutations.appendChild(node.id, child.id)
  }
}

function removeFromParent(node: HostNode): void {
  const parent = node.parent
  if (!parent) return
  const index = parent.children.indexOf(node)
  if (index >= 0) parent.children.splice(index, 1)
  node.parent = null
}

export function insertHostNode(
  parent: HostParent,
  node: HostNode,
  anchor?: HostNode | null
): void {
  if (anchor && anchor.parent !== parent) {
    throw new Error("GPUIX Solid anchor is not a child of its parent")
  }
  removeFromParent(node)
  const index = anchor ? parent.children.indexOf(anchor) : parent.children.length
  parent.children.splice(index, 0, node)
  node.parent = parent
  const root = parent.kind === "root" ? parent : parent.root
  if (!root) return
  mount(root, node)
  if (parent.kind === "root") root.mutations.setRoot(node.id)
  else if (anchor) root.mutations.insertBefore(parent.id, node.id, anchor.id)
  else root.mutations.appendChild(parent.id, node.id)
}

function markUnmounted(node: HostNode): void {
  node.mounted = false
  if (node.root) unregisterEventHandlers(node.root.eventHandlers, node.id)
  for (const child of node.children) markUnmounted(child)
}

export function removeHostNode(parent: HostParent, node: HostNode): void {
  if (node.parent !== parent) return
  removeFromParent(node)
  const root = parent.kind === "root" ? parent : parent.root
  if (!root || !node.mounted) return
  markUnmounted(node)
  root.mutations.destroyElement(node.id)
}

export function getParentNode(node: HostNode): HostParent | undefined {
  return node.parent ?? undefined
}

export function getFirstChild(parent: HostParent): HostNode | undefined {
  return parent.children[0]
}

export function getNextSibling(node: HostNode): HostNode | undefined {
  const parent = node.parent
  if (!parent) return undefined
  const index = parent.children.indexOf(node)
  return index < 0 ? undefined : parent.children[index + 1]
}

export function isHostText(node: HostNode | HostParent): node is HostText {
  return node.kind === "text"
}

/** Structural, not `instanceof`: a JSX file compiled against the published
 *  package and a component from source build nodes from two class copies. */
export function isHostElement(value: unknown): value is HostElement {
  return typeof value === "object" && value !== null &&
    (value as { kind?: unknown }).kind === "element"
}

function isStyle(value: HostPropertyValue): value is StyleDesc {
  return value !== null && typeof value === "object" && !Array.isArray(value)
}

export type SolidHostProps = HostProps & {
  children?: unknown
  ref?: (element: HostElement) => void
}
