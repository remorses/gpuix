---
'@gpuix/native': patch
---

Terminal mode over SSH now sends only the parts of the screen that changed, in
terminals that can edit a shown image (kitty, Ghostty 1.4+). A hover or a
keystroke is a few KB instead of the whole frame (about 120 KB at 1600x960), so
a slow link keeps a much higher frame rate. Other terminals still get whole
frames. In the first group, a frame with no visible change sends nothing.
