use std::{
    collections::HashMap,
    sync::{Arc, Mutex, OnceLock},
};

use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use serde::Deserialize;

use super::{CustomElement, CustomElementFactory, CustomRenderContext};

const CELL_BYTES: usize = 16;
const CELL_WIDE: u16 = 1 << 0;
const CELL_SPACER: u16 = 1 << 1;
const CELL_BOLD: u16 = 1 << 2;
const CELL_ITALIC: u16 = 1 << 3;
const CELL_UNDERLINE: u16 = 1 << 4;
const CELL_STRIKE: u16 = 1 << 5;
const CELL_FILL: u16 = 1 << 6;
const CELL_NERD_FONT: u16 = 1 << 7;
const GRAPHEME_INDEX: u32 = 0x8000_0000;
const SHAPE_CACHE_LIMIT: usize = 4096;

pub struct TerminalFactory;

impl CustomElementFactory for TerminalFactory {
    fn element_type(&self) -> &str {
        "terminal"
    }

    fn create(&self, id: u64) -> Box<dyn CustomElement> {
        Box::new(TerminalElement::new(id))
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WireFrame {
    version: u8,
    cols: usize,
    rows: usize,
    cell_width: f32,
    line_height: f32,
    font_size: f32,
    background: String,
    cursor_color: String,
    cursor_x: usize,
    cursor_y: usize,
    cursor_visible: bool,
    font_family: String,
    nerd_font_family: String,
    ligatures_enabled: bool,
    #[serde(default)]
    cells: String,
    #[serde(default)]
    graphemes: Vec<String>,
}

#[derive(Clone, Copy, Debug)]
struct PackedCell {
    glyph: u32,
    foreground: u32,
    background: u32,
    flags: u16,
}

#[derive(Clone, Debug, PartialEq)]
struct PaintRun {
    row: usize,
    column: usize,
    columns: usize,
    foreground: u32,
    background: u32,
    flags: u16,
    font_family: String,
    text: String,
    box_drawing: bool,
}

impl PaintRun {
    fn can_append(
        &self,
        cell: PackedCell,
        column: usize,
        font_family: &str,
        box_drawing: bool,
    ) -> bool {
        self.column + self.columns == column
            && self.foreground == cell.foreground
            && (box_drawing || self.background == cell.background)
            && self.flags == cell.flags
            && self.flags & CELL_WIDE == 0
            && cell.flags & CELL_WIDE == 0
            && self.font_family == font_family
            && self.box_drawing == box_drawing
    }
}

#[derive(Debug, PartialEq)]
struct OverlayCell {
    row: usize,
    column: usize,
    columns: usize,
    glyph: u32,
    foreground: u32,
    background: Option<u32>,
    flags: u16,
    font_family: String,
    text: String,
}

#[derive(Debug, PartialEq)]
struct OverlayKey {
    cell_width: u32,
    line_height: u32,
    font_size: u32,
    ligatures_enabled: bool,
    cursor: Option<(usize, usize, String)>,
    cells: Vec<OverlayCell>,
}

struct TerminalFrame {
    cols: usize,
    rows: usize,
    cell_width: f32,
    line_height: f32,
    font_size: f32,
    background: gpui::Hsla,
    cursor_color: gpui::Hsla,
    cursor_x: usize,
    cursor_y: usize,
    cursor_visible: bool,
    runs: Arc<Vec<PaintRun>>,
    overlay_key: Arc<OverlayKey>,
    background_image: Arc<gpui::RenderImage>,
}

impl TerminalFrame {
    fn from_wire(frame: WireFrame) -> Option<Self> {
        let count = frame.cols.checked_mul(frame.rows)?;
        let cells = decode_cells(&frame.cells, count)?;
        Self::from_cells(frame, cells, None)
    }

    fn from_cells(
        frame: WireFrame,
        cells: Vec<PackedCell>,
        image_id: Option<gpui::ImageId>,
    ) -> Option<Self> {
        if frame.version != 2 || frame.cols == 0 || frame.rows == 0 {
            return None;
        }
        if cells.len() != frame.cols.checked_mul(frame.rows)? {
            return None;
        }
        let background = parse_color(&frame.background, gpui::transparent_black());
        let mut background_image = cell_image(&frame, &cells)?;
        if let Some(image_id) = image_id {
            background_image.id = image_id;
        }
        let background_image = Arc::new(background_image);
        let runs = Arc::new(build_runs(&frame, &cells));
        let cursor =
            (frame.cursor_visible && frame.cursor_x < frame.cols && frame.cursor_y < frame.rows)
                .then(|| (frame.cursor_x, frame.cursor_y, frame.cursor_color.clone()));
        let overlay_key = Arc::new(OverlayKey {
            cell_width: frame.cell_width.max(1.0).to_bits(),
            line_height: frame.line_height.max(1.0).to_bits(),
            font_size: frame.font_size.max(1.0).to_bits(),
            ligatures_enabled: frame.ligatures_enabled,
            cursor,
            cells: build_overlay_cells(&frame, &cells),
        });
        Some(Self {
            cols: frame.cols,
            rows: frame.rows,
            cell_width: frame.cell_width.max(1.0),
            line_height: frame.line_height.max(1.0),
            font_size: frame.font_size.max(1.0),
            background,
            cursor_color: parse_color(&frame.cursor_color, gpui::white()),
            cursor_x: frame.cursor_x,
            cursor_y: frame.cursor_y,
            cursor_visible: frame.cursor_visible,
            runs,
            overlay_key,
            background_image,
        })
    }
}

fn decode_cells(encoded: &str, count: usize) -> Option<Vec<PackedCell>> {
    let bytes = BASE64.decode(encoded).ok()?;
    decode_cell_bytes(&bytes, count)
}

fn decode_cell_bytes(bytes: &[u8], count: usize) -> Option<Vec<PackedCell>> {
    if bytes.len() != count.checked_mul(CELL_BYTES)? {
        return None;
    }
    Some(
        bytes
            .chunks_exact(CELL_BYTES)
            .map(|cell| PackedCell {
                glyph: u32::from_le_bytes(cell[0..4].try_into().unwrap()),
                foreground: u32::from_le_bytes(cell[4..8].try_into().unwrap()) & 0x00ff_ffff,
                background: u32::from_le_bytes(cell[8..12].try_into().unwrap()) & 0x00ff_ffff,
                flags: u16::from_le_bytes(cell[12..14].try_into().unwrap()),
            })
            .collect(),
    )
}

enum StagedFrame {
    Raw { metadata: WireFrame, cells: Vec<u8> },
    Prepared(TerminalFrame),
}

impl StagedFrame {
    fn into_frame(self, image_id: Option<gpui::ImageId>) -> Option<TerminalFrame> {
        match self {
            Self::Raw { metadata, cells } => {
                let count = metadata.cols.checked_mul(metadata.rows)?;
                let cells = decode_cell_bytes(&cells, count)?;
                TerminalFrame::from_cells(metadata, cells, image_id)
            }
            Self::Prepared(frame) => Some(frame),
        }
    }
}

#[derive(Clone)]
struct ActiveFrame {
    image_id: gpui::ImageId,
    cols: usize,
    rows: usize,
    overlay_key: Arc<OverlayKey>,
}

pub(crate) enum FrameUpdate {
    Invalidate,
    Repaint(Arc<gpui::RenderImage>),
}

fn staged_frames() -> &'static Mutex<HashMap<u64, StagedFrame>> {
    static FRAMES: OnceLock<Mutex<HashMap<u64, StagedFrame>>> = OnceLock::new();
    FRAMES.get_or_init(|| Mutex::new(HashMap::new()))
}

fn active_frames() -> &'static Mutex<HashMap<u64, ActiveFrame>> {
    static FRAMES: OnceLock<Mutex<HashMap<u64, ActiveFrame>>> = OnceLock::new();
    FRAMES.get_or_init(|| Mutex::new(HashMap::new()))
}

pub(crate) fn stage_frame(id: u64, metadata: &str, bytes: &[u8]) -> Result<FrameUpdate, String> {
    let metadata: WireFrame = serde_json::from_str(metadata)
        .map_err(|error| format!("Invalid terminal frame metadata: {error}"))?;
    if metadata.version != 2 || metadata.cols == 0 || metadata.rows == 0 {
        return Err("Terminal frame metadata is invalid".to_string());
    }
    let count = metadata
        .cols
        .checked_mul(metadata.rows)
        .ok_or_else(|| "Terminal frame dimensions overflow".to_string())?;
    let expected = count
        .checked_mul(CELL_BYTES)
        .ok_or_else(|| "Terminal frame dimensions overflow".to_string())?;
    if bytes.len() != expected {
        return Err("Terminal frame cell payload has the wrong length".to_string());
    }
    metadata
        .cols
        .checked_mul(2)
        .and_then(|width| {
            metadata
                .rows
                .checked_mul(2)
                .and_then(|height| width.checked_mul(height))
        })
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or_else(|| "Terminal frame dimensions overflow".to_string())?;

    let active = active_frames().lock().unwrap().get(&id).cloned();
    let (staged, update) = if let Some(active) = active {
        let cells = decode_cell_bytes(bytes, count)
            .ok_or_else(|| "Terminal frame cell payload is invalid".to_string())?;
        let frame = TerminalFrame::from_cells(metadata, cells, Some(active.image_id))
            .ok_or_else(|| "Terminal frame could not be prepared".to_string())?;
        let can_repaint = active.cols == frame.cols
            && active.rows == frame.rows
            && active.overlay_key.as_ref() == frame.overlay_key.as_ref();
        let update = if can_repaint {
            FrameUpdate::Repaint(frame.background_image.clone())
        } else {
            FrameUpdate::Invalidate
        };
        (StagedFrame::Prepared(frame), update)
    } else {
        (
            StagedFrame::Raw {
                metadata,
                cells: bytes.to_vec(),
            },
            FrameUpdate::Invalidate,
        )
    };
    staged_frames().lock().unwrap().insert(id, staged);
    Ok(update)
}

fn cell_image(frame: &WireFrame, cells: &[PackedCell]) -> Option<gpui::RenderImage> {
    let width = frame.cols.checked_mul(2)?;
    let height = frame.rows.checked_mul(2)?;
    let mut bgra = vec![0; width.checked_mul(height)?.checked_mul(4)?];
    for row in 0..frame.rows {
        for column in 0..frame.cols {
            let cell = cells[row * frame.cols + column];
            let mask = if cell.flags & CELL_FILL != 0 {
                0b1111
            } else {
                block_mask(cell.glyph).unwrap_or(0)
            };
            for pixel_y in 0..2 {
                for pixel_x in 0..2 {
                    let bit = 1 << (pixel_y * 2 + pixel_x);
                    let color = if mask & bit != 0 {
                        cell.foreground
                    } else {
                        cell.background
                    };
                    let offset = ((row * 2 + pixel_y) * width + column * 2 + pixel_x) * 4;
                    bgra[offset] = color as u8;
                    bgra[offset + 1] = (color >> 8) as u8;
                    bgra[offset + 2] = (color >> 16) as u8;
                    bgra[offset + 3] = 255;
                }
            }
        }
    }
    gpui::RenderImage::from_bgra(width.try_into().ok()?, height.try_into().ok()?, bgra)
}

fn block_mask(glyph: u32) -> Option<u8> {
    Some(match glyph {
        0x2580 => 0b0011,
        0x2584 => 0b1100,
        0x2588 => 0b1111,
        0x258c => 0b0101,
        0x2590 => 0b1010,
        0x2591 => 0b0001,
        0x2592 => 0b1001,
        0x2593 => 0b1110,
        0x2596 => 0b0100,
        0x2597 => 0b1000,
        0x2598 => 0b0001,
        0x2599 => 0b1101,
        0x259a => 0b1001,
        0x259b => 0b0111,
        0x259c => 0b1011,
        0x259d => 0b0010,
        0x259e => 0b0110,
        0x259f => 0b1110,
        _ => return None,
    })
}

fn build_runs(frame: &WireFrame, cells: &[PackedCell]) -> Vec<PaintRun> {
    let mut runs = Vec::<PaintRun>::new();
    for row in 0..frame.rows {
        let mut column = 0;
        let mut last_run: Option<usize> = None;
        while column < frame.cols {
            let cell = cells[row * frame.cols + column];
            if cell.flags & CELL_SPACER != 0 {
                last_run = None;
                column += 1;
                continue;
            }
            let columns = if cell.flags & CELL_WIDE != 0 { 2 } else { 1 };
            let decorated = cell.flags & (CELL_UNDERLINE | CELL_STRIKE) != 0;
            if cell.flags & CELL_FILL != 0 || block_mask(cell.glyph).is_some() {
                last_run = None;
                column += columns;
                continue;
            }
            let text = glyph_text(cell.glyph, &frame.graphemes);
            if text.is_empty()
                || (text.chars().all(char::is_whitespace) && !decorated)
                || (cell.foreground == cell.background && !decorated)
            {
                last_run = None;
                column += columns;
                continue;
            }
            let font_family = if cell.flags & CELL_NERD_FONT != 0 {
                frame.nerd_font_family.as_str()
            } else {
                frame.font_family.as_str()
            };
            let box_drawing = is_box_drawing(cell.glyph);
            if let Some(index) = last_run
                .filter(|index| runs[*index].can_append(cell, column, font_family, box_drawing))
            {
                if !frame.ligatures_enabled {
                    runs[index].text.push('\u{200c}');
                }
                runs[index].text.push_str(&text);
                runs[index].columns += columns;
            } else {
                runs.push(PaintRun {
                    row,
                    column,
                    columns,
                    foreground: cell.foreground,
                    background: if box_drawing { 0 } else { cell.background },
                    flags: cell.flags,
                    font_family: font_family.to_string(),
                    text,
                    box_drawing,
                });
                last_run = Some(runs.len() - 1);
            }
            column += columns;
        }
    }
    runs
}

fn build_overlay_cells(frame: &WireFrame, cells: &[PackedCell]) -> Vec<OverlayCell> {
    let mut overlay = Vec::new();
    for row in 0..frame.rows {
        let mut column = 0;
        while column < frame.cols {
            let cell = cells[row * frame.cols + column];
            if cell.flags & CELL_SPACER != 0 {
                column += 1;
                continue;
            }
            let columns = if cell.flags & CELL_WIDE != 0 { 2 } else { 1 };
            let decorated = cell.flags & (CELL_UNDERLINE | CELL_STRIKE) != 0;
            if cell.flags & CELL_FILL != 0 || block_mask(cell.glyph).is_some() {
                column += columns;
                continue;
            }
            let text = glyph_text(cell.glyph, &frame.graphemes);
            if text.is_empty()
                || (text.chars().all(char::is_whitespace) && !decorated)
                || (cell.foreground == cell.background && !decorated)
            {
                column += columns;
                continue;
            }
            let font_family = if cell.flags & CELL_NERD_FONT != 0 {
                frame.nerd_font_family.clone()
            } else {
                frame.font_family.clone()
            };
            overlay.push(OverlayCell {
                row,
                column,
                columns,
                glyph: cell.glyph,
                foreground: cell.foreground,
                background: (!is_box_drawing(cell.glyph)).then_some(cell.background),
                flags: cell.flags,
                font_family,
                text,
            });
            column += columns;
        }
    }
    overlay
}

fn is_box_drawing(glyph: u32) -> bool {
    (0x2500..=0x257f).contains(&glyph)
}

fn glyph_text(glyph: u32, graphemes: &[String]) -> String {
    if glyph & GRAPHEME_INDEX != 0 {
        return graphemes
            .get((glyph & !GRAPHEME_INDEX) as usize)
            .cloned()
            .unwrap_or_else(|| "�".to_string());
    }
    char::from_u32(glyph).unwrap_or('�').to_string()
}

fn parse_color(value: &str, fallback: gpui::Hsla) -> gpui::Hsla {
    crate::color::parse_color_rgba(value)
        .map(Into::into)
        .unwrap_or(fallback)
}

fn packed_color(value: u32) -> gpui::Hsla {
    gpui::rgb(value & 0x00ff_ffff).into()
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct ShapeKey {
    font_family: String,
    font_weight: u16,
    italic: bool,
    text: String,
    columns: usize,
    font_size: u32,
    cell_width: u32,
}

#[derive(Clone)]
struct ShapedPaintRun {
    line: gpui::ShapedLine,
    x: f32,
    y: f32,
    width: f32,
    color: gpui::Hsla,
    underline: bool,
    strike: bool,
}

struct PaintState {
    frame: Arc<TerminalFrame>,
    text: Vec<ShapedPaintRun>,
}

pub struct TerminalElement {
    id: u64,
    frame: Option<Arc<TerminalFrame>>,
    stale_images: Vec<Arc<gpui::RenderImage>>,
    shape_cache: Arc<Mutex<HashMap<ShapeKey, gpui::ShapedLine>>>,
}

impl TerminalElement {
    fn new(id: u64) -> Self {
        Self {
            id,
            frame: None,
            stale_images: Vec::new(),
            shape_cache: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

impl CustomElement for TerminalElement {
    fn render(
        &mut self,
        ctx: CustomRenderContext,
        window: &mut gpui::Window,
        _cx: &mut gpui::Context<crate::renderer::GpuixView>,
    ) -> gpui::AnyElement {
        use gpui::prelude::*;

        if let Some(staged) = staged_frames().lock().unwrap().remove(&self.id) {
            let image_id = self.frame.as_ref().map(|frame| frame.background_image.id);
            if let Some(frame) = staged.into_frame(image_id).map(Arc::new) {
                if image_id.is_some() {
                    let _ = window.update_image(frame.background_image.clone());
                }
                self.frame = Some(frame);
            }
        }
        for image in self.stale_images.drain(..) {
            let _ = window.drop_image(image);
        }
        let Some(frame) = self.frame.clone() else {
            active_frames().lock().unwrap().remove(&self.id);
            let empty = super::custom_surface(
                gpui::div().id(gpui::SharedString::from(format!(
                    "__gpuix_terminal_{}",
                    ctx.id
                ))),
                &ctx,
            );
            return empty.into_any_element();
        };
        active_frames().lock().unwrap().insert(
            self.id,
            ActiveFrame {
                image_id: frame.background_image.id,
                cols: frame.cols,
                rows: frame.rows,
                overlay_key: frame.overlay_key.clone(),
            },
        );
        let shape_cache = self.shape_cache.clone();
        let canvas = gpui::canvas(
            move |_bounds, window, _cx| {
                let mut text = Vec::with_capacity(frame.runs.len());
                let mut cache = shape_cache.lock().unwrap();
                if cache.len() > SHAPE_CACHE_LIMIT {
                    cache.clear();
                }
                for run in frame.runs.iter() {
                    let key = ShapeKey {
                        font_family: run.font_family.clone(),
                        font_weight: if run.flags & CELL_BOLD != 0 { 700 } else { 400 },
                        italic: run.flags & CELL_ITALIC != 0,
                        text: run.text.clone(),
                        columns: run.columns,
                        font_size: frame.font_size.to_bits(),
                        cell_width: frame.cell_width.to_bits(),
                    };
                    let line = if let Some(line) = cache.get(&key) {
                        line.clone()
                    } else {
                        let mut font = gpui::font(run.font_family.clone());
                        font.weight = gpui::FontWeight(key.font_weight as f32);
                        font.style = if key.italic {
                            gpui::FontStyle::Italic
                        } else {
                            gpui::FontStyle::Normal
                        };
                        let text_run = gpui::TextRun {
                            len: run.text.len(),
                            font,
                            color: gpui::white(),
                            background_color: None,
                            underline: None,
                            strikethrough: None,
                        };
                        let line = window.text_system().shape_line(
                            run.text.clone().into(),
                            gpui::px(frame.font_size),
                            &[text_run],
                            Some(gpui::px(frame.cell_width)),
                        );
                        cache.insert(key, line.clone());
                        line
                    };
                    text.push(ShapedPaintRun {
                        line,
                        x: run.column as f32 * frame.cell_width,
                        y: run.row as f32 * frame.line_height,
                        width: run.columns as f32 * frame.cell_width,
                        color: packed_color(run.foreground),
                        underline: run.flags & CELL_UNDERLINE != 0,
                        strike: run.flags & CELL_STRIKE != 0,
                    });
                }
                drop(cache);
                PaintState { frame, text }
            },
            move |bounds, state, window, cx| {
                window.paint_quad(gpui::fill(bounds, state.frame.background));
                let _ = window.paint_image_nearest(
                    bounds,
                    bounds,
                    Default::default(),
                    state.frame.background_image.clone(),
                    0,
                    false,
                );
                if state.frame.cursor_visible
                    && state.frame.cursor_x < state.frame.cols
                    && state.frame.cursor_y < state.frame.rows
                {
                    window.paint_quad(gpui::fill(
                        gpui::Bounds::new(
                            gpui::point(
                                bounds.origin.x
                                    + gpui::px(
                                        state.frame.cursor_x as f32 * state.frame.cell_width,
                                    ),
                                bounds.origin.y
                                    + gpui::px(
                                        state.frame.cursor_y as f32 * state.frame.line_height,
                                    ),
                            ),
                            gpui::size(
                                gpui::px(state.frame.cell_width),
                                gpui::px(state.frame.line_height),
                            ),
                        ),
                        state.frame.cursor_color.opacity(0.35),
                    ));
                }
                for run in state.text {
                    crate::text::paint::log_painted_text(run.line.text.clone());
                    let origin = gpui::point(
                        bounds.origin.x + gpui::px(run.x),
                        bounds.origin.y + gpui::px(run.y),
                    );
                    let _ = paint_shaped_line(
                        &run.line,
                        origin,
                        gpui::px(state.frame.line_height),
                        run.color,
                        window,
                    );
                    if run.underline {
                        window.paint_quad(gpui::fill(
                            gpui::Bounds::new(
                                gpui::point(
                                    origin.x,
                                    origin.y + gpui::px(state.frame.line_height - 2.0),
                                ),
                                gpui::size(gpui::px(run.width), gpui::px(1.0)),
                            ),
                            run.color,
                        ));
                    }
                    if run.strike {
                        window.paint_quad(gpui::fill(
                            gpui::Bounds::new(
                                gpui::point(
                                    origin.x,
                                    origin.y + gpui::px(state.frame.line_height * 0.52),
                                ),
                                gpui::size(gpui::px(run.width), gpui::px(1.0)),
                            ),
                            run.color,
                        ));
                    }
                }
                let _ = cx;
            },
        )
        .absolute()
        .size_full();

        let surface = super::custom_surface(
            gpui::div()
                .id(gpui::SharedString::from(format!(
                    "__gpuix_terminal_{}",
                    ctx.id
                )))
                .overflow_hidden(),
            &ctx,
        );
        surface.child(canvas).into_any_element()
    }

    fn set_prop(&mut self, key: &str, value: serde_json::Value) {
        if key != "frame" {
            return;
        }
        if let Some(frame) = self.frame.take() {
            self.stale_images.push(frame.background_image.clone());
        }
        self.frame = serde_json::from_value::<WireFrame>(value)
            .ok()
            .and_then(TerminalFrame::from_wire)
            .map(Arc::new);
    }

    fn supported_props(&self) -> &'static [&'static str] {
        &["frame"]
    }

    fn supported_events(&self) -> &'static [&'static str] {
        &[]
    }

    fn destroy(&mut self) {
        staged_frames().lock().unwrap().remove(&self.id);
        active_frames().lock().unwrap().remove(&self.id);
        self.frame = None;
        self.stale_images.clear();
        self.shape_cache.lock().unwrap().clear();
    }
}

#[cfg(test)]
mod tests {
    use super::{
        active_frames, block_mask, stage_frame, staged_frames, ActiveFrame, FrameUpdate, CELL_BYTES,
    };

    fn metadata(cursor_x: usize) -> String {
        metadata_with_cursor(cursor_x, true)
    }

    fn metadata_with_cursor(cursor_x: usize, cursor_visible: bool) -> String {
        format!(
            r##"{{"version":2,"cols":1,"rows":1,"cellWidth":8,"lineHeight":18,"fontSize":13,"background":"#000000","cursorColor":"#ffffff","cursorX":{cursor_x},"cursorY":0,"cursorVisible":{cursor_visible},"fontFamily":"Menlo","nerdFontFamily":"Symbols Nerd Font Mono","ligaturesEnabled":true,"graphemes":[]}}"##
        )
    }

    #[test]
    fn maps_framebuffer_blocks_to_quarter_cell_masks() {
        assert_eq!(block_mask('▀' as u32), Some(0b0011));
        assert_eq!(block_mask('▄' as u32), Some(0b1100));
        assert_eq!(block_mask('▌' as u32), Some(0b0101));
        assert_eq!(block_mask('▐' as u32), Some(0b1010));
        assert_eq!(block_mask('▙' as u32), Some(0b1101));
        assert_eq!(block_mask('▟' as u32), Some(0b1110));
        assert_eq!(block_mask('A' as u32), None);
    }

    #[test]
    fn repaints_stable_text_overlays_without_invalidating_the_view_tree() {
        const ID: u64 = u64::MAX - 8;
        staged_frames().lock().unwrap().remove(&ID);
        active_frames().lock().unwrap().remove(&ID);

        let metadata = metadata_with_cursor(0, false).replace("\"cols\":1", "\"cols\":2");
        let mut cells = vec![0; CELL_BYTES * 2];
        cells[0..4].copy_from_slice(&(u32::from('A')).to_le_bytes());
        cells[4..8].copy_from_slice(&0x00ff_ffff_u32.to_le_bytes());
        cells[CELL_BYTES..CELL_BYTES + 4].copy_from_slice(&(u32::from('▀')).to_le_bytes());
        cells[CELL_BYTES + 4..CELL_BYTES + 8].copy_from_slice(&0x0000_00ff_u32.to_le_bytes());

        assert!(matches!(
            stage_frame(ID, &metadata, &cells).unwrap(),
            FrameUpdate::Invalidate
        ));
        let frame = staged_frames()
            .lock()
            .unwrap()
            .remove(&ID)
            .unwrap()
            .into_frame(None)
            .unwrap();
        let image_id = frame.background_image.id;
        active_frames().lock().unwrap().insert(
            ID,
            ActiveFrame {
                image_id,
                cols: frame.cols,
                rows: frame.rows,
                overlay_key: frame.overlay_key,
            },
        );

        cells[CELL_BYTES + 4..CELL_BYTES + 8].copy_from_slice(&0x0000_ff00_u32.to_le_bytes());
        match stage_frame(ID, &metadata, &cells).unwrap() {
            FrameUpdate::Repaint(image) => assert_eq!(image.id, image_id),
            FrameUpdate::Invalidate => panic!("unchanged text overlay invalidated the view tree"),
        }

        cells[0..4].copy_from_slice(&(u32::from('B')).to_le_bytes());
        assert!(matches!(
            stage_frame(ID, &metadata, &cells).unwrap(),
            FrameUpdate::Invalidate
        ));

        staged_frames().lock().unwrap().remove(&ID);
        active_frames().lock().unwrap().remove(&ID);
    }

    #[test]
    fn ignores_texture_owned_background_changes_behind_box_glyphs() {
        const ID: u64 = u64::MAX - 9;
        staged_frames().lock().unwrap().remove(&ID);
        active_frames().lock().unwrap().remove(&ID);

        let metadata = metadata_with_cursor(0, false);
        let mut cells = vec![0; CELL_BYTES];
        cells[0..4].copy_from_slice(&(u32::from('═')).to_le_bytes());
        cells[4..8].copy_from_slice(&0x00ff_ffff_u32.to_le_bytes());

        stage_frame(ID, &metadata, &cells).unwrap();
        let frame = staged_frames()
            .lock()
            .unwrap()
            .remove(&ID)
            .unwrap()
            .into_frame(None)
            .unwrap();
        let image_id = frame.background_image.id;
        let runs = frame.runs.clone();
        active_frames().lock().unwrap().insert(
            ID,
            ActiveFrame {
                image_id,
                cols: frame.cols,
                rows: frame.rows,
                overlay_key: frame.overlay_key,
            },
        );

        cells[8..12].copy_from_slice(&0x0000_00ff_u32.to_le_bytes());
        assert!(matches!(
            stage_frame(ID, &metadata, &cells).unwrap(),
            FrameUpdate::Repaint(_)
        ));
        let replacement = staged_frames()
            .lock()
            .unwrap()
            .remove(&ID)
            .unwrap()
            .into_frame(Some(image_id))
            .unwrap();
        assert_eq!(replacement.runs, runs);

        cells[4..8].copy_from_slice(&0x0000_ff00_u32.to_le_bytes());
        assert!(matches!(
            stage_frame(ID, &metadata, &cells).unwrap(),
            FrameUpdate::Invalidate
        ));

        staged_frames().lock().unwrap().remove(&ID);
        active_frames().lock().unwrap().remove(&ID);
    }

    #[test]
    fn stages_only_the_latest_valid_binary_frame() {
        const ID: u64 = u64::MAX - 7;
        staged_frames().lock().unwrap().remove(&ID);
        let mut cells = vec![0; CELL_BYTES];
        cells[0..4].copy_from_slice(&(u32::from('A')).to_le_bytes());
        cells[4..8].copy_from_slice(&0x00ff_ffff_u32.to_le_bytes());

        stage_frame(ID, &metadata(0), &cells).unwrap();
        stage_frame(ID, &metadata(1), &cells).unwrap();
        let frame = staged_frames()
            .lock()
            .unwrap()
            .remove(&ID)
            .unwrap()
            .into_frame(None)
            .unwrap();

        assert_eq!(frame.cols, 1);
        assert_eq!(frame.rows, 1);
        assert_eq!(frame.cursor_x, 1);

        let image_id = frame.background_image.id;
        stage_frame(ID, &metadata(0), &cells).unwrap();
        let replacement = staged_frames()
            .lock()
            .unwrap()
            .remove(&ID)
            .unwrap()
            .into_frame(Some(image_id))
            .unwrap();
        assert_eq!(replacement.background_image.id, image_id);

        assert!(stage_frame(ID, &metadata(0), &cells[..CELL_BYTES - 1]).is_err());
        assert!(!staged_frames().lock().unwrap().contains_key(&ID));
    }
}

fn paint_shaped_line(
    line: &gpui::ShapedLine,
    origin: gpui::Point<gpui::Pixels>,
    line_height: gpui::Pixels,
    color: gpui::Hsla,
    window: &mut gpui::Window,
) -> gpui::Result<()> {
    let padding_top = (line_height - line.ascent - line.descent) / 2.0;
    let baseline = origin.y + padding_top + line.ascent;
    for shaped_run in &line.runs {
        for glyph in &shaped_run.glyphs {
            let glyph_origin =
                gpui::point(origin.x + glyph.position.x, baseline + glyph.position.y);
            if glyph.is_emoji {
                window.paint_emoji(glyph_origin, shaped_run.font_id, glyph.id, line.font_size)?;
            } else {
                window.paint_glyph(
                    glyph_origin,
                    shaped_run.font_id,
                    glyph.id,
                    line.font_size,
                    color,
                )?;
            }
        }
    }
    Ok(())
}
