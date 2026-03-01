// React 19's jsx-dev-runtime only exports jsxDEV (not jsx/jsxs).
// Re-export all variants so both prod and dev transforms work.
export { jsxDEV, jsxDEV as jsx, jsxDEV as jsxs, Fragment } from "react/jsx-dev-runtime"
