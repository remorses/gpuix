import type { ElementDesc, StyleDesc, EventPayload } from "@gpuix/native"

// Element types supported by GPUIX
export type ElementType = "div" | "text" | "img" | "svg" | "canvas"

// Props passed to elements
export type Props = Record<string, unknown> & {
  id?: string
  style?: StyleDesc
  children?: React.ReactNode
  // Events
  onClick?: (event: EventPayload) => void
  onMouseDown?: (event: EventPayload) => void
  onMouseUp?: (event: EventPayload) => void
  onMouseEnter?: (event: EventPayload) => void
  onMouseLeave?: (event: EventPayload) => void
  onMouseMove?: (event: EventPayload) => void
  onKeyDown?: (event: EventPayload) => void
  onKeyUp?: (event: EventPayload) => void
  onFocus?: (event: EventPayload) => void
  onBlur?: (event: EventPayload) => void
  onScroll?: (event: EventPayload) => void
  // Focus
  tabIndex?: number
  tabStop?: boolean
  autoFocus?: boolean
}

// Container is the root renderer
export interface Container {
  render(tree: ElementDesc): void
  requestRender(): void
}

// Instance represents a GPUIX element in the tree
export interface Instance {
  id: string
  type: ElementType
  props: Props
  children: Instance[]
  parent: Instance | null
  textContent?: string
}

// Text instance for raw text nodes
export interface TextInstance {
  id: string
  text: string
  parent: Instance | null
}

// Public instance exposed via refs
export type PublicInstance = Instance

// Host context passed down the tree
export interface HostContext {
  isInsideText: boolean
}
