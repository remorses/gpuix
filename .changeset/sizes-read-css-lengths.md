---
"@gpuix/native": minor
"@gpuix/react": minor
---

Read `width` and `height` the way CSS reads them.

`width`, `height`, `minWidth`, `minHeight`, `maxWidth` and `maxHeight` used to
take a number, a percentage or `"auto"` and nothing else, so `"200px"`,
`"6rem"`, `"calc(100px + 2rem)"` and `"var(--size)"` were all rejected. They now
go through the same length parser as `padding`, `gap` and `fontSize`, and keep
the percentage and `"auto"` they always took.

A value the parser cannot read used to throw out of `setStyle` and lose every
other property written in the same commit, so one bad size painted an element
with no style at all. It now drops the one declaration and leaves the rest
alone, which is what a browser does with a declaration it cannot parse.

The values resolve when the style resolves rather than when it is read off the
wire, so a size can name a custom property and follow it when the property
changes.
