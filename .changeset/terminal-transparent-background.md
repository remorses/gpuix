---
"@gpuix/native": minor
---

Terminal mode honours `windowBackground: 'transparent'` and `'blurred'`. Frames
keep their alpha, converted to the straight RGBA the kitty graphics protocol
expects, so a terminal with its own background opacity and blur (kitty
`background_blur`, Ghostty `background-blur`) shows the blurred wallpaper
behind the app. `'opaque'` now sends opaque pixels on Linux too; unpainted
areas used to come through as transparent there.
