---
"@gpuix/native": patch
"@gpuix/react": patch
---

Index and child selector conditions from a class

A class resolver can now return `selectors`, a list of `{ on, style }` rules.
The spellings form a closed set. `:first-child`, `:last-child`,
`:nth-child(odd)`, `:nth-child(even)` and `:only-child` read the position of
the element among its siblings. The walk knows that position at build time, so
they cost no event and no measurement. `& > *` and `& > :not(:last-child)` sit
on the parent and style its direct children, which is what `space-y-*` and
`divide-*` compile to. `& *` reaches the whole subtree. An unknown spelling
warns once and drops.

Every rule applies with specificity zero, as `:where()` does on the web. A
declaration the child makes itself wins over a rule from the parent, and the
`style` prop wins over an index rule key by key. Two tokens on the same
selector merge into one rule, the later token winning. The `style` prop type
excludes `selectors`, because a style attribute holds declarations, not
selectors.

Two places sit outside the tree walk. A virtual-list row builds on its own, so
it has no child position and the index conditions do not apply to it. A custom
element resolves its own style, so the rules of a parent stop at its border.
Verified on macOS. The Windows and Linux paths did not run here.
