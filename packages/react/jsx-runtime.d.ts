/// GPUIX JSX runtime types — maps intrinsic elements to GPUIX Props
/// instead of DOM types. Activated via "jsxImportSource": "@gpuix/react".
///
/// `key` is declared on `Props`, not on `IntrinsicAttributes` below.
/// TypeScript 5 ignores `IntrinsicAttributes` for intrinsic elements.

import type * as React from "react"
import type {
  AnchoredProps,
  CodeProps,
  DiffProps,
  ImgProps,
  InputProps,
  MarkdownProps,
  Props,
  ShimmerProps,
  SvgProps,
  TerminalProps,
  TextareaProps,
  VirtualListProps,
} from "./dist/types/host"

export { jsx, jsxs, Fragment } from "react/jsx-runtime"

export namespace JSX {
  type ElementType = React.JSX.ElementType
  type Element = React.JSX.Element
  type ElementClass = React.JSX.ElementClass
  type ElementAttributesProperty = React.JSX.ElementAttributesProperty
  type ElementChildrenAttribute = React.JSX.ElementChildrenAttribute
  type IntrinsicAttributes = React.JSX.IntrinsicAttributes
  type IntrinsicClassAttributes<T> = React.JSX.IntrinsicClassAttributes<T>

  interface IntrinsicElements {
    div: Props
    text: Props
    img: ImgProps
    svg: SvgProps
    canvas: Props
    input: InputProps
    textarea: TextareaProps
    anchored: AnchoredProps
    code: CodeProps
    diff: DiffProps
    markdown: MarkdownProps
    shimmer: ShimmerProps
    terminal: TerminalProps
    "virtual-list": VirtualListProps
  }
}
