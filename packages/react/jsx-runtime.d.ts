/// GPUIX JSX runtime types. Maps intrinsic elements to GPUIX Props instead of
/// DOM types. Turned on with "jsxImportSource": "@gpuix/react".

import type {
  AnchoredProps,
  CodeProps,
  DiffProps,
  ImgProps,
  InputProps,
  MarkdownProps,
  Props,
  SvgProps,
  TextareaProps,
  VirtualListProps,
} from "./dist/types/host.js"

export { jsx, jsxs, Fragment } from "react/jsx-runtime"

export namespace JSX {
  type ElementType = React.JSX.ElementType
  type Element = React.JSX.Element
  type ElementClass = React.JSX.ElementClass
  type ElementAttributesProperty = React.JSX.ElementAttributesProperty
  type ElementChildrenAttribute = React.JSX.ElementChildrenAttribute
  type IntrinsicAttributes = React.JSX.IntrinsicAttributes
  type IntrinsicClassAttributes<T> = React.JSX.IntrinsicClassAttributes<T>

  /// The props one built-in tag takes.
  ///
  /// TypeScript reads `IntrinsicAttributes` for a component tag but not for a
  /// built-in one, so `key` has to sit in the props of each tag. React does the
  /// same for every DOM tag through `ClassAttributes`.
  type Tag<P> = P & IntrinsicAttributes

  interface IntrinsicElements {
    div: Tag<Props>
    text: Tag<Props>
    img: Tag<ImgProps>
    svg: Tag<SvgProps>
    canvas: Tag<Props>
    input: Tag<InputProps>
    textarea: Tag<TextareaProps>
    anchored: Tag<AnchoredProps>
    code: Tag<CodeProps>
    diff: Tag<DiffProps>
    markdown: Tag<MarkdownProps>
    "virtual-list": Tag<VirtualListProps>
  }
}
