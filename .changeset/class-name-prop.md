---
"@gpuix/react": minor
---

Add a `className` prop and a resolver seam for it.

`className` is `string | undefined` on every element, so `clsx` and `cn` need no
special handling. GPUIX ships no resolver. A root registers one:

```ts
createRoot(renderer, { resolveClassName })
```

The resolver reads one class token, such as `p-4`, and returns the `StyleDesc`
it declares, or `null` for a token it does not know. `@gpuix/tailwind` will be
one. Without a resolver a `className` does nothing and the root warns once.

A declaration in `style` beats one from a class, key by key, and in the hover
and active states too. [CSS Style Attributes][spec] gives the attribute "a
specificity higher than any selector", so an element with
`style={{ backgroundColor: "red" }}` stays red under a `hover:bg-blue-500`
class, the way a browser keeps it red.

Caching is per token. `clsx("p-4", a && "bg-blue-500", b && "text-lg")` writes up
to eight strings from three tokens, and the resolver sees three. A bounded cache
over whole strings sits in front of it, so a repeated string skips both the split
and the merge.

`hideInstance` and `unhideInstance` now send the class-derived style as well.
React drives that pair for Suspense, and before this an element that suspended
came back with only its inline style.

[spec]: https://www.w3.org/TR/css-style-attr/#cascading
