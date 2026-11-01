---
"@gpuix/native": minor
"@gpuix/react": minor
---

Map the remaining ARIA state, value, and relation props onto AccessKit:
`aria-keyshortcuts`, `aria-placeholder`, `aria-checked` and `aria-pressed`
(including `"mixed"`),
`aria-valuenow`, `aria-valuemin`, `aria-valuemax`, `aria-orientation`,
`aria-posinset`, `aria-setsize`, `aria-rowindex`, `aria-colindex`,
`aria-rowcount`, and `aria-colcount`. Any `aria-*` attribute is now forwarded
from built-in elements; invalid tokens and non-finite numbers are ignored.
