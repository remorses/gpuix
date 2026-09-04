---
"@gpuix/native": minor
"@gpuix/react": minor
---

Add `scroll-behavior`, `scroll-snap-*`, `scroll-initial-target` and the
logical `scroll-margin` and `scroll-padding` variants.

`scroll-behavior: smooth` turns a programmatic scroll into a glide on the
offset. `scrollTo` and `scrollIntoView` also take a `behavior` argument,
`auto`, `instant` or `smooth`, like the web option. A wheel move that takes
the box away from the glide cancels it.

`scroll-snap-type` on a box and `scroll-snap-align` on its descendants snap
the box when a scroll comes to rest. `mandatory` always snaps to the nearest
position. `proximity`, the default, snaps within half a viewport.
`scroll-snap-stop: always` on an area stops a long scroll that would pass
over it. The snap area grows by the `scroll-margin` of the element, and the
viewport shrinks by the `scroll-padding` of the box, as in CSS.

`scroll-initial-target: nearest` scrolls the ancestors of an element to it
once, on the first frame after it paints.

`scroll-margin` and `scroll-padding` take the logical variants: `-block`,
`-block-start`, `-block-end`, `-inline`, `-inline-start` and `-inline-end`.
GPUIX lays text out horizontally, left to right, so block is vertical and
inline is horizontal.

`scroll-timeline` (and the `-name` and `-axis` longhands) on a scroll box
publishes a timeline, and `animation-timeline` on an element points its
`motion` at one: a `--name`, or the anonymous
`scroll(nearest | self | root, axis)`. GPUIX has no `@keyframes`, so the
`motion` prop is the keyframes source: `initial` is 0%, `animate` is 100%,
and `transition.ease` bends the progress. Duration and delay play no part,
because the scroll offset is the clock. Two divergences from CSS, on
purpose: a missing or `none` value keeps the clock instead of freezing the
animation, and a `--name` no box declares holds the progress at 0.

`scroll-marker-group: before | after` on a scroll box adds a group of
markers along that edge, one per snap area. The marker of the area nearest
the current offset paints stronger, and a click on a marker scrolls to its
area, with a glide when `scroll-behavior: smooth` applies. GPUIX has no
pseudo-elements, so the markers are round dots rather than
`::scroll-marker` content.

`scrollIntoView` takes the `container` option from CSSOM View: `"nearest"`
scrolls only the nearest scroll box, and `"all"`, the default, scrolls
every ancestor.

`scrollbar-color` now resolves `var()` and the other cascade colours, the
way every other colour property does.

Snap fires sooner: the idle window is 80ms, and the sub-pixel tail of a
wheel no longer resets it, so the glide starts while the wheel coasts.

`<virtual-list>` paints a scrollbar. The bar reads the list state for
the offset and the measured content height, so the thumb length settles
as rows measure. A classic bar reserves its gutter as right padding, and
the rows shrink by it. `scrollbar-width`, `scrollbar-gutter` and
`scrollbar-color` on the list apply as on a div. The list also lays its
rows out inside horizontal padding now, which it ignored before.

A trackpad fling snaps the moment the fingers lift, as a browser does.
The box predicts the landing point from the last 100ms of wheel deltas,
picks the snap position for that landing, and glides straight to it. The
OS momentum stream after the lift cannot cancel the glide, because the
box consumes it. A mouse wheel has no touch phases, so it keeps the 80ms
idle watcher.

The fling glide moves along Chromium's snap fling curve
(`cc/input/snap_fling_curve.cc`): each 16ms frame covers 0.92 of what
the frame before it covered, so the glide starts fast and slows down
like momentum, and a longer distance takes more time. A new touch on
the box stops a running glide at once. Programmatic smooth scrolls keep
the 300ms `easeInOut` glide.

The box consumes the whole momentum stream after a fling, not only the
part that overlaps the glide. The stream outlives a short glide by a
second or more, and the tail pushed the box off the snap position the
moment the glide ended. A gap of more than 100ms ends the stream, so a
later mouse wheel still scrolls.

The box never snaps while the fingers stay on the pad, however long
they rest. The web snaps at the end of the gesture, not during it. The
lift after a rest has zero velocity, so the box then snaps to the
nearest position.

The `scroll-snap-stop: always` scan for a fling starts at the offset of
the lift, not at the offset where the fingers went down. An `always`
area the drag already passed pulled the box backward at the lift, and
the consumed momentum stream then held it there. Blink measures the
fling from its start point, and now GPUIX does too.

The lift now asks the window for a paint. The glide moves one step per
painted frame, and the consumed momentum events after the lift schedule
no paint of their own, so without this first frame the glide sat inert
and the fling stopped dead at the lift. `GPUIX_SNAP_DEBUG=1` logs the
gesture stream, the landing prediction and each glide step.
