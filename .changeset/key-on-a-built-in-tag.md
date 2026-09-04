---
"@gpuix/react": patch
---

Accept `key` on a built-in tag under `jsxImportSource: "@gpuix/react"`.

`<text key={id} />` failed to typecheck. TypeScript reads
`JSX.IntrinsicAttributes` for a component tag but not for a built-in one, so
`key` has to sit in the props of each tag. React does the same for every DOM
tag. The props of a tag stay closed, so a name that is not a prop is still an
error.

`jsx-runtime.d.ts` also imported its types with no file extension, which
`moduleResolution: "nodenext"` cannot resolve. Under `skipLibCheck` that import
became `any` and every tag took any prop at all. The import now names the file.
