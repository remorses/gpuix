---
"@gpuix/native": minor
"@gpuix/react": minor
---

Add the View Transitions API.

`startViewTransition(renderer, update, options)` captures every element that
carries a `viewTransitionName`, applies the React update synchronously, and
animates each name from its old place to its new one. The renderer clones the
named subtrees before the update and paints the frozen copies over the live
tree while the transition runs, so the leaving screen stays visible under, or
over, the arriving one.

Options take a duration, a delay, and an ease per name, plus `translateX`,
`translateY`, `opacity` and `blur` ranges for the old side and the new side.
Percent lengths resolve against the size of the named element, so
`translateX: ["100%", "0%"]` slides a screen in from the right at any width.
A name with no options crossfades. A name that only enters animates against
its own bounds. A name that only leaves paints its frozen copy over the tree
while the `old` side runs, without the clip of its former ancestors.

The `motion` prop takes a `blur` field too: a `filter: blur()` sigma in
pixels that interpolates like `opacity`.

The new side moves through the motion channel, so the live element and its
hitboxes move together, and input lands where the screen paints. The frozen
copy takes fresh ids where the live tree still uses them, so a surviving
element and its copy never share GPUI element state.

Limits in this version: the frozen copy takes no input.
