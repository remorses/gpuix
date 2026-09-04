---
"@gpuix/native": minor
"@gpuix/react": minor
---

Ease the mix between two gradient stops.

An `<easing-function>` between two colour stops bends the mix, following the
CSSWG proposal in csswg-drafts issue 1332: `linear-gradient(to top, black,
ease-in-out, transparent)`. `ease`, `ease-in`, `ease-out`, `ease-in-out` and
`cubic-bezier()` are read. The easing paints as the GPUI colour hint whose
curve crosses one half at the same place, so the paint agrees with the easing
at both ends and at the half-way point. A straight fade to transparent looks
dense near the solid stop and thin near the clear one. An eased one reads as
one smooth fall-off.
