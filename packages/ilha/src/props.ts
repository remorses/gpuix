import type { DimensionValue, ElementType, StyleDesc } from "@gpuix/native/host"

/**
 * JSX tag → GPUIX element type.
 *
 * ilha writes markup; GPUIX has a fixed set of native elements. Every HTML tag
 * without a native counterpart becomes a `<div>`, so a component's structure
 * survives while styling stays GPUIX's job (there are no CSS classes).
 */
const ELEMENT_TYPE_BY_TAG: Readonly<Record<string, ElementType>> = {
  div: "div",
  span: "div",
  p: "div",
  a: "div",
  button: "div",
  label: "div",
  form: "div",
  section: "div",
  article: "div",
  aside: "div",
  header: "div",
  footer: "div",
  main: "div",
  nav: "div",
  ul: "div",
  ol: "div",
  li: "div",
  pre: "div",
  blockquote: "div",
  strong: "div",
  em: "div",
  small: "div",
  h1: "div",
  h2: "div",
  h3: "div",
  h4: "div",
  h5: "div",
  h6: "div",
  table: "div",
  thead: "div",
  tbody: "div",
  tr: "div",
  td: "div",
  th: "div",
  text: "text",
  img: "img",
  svg: "svg",
  canvas: "canvas",
  input: "input",
  textarea: "textarea",
  anchored: "anchored",
  code: "code",
  diff: "diff",
  markdown: "markdown",
  "virtual-list": "virtual-list",
}

export function elementTypeForTag(tag: string): ElementType | undefined {
  return ELEMENT_TYPE_BY_TAG[tag.toLowerCase()]
}

/** GPUIX reads these as a plain number, so a `px`/unit suffix is dropped. */
const NUMBER_STYLE_KEYS: ReadonlySet<string> = new Set([
  "padding",
  "paddingTop",
  "paddingRight",
  "paddingBottom",
  "paddingLeft",
  "margin",
  "marginTop",
  "marginRight",
  "marginBottom",
  "marginLeft",
  "gap",
  "rowGap",
  "columnGap",
  "flexGrow",
  "flexShrink",
  "flexBasis",
  "gridTemplateColumns",
  "gridTemplateRows",
  "top",
  "right",
  "bottom",
  "left",
  "opacity",
  "borderWidth",
  "borderTopWidth",
  "borderRightWidth",
  "borderBottomWidth",
  "borderLeftWidth",
  "borderRadius",
  "borderTopLeftRadius",
  "borderTopRightRadius",
  "borderBottomLeftRadius",
  "borderBottomRightRadius",
  "fontSize",
  "lineHeight",
  "outlineWidth",
  "outlineOffset",
  "lineClamp",
])

/** Sizing keys accept a number or a CSS string such as `"100%"` / `"auto"`. */
const DIMENSION_STYLE_KEYS: ReadonlySet<string> = new Set([
  "width",
  "height",
  "minWidth",
  "minHeight",
  "maxWidth",
  "maxHeight",
])

const numberOrUndefined = (raw: string): number | undefined => {
  const parsed = Number.parseFloat(raw)
  return Number.isFinite(parsed) ? parsed : undefined
}

/**
 * Turn the CSS declaration text ilha produces into a GPUIX `StyleDesc`.
 *
 * ilha serializes a `style` prop (object or string) to `kebab-case:value`
 * pairs; GPUIX wants camelCase keys and numbers for lengths. Keys GPUIX does
 * not know are passed through and ignored natively, like any other invalid
 * style value.
 */
export function parseStyleText(css: string): StyleDesc {
  const style: Record<string, unknown> = {}
  for (const declaration of css.split(";")) {
    const colon = declaration.indexOf(":")
    if (colon < 0) {
      continue
    }
    const name = declaration
      .slice(0, colon)
      .trim()
      .replace(/-([a-z0-9])/g, (_, letter: string) => letter.toUpperCase())
    const raw = declaration.slice(colon + 1).trim()
    if (!name || !raw) {
      continue
    }
    if (DIMENSION_STYLE_KEYS.has(name)) {
      if (raw.endsWith("%") || raw === "auto") {
        style[name] = raw satisfies DimensionValue
        continue
      }
      const parsed = numberOrUndefined(raw)
      style[name] = parsed ?? raw
      continue
    }
    if (NUMBER_STYLE_KEYS.has(name)) {
      const parsed = numberOrUndefined(raw)
      if (parsed !== undefined) {
        style[name] = parsed
      }
      continue
    }
    style[name] = raw
  }
  return style as StyleDesc
}
