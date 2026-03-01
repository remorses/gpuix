/// NodeDisplay — implements gpui::PlatformDisplay for the Node.js environment.
///
/// Returns fixed screen bounds. Could later be enhanced to query the actual
/// display info from the OS, but for now a sensible default (1920x1080) is fine.
///
/// Reference: gpui_web/src/display.rs (98 lines)
use gpui::{px, Bounds, DisplayId, Pixels, PlatformDisplay, Point, Size};

#[derive(Debug)]
pub struct NodeDisplay {
    id: DisplayId,
    uuid: uuid::Uuid,
    bounds: Bounds<Pixels>,
}

impl NodeDisplay {
    pub fn new() -> Self {
        Self {
            id: DisplayId::new(1),
            uuid: uuid::Uuid::new_v4(),
            bounds: Bounds {
                origin: Point::default(),
                size: Size {
                    width: px(1920.),
                    height: px(1080.),
                },
            },
        }
    }
}

impl PlatformDisplay for NodeDisplay {
    fn id(&self) -> DisplayId {
        self.id
    }

    fn uuid(&self) -> anyhow::Result<uuid::Uuid> {
        Ok(self.uuid)
    }

    fn bounds(&self) -> Bounds<Pixels> {
        self.bounds
    }

    fn visible_bounds(&self) -> Bounds<Pixels> {
        self.bounds
    }

    fn default_bounds(&self) -> Bounds<Pixels> {
        // Default window: 75% of screen, centered
        let width = self.bounds.size.width * 0.75;
        let height = self.bounds.size.height * 0.75;
        let origin_x = (self.bounds.size.width - width) / 2.0;
        let origin_y = (self.bounds.size.height - height) / 2.0;
        Bounds {
            origin: Point::new(origin_x, origin_y),
            size: Size { width, height },
        }
    }
}
