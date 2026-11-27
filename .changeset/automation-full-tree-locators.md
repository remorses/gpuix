---
"@gpuix/native": minor
"@gpuix/react": minor
---

Automation: `locator.allInTree()` / `elementInTree()` read the retained tree instead of the last painted frame, so locators find and read elements that are below the fold, clipped by `overflow`, or outside a virtual list's window. `textContent()` now resolves through the retained tree too, matching what it already claimed to do — reading text does not require paint.

Closes #143's text half: `getByText` and `getByTestId` matches were always present in the tree, but the only paint-independent reads were the raw `getAllText()` string array and hand-walking `getTree` results that live mode returned as `null`. Wire-level change is one new validated method, `getFullTree`, which mirrors `getTree` without forcing a render; the stdio and in-process backends both serve it.
