/// GPUIX JSX runtime types — maps intrinsic elements to GPUIX props instead of
/// DOM types. Activated with "jsxImportSource": "@gpuix/ilha".
///
/// The factory is ilha's `h()`, so a component still returns ilha vnodes; only
/// the element and prop shapes are GPUIX's.

import type { View } from "ilha"
import type {
  AnchoredProps,
  CodeProps,
  DiffProps,
  HostProps,
  ImgProps as NativeImgProps,
  InputProps as NativeInputProps,
  MarkdownProps,
  SvgProps as NativeSvgProps,
  TextareaProps as NativeTextareaProps,
  VirtualListProps,
} from "@gpuix/native/host"
import type { ElementProps, FormProps } from "./dist/types.js"

export { Fragment, h, jsx, jsxDEV, jsxs } from "ilha/jsx-runtime"

export namespace JSX {
  export type Element = View
  export type ElementType =
    | string
    | ((
        props: never
      ) =>
        | View
        | Promise<View | undefined>
        | Generator<View, View | undefined, View>
        | undefined)

  export interface ElementChildrenAttribute {
    children: View
  }

  export interface IntrinsicAttributes {
    key?: string | number | null
  }

  export interface IntrinsicElements {
    // GPUIX elements
    div: ElementProps<HostProps>
    text: ElementProps<HostProps>
    img: ElementProps<NativeImgProps>
    svg: ElementProps<NativeSvgProps>
    canvas: ElementProps<HostProps>
    input: FormProps<NativeInputProps>
    textarea: FormProps<NativeTextareaProps>
    anchored: ElementProps<AnchoredProps>
    code: ElementProps<CodeProps>
    diff: ElementProps<DiffProps>
    markdown: ElementProps<MarkdownProps>
    "virtual-list": ElementProps<VirtualListProps>

    // HTML tags GPUIX renders as a <div>, so markup ports unchanged
    a: ElementProps<HostProps>
    article: ElementProps<HostProps>
    aside: ElementProps<HostProps>
    blockquote: ElementProps<HostProps>
    button: ElementProps<HostProps>
    em: ElementProps<HostProps>
    footer: ElementProps<HostProps>
    form: ElementProps<HostProps>
    h1: ElementProps<HostProps>
    h2: ElementProps<HostProps>
    h3: ElementProps<HostProps>
    h4: ElementProps<HostProps>
    h5: ElementProps<HostProps>
    h6: ElementProps<HostProps>
    header: ElementProps<HostProps>
    label: ElementProps<HostProps>
    li: ElementProps<HostProps>
    main: ElementProps<HostProps>
    nav: ElementProps<HostProps>
    ol: ElementProps<HostProps>
    p: ElementProps<HostProps>
    pre: ElementProps<HostProps>
    section: ElementProps<HostProps>
    small: ElementProps<HostProps>
    span: ElementProps<HostProps>
    strong: ElementProps<HostProps>
    table: ElementProps<HostProps>
    tbody: ElementProps<HostProps>
    td: ElementProps<HostProps>
    th: ElementProps<HostProps>
    thead: ElementProps<HostProps>
    tr: ElementProps<HostProps>
    ul: ElementProps<HostProps>
  }
}
