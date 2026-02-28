import type { EventPayload, GpuixRenderer } from "@gpuix/native"

export type DimensionValue = number | string

export interface StyleDesc {
  display?: string
  visibility?: string
  flexDirection?: string
  flexWrap?: string
  flexGrow?: number
  flexShrink?: number
  flexBasis?: number
  alignItems?: string
  alignSelf?: string
  alignContent?: string
  justifyContent?: string
  gap?: number
  rowGap?: number
  columnGap?: number

  width?: DimensionValue
  height?: DimensionValue
  minWidth?: DimensionValue
  minHeight?: DimensionValue
  maxWidth?: DimensionValue
  maxHeight?: DimensionValue

  padding?: number
  paddingTop?: number
  paddingRight?: number
  paddingBottom?: number
  paddingLeft?: number

  margin?: number
  marginTop?: number
  marginRight?: number
  marginBottom?: number
  marginLeft?: number

  position?: string
  top?: number
  right?: number
  bottom?: number
  left?: number

  background?: string
  backgroundColor?: string
  color?: string
  opacity?: number

  borderWidth?: number
  borderColor?: string
  borderRadius?: number
  borderTopLeftRadius?: number
  borderTopRightRadius?: number
  borderBottomLeftRadius?: number
  borderBottomRightRadius?: number

  fontSize?: number
  fontWeight?: string
  textAlign?: string
  lineHeight?: number

  overflow?: string
  overflowX?: string
  overflowY?: string

  cursor?: string
}

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

// Container holds the native renderer reference.
// Mutations go directly via napi calls, no JSON serialization.
export interface Container {
  renderer: GpuixRenderer
}

// Instance — minimal handle for React's reconciler.
// The real element state lives in Rust's RetainedTree.
export interface Instance {
  id: number
  type: ElementType
  props: Props
}

// Text instance for raw text nodes
export interface TextInstance {
  id: number
  text: string
  parentId: number | null
}

// Public instance exposed via refs
export type PublicInstance = Instance

// Host context passed down the tree
export interface HostContext {
  isInsideText: boolean
}
