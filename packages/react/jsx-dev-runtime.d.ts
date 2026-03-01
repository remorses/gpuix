/// GPUIX JSX dev-runtime types — mirrors jsx-runtime.d.ts for development builds.

import type { AnchoredProps, ImgProps, Props, InputProps } from "./src/types/host"

export { jsx, jsxs, Fragment } from "react/jsx-dev-runtime"

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
