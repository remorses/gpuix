//! Presents rendered frames with the kitty graphics protocol.
//!
//! When the terminal runs on this machine, each frame is a POSIX shared
//! memory object (`t=s`) that the GPU readback writes into directly, so the
//! pty only carries a ~100 byte escape code. Over SSH the terminal cannot open
//! our shared memory, so frames are sent inline instead: zlib-compressed,
//! base64, in 4096 byte chunks (`t=d,o=z`).
//!
//! Which one works is asked once at startup, like kitty's own `icat` does: a
//! one pixel shared memory query (`a=q,t=s`) followed by `CSI c`. Terminals
//! answer in order, so a device attributes reply without an `OK` for the query
//! means shared memory is not reachable.
//!
//! The same round trip asks whether the terminal can edit the shown frame in
//! place (`a=f,r=1`, kitty 0.20 and Ghostty 1.4). If it can, an inline frame
//! only carries the regions that changed since the previous one: a hover or a
//! keystroke is a few KB instead of the whole frame.
//!
//! Frames are transmitted with `U=1`, which only stores them, and shown by
//! painting a grid of Unicode placeholders (`U+10EEEE` plus row and column
//! diacritics, in the image id's colour). The picture is then ordinary text:
//! it scrolls, is clipped and is redrawn with the cells it occupies. That is
//! what makes it work inside tmux, which has no graphics protocol of its own
//! and can only pass the escape codes through (`ESC P tmux; ... ESC \`).
//! https://sw.kovidgoyal.net/kitty/graphics-protocol/

use std::{
    collections::VecDeque,
    ffi::CString,
    io::{self, Write as _},
    ptr,
    sync::atomic::Ordering,
    time::{Duration, Instant},
};

use anyhow::{Context as _, Result};
use base64::Engine as _;

use crate::tty::{IMAGE_ID, Tty, tmux_passthrough};

/// Frames whose shared memory object we still own. The terminal unlinks an
/// object once it read it; we unlink anything older than this ourselves, so a
/// terminal that stops reading cannot leak memory without bound.
const IN_FLIGHT_FRAMES: usize = 4;
/// Image id of the startup shared memory query.
const PROBE_ID: u32 = 0x4750_0001;
/// Image id of the startup frame edit probe.
const FRAME_EDIT_PROBE_ID: u32 = 0x4750_0002;
/// Changed regions are found on a grid of square tiles this many pixels wide.
const DIRTY_TILE: u32 = 32;
/// A terminal that answers neither the query nor `CSI c` gets inline frames.
const PROBE_TIMEOUT: Duration = Duration::from_secs(2);
/// Inline frames are followed by a query whose reply says the terminal
/// consumed them. Each fence gets a fresh id from this range.
const FENCE_ID_START: u32 = 0x4750_1000;
const FENCE_ID_COUNT: u32 = 0x1_0000;
/// Inline frames that may be on their way to the terminal. More would only
/// sit in the SSH buffers and delay the response to input.
const MAX_UNACKED_FRAMES: usize = 2;
/// A fence that never got a reply stops holding frames back after this.
const FENCE_TIMEOUT: Duration = Duration::from_secs(2);
/// Kitty's limit for the base64 payload of one escape code.
const CHUNK_SIZE: usize = 4096;

enum Transport {
    /// Waiting for the terminal to answer the startup queries.
    Probing {
        /// Must stay mapped and linked until the terminal answered.
        _probe: Option<SharedMemory>,
        shared_memory: bool,
        frame_edits: bool,
        deadline: Instant,
    },
    SharedMemory,
    /// `frame_edits`: the terminal patches the shown frame (`a=f`), so only
    /// changed regions are sent.
    Inline {
        frame_edits: bool,
    },
}

pub(crate) struct KittyPresenter {
    transport: Transport,
    process_id: u32,
    /// tmux speaks no graphics protocol: every APC has to go through its
    /// passthrough envelope or it is dropped.
    tmux: bool,
    /// Image id this instance owns, see [`instance_image_id`].
    image_id: u32,
    /// Grid geometry the placeholders were last painted for.
    grid: (u16, u16),
    frame_counter: u64,
    in_flight: VecDeque<CString>,
    command: Vec<u8>,
    /// Reused render target for inline frames.
    inline_pixels: Vec<u8>,
    /// What the terminal shows, kept to diff the next frame against.
    shown: Option<ShownFrame>,
    fences: VecDeque<(u32, Instant)>,
    next_fence: u32,
}

struct ShownFrame {
    pixels: Vec<u8>,
    width: u32,
    height: u32,
}

/// A region of the frame in device pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Rect {
    x: u32,
    y: u32,
    width: u32,
    height: u32,
}

/// A frame the next scene is rendered into.
pub(crate) struct Frame {
    width: u32,
    height: u32,
    rows: u16,
    columns: u16,
    target: Target,
}

enum Target {
    Shared(SharedMemory),
    Inline(Vec<u8>),
}

impl Frame {
    pub(crate) fn pixels(&mut self) -> &mut [u8] {
        match &mut self.target {
            Target::Shared(shared) => shared.bytes(),
            Target::Inline(pixels) => pixels,
        }
    }
}

/// A mapped POSIX shared memory object. Dropping it unlinks the object unless
/// it was handed to the terminal, which unlinks it after reading.
struct SharedMemory {
    name: CString,
    mapping: *mut libc::c_void,
    length: usize,
    presented: bool,
}

impl SharedMemory {
    fn bytes(&mut self) -> &mut [u8] {
        unsafe { std::slice::from_raw_parts_mut(self.mapping as *mut u8, self.length) }
    }
}

impl Drop for SharedMemory {
    fn drop(&mut self) {
        unsafe { libc::munmap(self.mapping, self.length) };
        if !self.presented {
            unsafe { libc::shm_unlink(self.name.as_ptr()) };
        }
    }
}

impl KittyPresenter {
    /// Asks the terminal whether it can read our shared memory and whether it
    /// can edit a shown frame. No frame can be presented until `wait_until`
    /// returns `None`.
    pub(crate) fn new(tty: &mut Tty) -> Result<Self> {
        let image_id = instance_image_id();
        IMAGE_ID.store(image_id, Ordering::Relaxed);
        let mut presenter = Self {
            transport: Transport::Inline { frame_edits: false },
            process_id: std::process::id(),
            tmux: std::env::var_os("TMUX").is_some(),
            image_id,
            grid: (0, 0),
            frame_counter: 0,
            in_flight: VecDeque::new(),
            command: Vec::new(),
            inline_pixels: Vec::new(),
            shown: None,
            fences: VecDeque::new(),
            next_fence: 0,
        };
        let mut query = String::new();
        let probe = match presenter.create_shared_memory(3) {
            Ok(mut probe) => {
                probe.bytes().fill(0);
                let encoded_name =
                    base64::engine::general_purpose::STANDARD.encode(probe.name.as_bytes());
                query.push_str(&format!(
                    "\x1b_Ga=q,i={PROBE_ID},s=1,v=1,f=24,t=s,S=3;{encoded_name}\x1b\\"
                ));
                Some(probe)
            }
            Err(error) => {
                log::info!("no shared memory ({error:#}), sending frames inline");
                None
            }
        };
        // A 1x1 image, an edit of its root frame (the only reply), then delete it.
        // Ghostty 1.3 answers the edit with an error that carries no image id.
        query.push_str(&format!(
            "\x1b_Ga=t,i={FRAME_EDIT_PROBE_ID},s=1,v=1,f=24,t=d,q=2;AAAA\x1b\\\
             \x1b_Ga=f,i={FRAME_EDIT_PROBE_ID},r=1,s=1,v=1,X=1,f=24,t=d;AAAA\x1b\\\
             \x1b_Ga=d,d=I,i={FRAME_EDIT_PROBE_ID},q=2\x1b\\\x1b[c"
        ));
        if presenter.tmux {
            tty.write_all(&tmux_passthrough(query.as_bytes()))?;
        } else {
            tty.write_all(query.as_bytes())?;
        }
        presenter.transport = Transport::Probing {
            _probe: probe,
            shared_memory: false,
            frame_edits: false,
            deadline: Instant::now() + PROBE_TIMEOUT,
        };
        Ok(presenter)
    }

    pub(crate) fn handle_reply(&mut self, id: u32, ok: bool) {
        if id == PROBE_ID || id == FRAME_EDIT_PROBE_ID {
            if let Transport::Probing {
                shared_memory,
                frame_edits,
                ..
            } = &mut self.transport
            {
                *(if id == PROBE_ID {
                    shared_memory
                } else {
                    frame_edits
                }) = ok;
            }
            // tmux answers the device-attributes query itself, so under tmux
            // this reply is the only thing that can end the probe.
            if self.tmux {
                self.finish_probe();
            }
            return;
        }
        // Replies arrive in order, so a fence's reply also answers every older one.
        if let Some(index) = self.fences.iter().position(|(fence, _)| *fence == id) {
            self.fences.drain(..=index);
        }
    }

    /// The reply to the `CSI c` sent after the startup queries, so every
    /// answer the terminal will give to them has arrived.
    pub(crate) fn handle_device_attributes(&mut self) {
        // tmux answers this query itself, before the terminal's graphics reply
        // can arrive, so under tmux it says nothing about graphics support.
        if self.tmux {
            return;
        }
        self.finish_probe();
    }

    fn finish_probe(&mut self) {
        let Transport::Probing {
            shared_memory,
            frame_edits,
            ..
        } = self.transport
        else {
            return;
        };
        self.transport = if shared_memory {
            log::info!("sending frames through shared memory");
            Transport::SharedMemory
        } else {
            let frame_edits = frame_edits && !self.tmux;
            log::info!(
                "the terminal cannot read our shared memory, sending frames inline \
                 ({})",
                if frame_edits {
                    "changed regions only"
                } else {
                    "whole frames, the terminal cannot edit frames"
                }
            );
            Transport::Inline { frame_edits }
        };
    }

    /// When a frame cannot be presented yet, the time to check again. A
    /// terminal reply may also make it ready earlier.
    pub(crate) fn wait_until(&mut self, now: Instant) -> Option<Instant> {
        if let Transport::Probing { deadline, .. } = self.transport {
            if now < deadline {
                return Some(deadline);
            }
            log::warn!("the terminal did not answer the graphics query");
            self.finish_probe();
        }
        while self.fences.len() >= MAX_UNACKED_FRAMES {
            let (_, sent) = self.fences.front()?;
            let expires = *sent + FENCE_TIMEOUT;
            if now < expires {
                return Some(expires);
            }
            self.fences.pop_front();
        }
        None
    }

    /// Allocates the target for an RGBA frame of `width` x `height`.
    /// Dropping the frame without `present` discards it.
    pub(crate) fn begin_frame(
        &mut self,
        width: u32,
        height: u32,
        rows: u16,
        columns: u16,
    ) -> Result<Frame> {
        let length = width as usize * height as usize * 4;
        let target = match self.transport {
            Transport::Probing { .. } => anyhow::bail!("the frame transport is not known yet"),
            Transport::SharedMemory => Target::Shared(self.create_shared_memory(length)?),
            Transport::Inline { .. } => {
                let mut pixels = std::mem::take(&mut self.inline_pixels);
                pixels.resize(length, 0);
                Target::Inline(pixels)
            }
        };
        Ok(Frame {
            width,
            height,
            rows,
            columns,
            target,
        })
    }

    /// Shows `frame` covering the grid from its top-left cell.
    pub(crate) fn present(&mut self, tty: &mut Tty, frame: Frame) -> Result<()> {
        self.command.clear();
        match frame.target {
            Target::Shared(mut shared) => {
                if self.in_flight.len() >= IN_FLIGHT_FRAMES
                    && let Some(stale) = self.in_flight.pop_front()
                {
                    // ENOENT is the normal case: the terminal already unlinked it.
                    unsafe { libc::shm_unlink(stale.as_ptr()) };
                }
                let encoded_name =
                    base64::engine::general_purpose::STANDARD.encode(shared.name.as_bytes());
                write!(
                    self.command,
                    "\x1b[?2026h\x1b[H\x1b_Ga=T,i={},U=1,f=32,t=s,s={},v={},S={},q=2;{encoded_name}\x1b\\\x1b[?2026l",
                    self.image_id, frame.width, frame.height, shared.length,
                )?;
                shared.presented = true;
                self.in_flight.push_back(shared.name.clone());
            }
            Target::Inline(pixels) => {
                let frame_edits = matches!(self.transport, Transport::Inline { frame_edits: true });
                let edits = self
                    .shown
                    .as_ref()
                    .filter(|shown| {
                        frame_edits && shown.width == frame.width && shown.height == frame.height
                    })
                    .map(|shown| dirty_rects(&shown.pixels, &pixels, frame.width, frame.height));
                if edits.as_ref().is_some_and(Vec::is_empty) {
                    self.inline_pixels = pixels;
                    return Ok(());
                }
                let fence = FENCE_ID_START + self.next_fence;
                self.next_fence = (self.next_fence + 1) % FENCE_ID_COUNT;
                match edits {
                    Some(rects) => {
                        write_frame_edits(&mut self.command, &pixels, frame.width, &rects, fence)?
                    }
                    None => write_inline_frame(
                        &mut self.command,
                        &pixels,
                        frame.width,
                        frame.height,
                        fence,
                        self.image_id,
                    )?,
                }
                self.fences.push_back((fence, Instant::now()));
                if frame_edits {
                    let previous = self.shown.replace(ShownFrame {
                        pixels,
                        width: frame.width,
                        height: frame.height,
                    });
                    self.inline_pixels = previous.map(|shown| shown.pixels).unwrap_or_default();
                } else {
                    self.inline_pixels = pixels;
                }
            }
        }
        // TODO: over a slow link this write blocks the main thread until the
        // link drains. A writer thread would keep input responsive; the fence
        // gate already bounds it to two frames.
        if self.tmux {
            tty.write_all(&tmux_passthrough(&self.command))?;
        } else {
            tty.write_all(&self.command)?;
        }
        // The picture is this grid: ordinary text cells, so tmux keeps them in
        // its own grid, clips them to the pane and repaints them on return.
        // Install them once per geometry; retransmitting the image updates them.
        if self.grid != (frame.rows, frame.columns) {
            self.grid = (frame.rows, frame.columns);
            tty.write_all(&placeholders(frame.rows, frame.columns, self.image_id))?;
        }
        Ok(())
    }

    fn create_shared_memory(&mut self, length: usize) -> Result<SharedMemory> {
        // macOS caps shared memory names at 31 bytes including the slash.
        let name = format!("/gpui{:x}.{:x}", self.process_id, self.frame_counter);
        self.frame_counter += 1;
        let name = CString::new(name).context("shared memory name")?;
        let mapping = map_shared_memory(&name, length)?;
        Ok(SharedMemory {
            name,
            mapping,
            length,
            presented: false,
        })
    }
}

impl Drop for KittyPresenter {
    fn drop(&mut self) {
        for name in self.in_flight.drain(..) {
            unsafe { libc::shm_unlink(name.as_ptr()) };
        }
    }
}

/// Image ids live in the terminal's global namespace and the placeholders only
/// carry 24 bits of it, so every instance takes its own: two apps in separate
/// tmux panes would otherwise overwrite each other's picture.
fn instance_image_id() -> u32 {
    (std::process::id() & 0x00ff_ffff) | 1
}

/// Kitty's combining diacritics, one per row or column index (the spec table,
/// same order as kitty's docs).
const DIACRITICS: [char; 297] = [
    '\u{0305}', '\u{030D}', '\u{030E}', '\u{0310}', '\u{0312}', '\u{033D}', '\u{033E}', '\u{033F}', '\u{0346}', '\u{034A}', '\u{034B}', '\u{034C}',
    '\u{0350}', '\u{0351}', '\u{0352}', '\u{0357}', '\u{035B}', '\u{0363}', '\u{0364}', '\u{0365}', '\u{0366}', '\u{0367}', '\u{0368}', '\u{0369}',
    '\u{036A}', '\u{036B}', '\u{036C}', '\u{036D}', '\u{036E}', '\u{036F}', '\u{0483}', '\u{0484}', '\u{0485}', '\u{0486}', '\u{0487}', '\u{0592}',
    '\u{0593}', '\u{0594}', '\u{0595}', '\u{0597}', '\u{0598}', '\u{0599}', '\u{059C}', '\u{059D}', '\u{059E}', '\u{059F}', '\u{05A0}', '\u{05A1}',
    '\u{05A8}', '\u{05A9}', '\u{05AB}', '\u{05AC}', '\u{05AF}', '\u{05C4}', '\u{0610}', '\u{0611}', '\u{0612}', '\u{0613}', '\u{0614}', '\u{0615}',
    '\u{0616}', '\u{0617}', '\u{0657}', '\u{0658}', '\u{0659}', '\u{065A}', '\u{065B}', '\u{065D}', '\u{065E}', '\u{06D6}', '\u{06D7}', '\u{06D8}',
    '\u{06D9}', '\u{06DA}', '\u{06DB}', '\u{06DC}', '\u{06DF}', '\u{06E0}', '\u{06E1}', '\u{06E2}', '\u{06E4}', '\u{06E7}', '\u{06E8}', '\u{06EB}',
    '\u{06EC}', '\u{0730}', '\u{0732}', '\u{0733}', '\u{0735}', '\u{0736}', '\u{073A}', '\u{073D}', '\u{073F}', '\u{0740}', '\u{0741}', '\u{0743}',
    '\u{0745}', '\u{0747}', '\u{0749}', '\u{074A}', '\u{07EB}', '\u{07EC}', '\u{07ED}', '\u{07EE}', '\u{07EF}', '\u{07F0}', '\u{07F1}', '\u{07F3}',
    '\u{0816}', '\u{0817}', '\u{0818}', '\u{0819}', '\u{081B}', '\u{081C}', '\u{081D}', '\u{081E}', '\u{081F}', '\u{0820}', '\u{0821}', '\u{0822}',
    '\u{0823}', '\u{0825}', '\u{0826}', '\u{0827}', '\u{0829}', '\u{082A}', '\u{082B}', '\u{082C}', '\u{082D}', '\u{0951}', '\u{0953}', '\u{0954}',
    '\u{0F82}', '\u{0F83}', '\u{0F86}', '\u{0F87}', '\u{135D}', '\u{135E}', '\u{135F}', '\u{17DD}', '\u{193A}', '\u{1A17}', '\u{1A75}', '\u{1A76}',
    '\u{1A77}', '\u{1A78}', '\u{1A79}', '\u{1A7A}', '\u{1A7B}', '\u{1A7C}', '\u{1B6B}', '\u{1B6D}', '\u{1B6E}', '\u{1B6F}', '\u{1B70}', '\u{1B71}',
    '\u{1B72}', '\u{1B73}', '\u{1CD0}', '\u{1CD1}', '\u{1CD2}', '\u{1CDA}', '\u{1CDB}', '\u{1CE0}', '\u{1DC0}', '\u{1DC1}', '\u{1DC3}', '\u{1DC4}',
    '\u{1DC5}', '\u{1DC6}', '\u{1DC7}', '\u{1DC8}', '\u{1DC9}', '\u{1DCB}', '\u{1DCC}', '\u{1DD1}', '\u{1DD2}', '\u{1DD3}', '\u{1DD4}', '\u{1DD5}',
    '\u{1DD6}', '\u{1DD7}', '\u{1DD8}', '\u{1DD9}', '\u{1DDA}', '\u{1DDB}', '\u{1DDC}', '\u{1DDD}', '\u{1DDE}', '\u{1DDF}', '\u{1DE0}', '\u{1DE1}',
    '\u{1DE2}', '\u{1DE3}', '\u{1DE4}', '\u{1DE5}', '\u{1DE6}', '\u{1DFE}', '\u{20D0}', '\u{20D1}', '\u{20D4}', '\u{20D5}', '\u{20D6}', '\u{20D7}',
    '\u{20DB}', '\u{20DC}', '\u{20E1}', '\u{20E7}', '\u{20E9}', '\u{20F0}', '\u{2CEF}', '\u{2CF0}', '\u{2CF1}', '\u{2DE0}', '\u{2DE1}', '\u{2DE2}',
    '\u{2DE3}', '\u{2DE4}', '\u{2DE5}', '\u{2DE6}', '\u{2DE7}', '\u{2DE8}', '\u{2DE9}', '\u{2DEA}', '\u{2DEB}', '\u{2DEC}', '\u{2DED}', '\u{2DEE}',
    '\u{2DEF}', '\u{2DF0}', '\u{2DF1}', '\u{2DF2}', '\u{2DF3}', '\u{2DF4}', '\u{2DF5}', '\u{2DF6}', '\u{2DF7}', '\u{2DF8}', '\u{2DF9}', '\u{2DFA}',
    '\u{2DFB}', '\u{2DFC}', '\u{2DFD}', '\u{2DFE}', '\u{2DFF}', '\u{A66F}', '\u{A67C}', '\u{A67D}', '\u{A6F0}', '\u{A6F1}', '\u{A8E0}', '\u{A8E1}',
    '\u{A8E2}', '\u{A8E3}', '\u{A8E4}', '\u{A8E5}', '\u{A8E6}', '\u{A8E7}', '\u{A8E8}', '\u{A8E9}', '\u{A8EA}', '\u{A8EB}', '\u{A8EC}', '\u{A8ED}',
    '\u{A8EE}', '\u{A8EF}', '\u{A8F0}', '\u{A8F1}', '\u{AAB0}', '\u{AAB2}', '\u{AAB3}', '\u{AAB7}', '\u{AAB8}', '\u{AABE}', '\u{AABF}', '\u{AAC1}',
    '\u{FE20}', '\u{FE21}', '\u{FE22}', '\u{FE23}', '\u{FE24}', '\u{FE25}', '\u{FE26}', '\u{10A0F}', '\u{10A38}', '\u{1D185}', '\u{1D186}', '\u{1D187}',
    '\u{1D188}', '\u{1D189}', '\u{1D1AA}', '\u{1D1AB}', '\u{1D1AC}', '\u{1D1AD}', '\u{1D242}', '\u{1D243}', '\u{1D244}',
];

/// The kitty Unicode placeholder grid: one U+10EEEE per cell with the row and
/// column diacritics, in the image id's colour. Ordinary text, so the picture
/// is clipped to the pane, redrawn with it and erased by whatever comes next.
fn placeholders(rows: u16, columns: u16, image_id: u32) -> Vec<u8> {
    let (r, g, b) = (
        (image_id >> 16) & 0xff,
        (image_id >> 8) & 0xff,
        image_id & 0xff,
    );
    let mut out = Vec::with_capacity(rows as usize * columns as usize * 10 + 32);
    let _ = write!(out, "\x1b[38;2;{r};{g};{b}m");
    // A row has to be spelled out explicitly, so past the table there is no way
    // to address it: such a terminal is not supported.
    for row in 0..rows.min(DIACRITICS.len() as u16) {
        let _ = write!(out, "\x1b[{};1H", row + 1);
        let row_mark = DIACRITICS[row as usize];
        for column in 0..columns {
            let _ = write!(out, "\u{10EEEE}{row_mark}");
            // Past the table a column is the previous one plus one, which the
            // protocol infers when its diacritic is omitted.
            if let Some(column_mark) = DIACRITICS.get(column as usize) {
                let _ = write!(out, "{column_mark}");
            }
        }
    }
    let _ = write!(out, "\x1b[0m");
    out
}

/// Appends an RGBA frame as chunked `t=d,o=z` escape codes, then a one pixel
/// query with id `fence` whose reply tells us the terminal got this far.
fn write_inline_frame(
    command: &mut Vec<u8>,
    pixels: &[u8],
    width: u32,
    height: u32,
    fence: u32,
    image_id: u32,
) -> Result<()> {
    command.extend_from_slice(b"\x1b[?2026h\x1b[H");
    write_chunked(
        command,
        &format!("a=T,i={image_id},U=1,f=32,o=z,t=d,s={width},v={height},q=2"),
        "",
        [pixels],
    )?;
    write_fence(command, fence)
}

/// Appends the `rects` of an RGBA frame `width` pixels wide as edits of the
/// shown frame (`a=f,r=1`), then the fence like `write_inline_frame`.
fn write_frame_edits(
    command: &mut Vec<u8>,
    pixels: &[u8],
    width: u32,
    rects: &[Rect],
    fence: u32,
) -> Result<()> {
    let stride = width as usize * 4;
    command.extend_from_slice(b"\x1b[?2026h");
    for rect in rects {
        let (start, end) = (rect.x as usize * 4, (rect.x + rect.width) as usize * 4);
        let rows = (rect.y..rect.y + rect.height).map(|row| {
            let row = row as usize * stride;
            &pixels[row + start..row + end]
        });
        // X=1 replaces the pixels instead of alpha blending onto them.
        write_chunked(
            command,
            &format!(
                "a=f,i=1,r=1,x={},y={},s={},v={},X=1,f=32,o=z,t=d,q=2",
                rect.x, rect.y, rect.width, rect.height
            ),
            // kitty up to at least 0.42 reads the frame number from the last
            // chunk, so without `r=1` there the edit becomes a new frame.
            "a=f,r=1,",
            rows,
        )?;
    }
    write_fence(command, fence)
}

/// Appends `data` zlib-compressed and base64-encoded as escape codes of at
/// most `CHUNK_SIZE` payload bytes. The first code carries `keys`, the others
/// only `continuation` (the spec wants `a=f` repeated on every frame chunk).
fn write_chunked<'a>(
    command: &mut Vec<u8>,
    keys: &str,
    continuation: &str,
    data: impl IntoIterator<Item = &'a [u8]>,
) -> Result<()> {
    let mut encoder = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::fast());
    for slice in data {
        encoder.write_all(slice)?;
    }
    let payload = base64::engine::general_purpose::STANDARD.encode(encoder.finish()?);
    let mut chunks = payload.as_bytes().chunks(CHUNK_SIZE).peekable();
    let mut first = true;
    while let Some(chunk) = chunks.next() {
        let more = u8::from(chunks.peek().is_some());
        if first {
            write!(command, "\x1b_G{keys},m={more};")?;
            first = false;
        } else {
            write!(command, "\x1b_G{continuation}m={more},q=2;")?;
        }
        command.extend_from_slice(chunk);
        command.extend_from_slice(b"\x1b\\");
    }
    Ok(())
}

fn write_fence(command: &mut Vec<u8>, fence: u32) -> Result<()> {
    write!(
        command,
        "\x1b[?2026l\x1b_Ga=q,i={fence},s=1,v=1,f=24,t=d;AAAA\x1b\\"
    )?;
    Ok(())
}

/// The regions where two RGBA frames of the same size differ, on a grid of
/// `DIRTY_TILE` pixels. Adjacent changed tiles in a row of tiles become one
/// rect, and a rect directly below one with the same columns extends it, so a
/// scrolled region is a single edit.
fn dirty_rects(previous: &[u8], next: &[u8], width: u32, height: u32) -> Vec<Rect> {
    let stride = width as usize * 4;
    let tile_bytes = DIRTY_TILE as usize * 4;
    let columns = width.div_ceil(DIRTY_TILE) as usize;
    let mut dirty = vec![false; columns];
    let mut rects: Vec<Rect> = Vec::new();
    for band in (0..height).step_by(DIRTY_TILE as usize) {
        let band_height = DIRTY_TILE.min(height - band);
        dirty.fill(false);
        for row in band..band + band_height {
            let start = row as usize * stride;
            let (before, after) = (
                &previous[start..start + stride],
                &next[start..start + stride],
            );
            if before == after {
                continue;
            }
            for (column, dirty) in dirty.iter_mut().enumerate() {
                let x = column * tile_bytes;
                let end = (x + tile_bytes).min(stride);
                *dirty = *dirty || before[x..end] != after[x..end];
            }
        }
        let mut column = 0;
        while column < columns {
            if !dirty[column] {
                column += 1;
                continue;
            }
            let first = column;
            while column < columns && dirty[column] {
                column += 1;
            }
            let x = first as u32 * DIRTY_TILE;
            let run_width = (column as u32 * DIRTY_TILE).min(width) - x;
            match rects
                .iter_mut()
                .find(|rect| rect.x == x && rect.width == run_width && rect.y + rect.height == band)
            {
                Some(above) => above.height += band_height,
                None => rects.push(Rect {
                    x,
                    y: band,
                    width: run_width,
                    height: band_height,
                }),
            }
        }
    }
    rects
}

/// macOS refuses write(2) on shared memory, so the object is always mapped.
/// On Linux the pages are allocated up front: a full `/dev/shm` (64 MB in
/// Docker by default) then fails here with ENOSPC instead of a SIGBUS on write.
fn map_shared_memory(name: &CString, length: usize) -> Result<*mut libc::c_void> {
    let fd = unsafe {
        libc::shm_open(
            name.as_ptr(),
            libc::O_CREAT | libc::O_EXCL | libc::O_RDWR,
            0o600 as libc::c_uint,
        )
    };
    if fd < 0 {
        return Err(io::Error::last_os_error()).context("shm_open");
    }
    let result = if let Err(error) = allocate(fd, length) {
        Err(error)
    } else {
        let mapping = unsafe {
            libc::mmap(
                ptr::null_mut(),
                length,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_SHARED,
                fd,
                0,
            )
        };
        if mapping == libc::MAP_FAILED {
            Err(io::Error::last_os_error()).context("mmap")
        } else {
            Ok(mapping)
        }
    };
    unsafe { libc::close(fd) };
    if result.is_err() {
        unsafe { libc::shm_unlink(name.as_ptr()) };
    }
    result
}

#[cfg(target_os = "macos")]
fn allocate(fd: libc::c_int, length: usize) -> Result<()> {
    if unsafe { libc::ftruncate(fd, length as libc::off_t) } != 0 {
        return Err(io::Error::last_os_error()).context("ftruncate");
    }
    Ok(())
}

#[cfg(not(target_os = "macos"))]
fn allocate(fd: libc::c_int, length: usize) -> Result<()> {
    // posix_fallocate returns the error number instead of setting errno.
    match unsafe { libc::posix_fallocate(fd, 0, length as libc::off_t) } {
        0 => Ok(()),
        error => Err(io::Error::from_raw_os_error(error)).context("posix_fallocate"),
    }
}

#[cfg(test)]
mod tests {
    use std::io::Read as _;

    use super::*;

    #[test]
    fn inline_frames_are_chunked_and_fenced() {
        let (width, height) = (64u32, 48u32);
        // Noise, so the compressed payload spans several chunks.
        let pixels = noise(width * height * 4, 0x9e37_79b9);
        let mut command = Vec::new();
        write_inline_frame(&mut command, &pixels, width, height, 77, 1).unwrap();
        let text = String::from_utf8(command).unwrap();

        let body = text
            .strip_prefix("\x1b[?2026h\x1b[H")
            .unwrap()
            .strip_suffix("\x1b[?2026l\x1b_Ga=q,i=77,s=1,v=1,f=24,t=d;AAAA\x1b\\")
            .unwrap();
        let codes: Vec<&str> = body
            .split("\x1b\\")
            .filter(|code| !code.is_empty())
            .collect();
        assert!(codes.len() > 1, "expected several chunks");
        let mut payload = String::new();
        for (index, code) in codes.iter().enumerate() {
            let (keys, chunk) = code
                .strip_prefix("\x1b_G")
                .unwrap()
                .split_once(';')
                .unwrap();
            let last = index == codes.len() - 1;
            let expected_keys = match (index, last) {
                (0, _) => format!(
                    "a=T,i=1,U=1,f=32,o=z,t=d,s={width},v={height},q=2,m={}",
                    u8::from(!last)
                ),
                (_, false) => "m=1,q=2".to_owned(),
                (_, true) => "m=0,q=2".to_owned(),
            };
            assert_eq!(keys, expected_keys);
            assert!(chunk.len() <= CHUNK_SIZE);
            if !last {
                assert_eq!(chunk.len() % 4, 0);
            }
            payload.push_str(chunk);
        }

        assert_eq!(inflate(&payload), pixels);
    }

    #[test]
    fn frame_edits_patch_only_changed_regions() {
        let (width, height) = (200u32, 150u32);
        let previous = noise(width * height * 4, 7);
        let mut next = previous.clone();
        let mut paint = |x: u32, y: u32, w: u32, h: u32| {
            for row in y..y + h {
                for column in x..x + w {
                    let index = ((row * width + column) * 4) as usize;
                    next[index] ^= 0xff;
                }
            }
        };
        // A hover in the first tile, a band that scrolls, and the clipped corner.
        paint(3, 3, 2, 2);
        paint(64, 40, 70, 60);
        paint(199, 149, 1, 1);

        let rects = dirty_rects(&previous, &next, width, height);
        assert_eq!(
            rects,
            vec![
                Rect {
                    x: 0,
                    y: 0,
                    width: 32,
                    height: 32
                },
                Rect {
                    x: 64,
                    y: 32,
                    width: 96,
                    height: 96
                },
                Rect {
                    x: 192,
                    y: 128,
                    width: 8,
                    height: 22
                },
            ]
        );

        let mut command = Vec::new();
        write_frame_edits(&mut command, &next, width, &rects, 9).unwrap();
        let text = String::from_utf8(command).unwrap();
        let body = text
            .strip_prefix("\x1b[?2026h")
            .unwrap()
            .strip_suffix("\x1b[?2026l\x1b_Ga=q,i=9,s=1,v=1,f=24,t=d;AAAA\x1b\\")
            .unwrap();

        // Replay the edits onto the previous frame, like the terminal does.
        let mut shown = previous.clone();
        let mut edit: Option<(Vec<u32>, String)> = None;
        for code in body.split("\x1b\\").filter(|code| !code.is_empty()) {
            let (keys, chunk) = code
                .strip_prefix("\x1b_G")
                .unwrap()
                .split_once(';')
                .unwrap();
            assert!(chunk.len() <= CHUNK_SIZE);
            let more = match edit.as_mut() {
                None => {
                    let rest = keys
                        .strip_prefix("a=f,i=1,r=1,")
                        .unwrap()
                        .strip_suffix(",X=1,f=32,o=z,t=d,q=2,m=0")
                        .or_else(|| {
                            keys.strip_prefix("a=f,i=1,r=1,")?
                                .strip_suffix(",X=1,f=32,o=z,t=d,q=2,m=1")
                        })
                        .unwrap();
                    let numbers = rest
                        .split(',')
                        .map(|key| key[2..].parse().unwrap())
                        .collect();
                    edit = Some((numbers, chunk.to_owned()));
                    keys.ends_with("m=1")
                }
                Some((_, payload)) => {
                    assert!(
                        keys == "a=f,r=1,m=1,q=2" || keys == "a=f,r=1,m=0,q=2",
                        "{keys}"
                    );
                    payload.push_str(chunk);
                    keys == "a=f,r=1,m=1,q=2"
                }
            };
            if more {
                continue;
            }
            let (numbers, payload) = edit.take().unwrap();
            let [x, y, w, h] = numbers[..] else { panic!() };
            let data = inflate(&payload);
            assert_eq!(data.len(), (w * h * 4) as usize);
            for row in 0..h {
                let to = (((y + row) * width + x) * 4) as usize;
                let from = (row * w * 4) as usize;
                shown[to..to + (w * 4) as usize]
                    .copy_from_slice(&data[from..from + (w * 4) as usize]);
            }
        }
        assert!(edit.is_none());
        assert!(shown == next, "the replayed edits must give the new frame");
    }

    fn noise(length: u32, mut seed: u32) -> Vec<u8> {
        (0..length)
            .map(|_| {
                seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                (seed >> 24) as u8
            })
            .collect()
    }

    fn inflate(payload: &str) -> Vec<u8> {
        let compressed = base64::engine::general_purpose::STANDARD
            .decode(payload)
            .unwrap();
        let mut decoded = Vec::new();
        flate2::read::ZlibDecoder::new(compressed.as_slice())
            .read_to_end(&mut decoded)
            .unwrap();
        decoded
    }

    #[test]
    fn placeholders_cover_the_grid_in_the_image_colour() {
        let grid = String::from_utf8(placeholders(2, 3, 1)).unwrap();
        // The image id has to travel in the cell's foreground colour.
        assert!(grid.starts_with("\x1b[38;2;0;0;1m"));
        assert_eq!(grid.matches('\u{10EEEE}').count(), 6);
        assert!(grid.contains("\x1b[1;1H"));
        assert!(grid.contains("\x1b[2;1H"));
        // Row and column diacritics follow every placeholder.
        assert!(grid.contains("\u{10EEEE}\u{0305}\u{0305}"));
        assert!(grid.contains("\u{10EEEE}\u{030D}\u{0305}"));
    }

    #[test]
    fn columns_past_the_table_inherit_the_previous_one() {
        let grid = String::from_utf8(placeholders(1, 298, 1)).unwrap();
        // The last cell carries only the row diacritic: the protocol then reads
        // the column as the previous one plus one.
        let last = grid.rsplit('\u{10EEEE}').next().unwrap();
        assert_eq!(last, "\u{0305}\x1b[0m");
    }
}
