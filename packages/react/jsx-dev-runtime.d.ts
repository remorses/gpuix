/// GPUIX JSX dev-runtime types — mirrors jsx-runtime.d.ts for development builds.
///
/// React 19's `react/jsx-dev-runtime` exports only `jsxDEV`, so the aliases here
/// must match jsx-dev-runtime.js instead of re-exporting `jsx` and `jsxs`.

import type * as React from "react"
import type {
  AnchoredProps,
  BrowserProps,
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

export { jsxDEV, jsxDEV as jsx, jsxDEV as jsxs, Fragment } from "react/jsx-dev-runtime"

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
    browser: BrowserProps
    code: CodeProps
    diff: DiffProps
    markdown: MarkdownProps
    shimmer: ShimmerProps
    terminal: TerminalProps
    "virtual-list": VirtualListProps
  }
}
