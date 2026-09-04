---
"@gpuix/native": minor
"@gpuix/react": minor
---

Paint scrollbars on scroll boxes, and add `scrollbar-width`, `scrollbar-color` and `scrollbar-gutter`.

A box with `overflow: scroll` or `overflow: auto` now gets a scrollbar
on each axis it scrolls. The OS picks the kind of bar, as a browser does.
When the OS auto-hides scrollbars, an overlay bar floats over the content,
shows for a second after a scroll and fades out, and reserves no space.
Otherwise a classic bar sits in a 15px gutter that the layout reserves.
`overflow: scroll` keeps the classic bar at all times and `auto` shows it
only while the content overflows. A drag of the thumb scrolls, a click in
the track moves one page, and the thumb widens under the mouse.
`scrollbar-width: thin` narrows the bar and `none` removes it.
`scrollbar-color` sets the thumb and the track. `scrollbar-gutter: stable`
reserves the gutter of a classic bar even while the content fits, and
`stable both-edges` reserves one at the start of the axis too.
`overflow: auto` used to do nothing and `clip` now clips like `hidden`.
`GPUIX_SCROLLBARS=overlay` or `classic` in the environment overrides the
OS choice, for tests.

A bar paints after the whole frame, above any effect a sibling of the
content paints, so a blurred sticky header does not cover it. When one
axis of `overflow` computes to `visible` or `clip` and the other axis
scrolls, the first becomes `auto` or `hidden`, as in CSS.

`scrollIntoView(elementId, block, inline)` on the renderer scrolls every
scroll box around an element until the element shows. `block` and
`inline` take `start`, `center`, `end` or `nearest`, with the web
defaults. `scroll-margin` on the target keeps space around it, and
`scroll-padding` on a scroll box keeps space inside the box, each as one
value or as the one-to-four shorthand.
