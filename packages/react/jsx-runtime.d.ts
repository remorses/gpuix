/// GPUIX JSX runtime types — maps intrinsic elements to GPUIX Props
/// instead of DOM types. Activated via "jsxImportSource": "@gpuix/react".

import type { AnchoredProps, ImgProps, Props, InputProps } from "./dist/types/host"

export { jsx, jsxs, Fragment } from "react/jsx-runtime"

export namespace JSX {
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
    svg: Props
    canvas: Props
    input: InputProps
    anchored: AnchoredProps
  }
}
