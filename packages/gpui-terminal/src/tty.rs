//! The controlling terminal: raw mode, the modes GPUIX turns on, window size,
//! and a parser that turns terminal input bytes into GPUI-shaped events.

use std::{
    fs::File,
    io::{self, Read as _, Write as _},
    os::fd::{AsRawFd, RawFd},
    ptr,
    sync::{
        Mutex, OnceLock,
        atomic::{AtomicI32, Ordering},
    },
    thread,
    time::Duration,
};

use anyhow::{Context as _, Result};
use gpui::{Keystroke, Modifiers, MouseButton, NavigationDirection};

use crate::dispatcher::{MainMessage, MainSender};

/// Kitty keyboard flags: disambiguate (1), report event types (2), report
/// alternate keys (4), report all keys as escape codes (8), report associated
/// text (16). With 8 every key, including plain text and Escape, arrives as an
/// unambiguous `CSI ... u`, so there is no lone-Escape timeout in practice.
/// https://sw.kovidgoyal.net/kitty/keyboard-protocol/#progressive-enhancement
const KITTY_KEYBOARD_FLAGS: u32 = 1 | 2 | 4 | 8 | 16;

/// Alternate screen, hidden cursor, any-motion mouse in SGR pixel mode, focus
/// reports, bracketed paste, in-band resize reports, kitty keyboard, then the
/// size and mode queries whose replies the parser reads.
fn enter_sequence() -> String {
    format!(
        "\x1b[?1049h\x1b[?25l\x1b[2J\
         \x1b[?1003h\x1b[?1006h\x1b[?1016h\
         \x1b[?1004h\x1b[?2004h\x1b[?2048h\
         \x1b[>{KITTY_KEYBOARD_FLAGS}u\
         \x1b[?1016$p"
    )
}

const EXIT_SEQUENCE: &str = "\x1b_Ga=d,d=i,i=1\x1b\\\
     \x1b[<u\
     \x1b[?2048l\x1b[?2004l\x1b[?1004l\
     \x1b[?1016l\x1b[?1006l\x1b[?1003l\
     \x1b[?25h\x1b[?1049l";

/// Wraps every kitty graphics APC in tmux's passthrough envelope, a DCS with
/// the `tmux;` prefix in which each ESC of the wrapped code is doubled. tmux
/// has no graphics protocol of its own and drops raw `ESC _ G` sequences.
pub(crate) fn tmux_passthrough(command: &[u8]) -> Vec<u8> {
    const APC: &[u8] = b"\x1b_G";
    const ST: &[u8] = b"\x1b\\";

    let mut wrapped = Vec::with_capacity(command.len() + 32);
    let mut rest = command;
    while let Some(start) = rest.windows(APC.len()).position(|w| w == APC) {
        wrapped.extend_from_slice(&rest[..start]);
        let apc = &rest[start..];
        let end = apc
            .windows(ST.len())
            .position(|w| w == ST)
            .map(|index| index + ST.len())
            .unwrap_or(apc.len());
        wrapped.extend_from_slice(b"\x1bPtmux;");
        for &byte in &apc[..end] {
            if byte == b'\x1b' {
                wrapped.push(b'\x1b');
            }
            wrapped.push(byte);
        }
        wrapped.extend_from_slice(ST);
        rest = &apc[end..];
    }
    wrapped.extend_from_slice(rest);
    wrapped
}

/// How long a lone ESC waits for the rest of an escape sequence. Only legacy
/// terminals send a bare ESC once the kitty keyboard protocol is on.
const ESCAPE_TIMEOUT: Duration = Duration::from_millis(20);

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct TerminalSize {
    pub columns: u16,
    pub rows: u16,
    pub width: u32,
    pub height: u32,
}

impl TerminalSize {
    pub(crate) fn cell_size(&self) -> (f32, f32) {
        (
            self.width as f32 / self.columns.max(1) as f32,
            self.height as f32 / self.rows.max(1) as f32,
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum KeyEventKind {
    Press,
    Repeat,
    Release,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MouseEventKind {
    Press(MouseButton),
    Release(MouseButton),
    Move(Option<MouseButton>),
    Wheel { dx: i8, dy: i8 },
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum TerminalEvent {
    Key {
        keystroke: Keystroke,
        kind: KeyEventKind,
    },
    /// A modifier key changed state. Only reported with the kitty protocol.
    Modifiers(Modifiers),
    /// `x` and `y` are terminal pixels when `pixels` is set, else 0-based cells.
    Mouse {
        kind: MouseEventKind,
        x: f32,
        y: f32,
        pixels: bool,
        modifiers: Modifiers,
    },
    Paste(String),
    Focus(bool),
    Resize(TerminalSize),
    /// Reply to the `CSI ? 1016 $ p` query: whether SGR pixel mouse works.
    PixelMouseSupported(bool),
    /// `ESC _ G i=<id>;<OK or error> ESC \`, the reply to a kitty graphics command.
    GraphicsReply {
        id: u32,
        ok: bool,
    },
    /// Reply to `CSI c` (primary device attributes). Terminals answer in
    /// order, so every reply to an earlier query has arrived before it.
    DeviceAttributes,
    Quit,
}

struct Restore {
    fd: RawFd,
    original: libc::termios,
    captured: Option<CapturedOutput>,
}

/// stdout and stderr, when they are this terminal, go to an unlinked temp file
/// while the app runs. Any print would otherwise scroll the alternate screen
/// and the frame with it. The file is replayed after the terminal is restored.
struct CapturedOutput {
    file: RawFd,
    /// `(redirected fd, dup of the original)`.
    saved: Vec<(RawFd, RawFd)>,
}

impl CapturedOutput {
    fn start() -> Option<Self> {
        let targets: Vec<RawFd> = [libc::STDOUT_FILENO, libc::STDERR_FILENO]
            .into_iter()
            .filter(|fd| unsafe { libc::isatty(*fd) } == 1)
            .collect();
        if targets.is_empty() {
            return None;
        }
        let directory = std::env::var("TMPDIR").unwrap_or_else(|_| "/tmp".to_owned());
        let template = format!(
            "{}/gpui-terminal-output.XXXXXX",
            directory.trim_end_matches('/')
        );
        let mut template = std::ffi::CString::new(template).ok()?.into_bytes_with_nul();
        let file = unsafe { libc::mkstemp(template.as_mut_ptr() as *mut libc::c_char) };
        if file < 0 {
            log::error!(
                "cannot capture terminal output: {}",
                io::Error::last_os_error()
            );
            return None;
        }
        unsafe { libc::unlink(template.as_ptr() as *const libc::c_char) };
        let saved = targets
            .into_iter()
            .filter_map(|target| {
                let original = unsafe { libc::dup(target) };
                if original < 0 || unsafe { libc::dup2(file, target) } < 0 {
                    return None;
                }
                Some((target, original))
            })
            .collect();
        Some(Self { file, saved })
    }

    fn finish(self) {
        for (target, original) in &self.saved {
            unsafe {
                libc::dup2(*original, *target);
                libc::close(*original);
            }
        }
        let mut buffer = [0u8; 16 * 1024];
        unsafe { libc::lseek(self.file, 0, libc::SEEK_SET) };
        loop {
            let count = unsafe {
                libc::read(
                    self.file,
                    buffer.as_mut_ptr() as *mut libc::c_void,
                    buffer.len(),
                )
            };
            if count <= 0 {
                break;
            }
            let mut written = 0;
            while written < count as usize {
                let result = unsafe {
                    libc::write(
                        libc::STDERR_FILENO,
                        buffer[written..].as_ptr() as *const libc::c_void,
                        count as usize - written,
                    )
                };
                if result <= 0 {
                    break;
                }
                written += result as usize;
            }
        }
        unsafe { libc::close(self.file) };
    }
}

/// Kept global so the panic hook and signal path can put the terminal back
/// even when the platform is mid-borrow.
static RESTORE: Mutex<Option<Restore>> = Mutex::new(None);
static SIGNAL_PIPE_WRITE: AtomicI32 = AtomicI32::new(-1);

pub(crate) struct Tty {
    file: File,
}

impl Tty {
    /// Opens the controlling terminal and switches it to raw mode.
    pub(crate) fn open() -> Result<Self> {
        let file = File::options()
            .read(true)
            .write(true)
            .open("/dev/tty")
            .context("gpui_terminal needs a controlling terminal (/dev/tty)")?;
        let fd = file.as_raw_fd();

        let mut original = unsafe { std::mem::zeroed::<libc::termios>() };
        if unsafe { libc::tcgetattr(fd, &mut original) } != 0 {
            return Err(io::Error::last_os_error()).context("tcgetattr failed");
        }
        let mut raw = original;
        unsafe { libc::cfmakeraw(&mut raw) };
        if unsafe { libc::tcsetattr(fd, libc::TCSANOW, &raw) } != 0 {
            return Err(io::Error::last_os_error()).context("tcsetattr failed");
        }
        *RESTORE.lock().unwrap_or_else(|error| error.into_inner()) = Some(Restore {
            fd,
            original,
            captured: CapturedOutput::start(),
        });
        install_restore_hooks();

        let mut tty = Self { file };
        tty.write_all(enter_sequence().as_bytes())?;
        Ok(tty)
    }

    pub(crate) fn write_all(&mut self, bytes: &[u8]) -> Result<()> {
        self.file
            .write_all(bytes)
            .context("writing to the terminal")
    }

    pub(crate) fn size(&self) -> Result<TerminalSize> {
        let mut size = unsafe { std::mem::zeroed::<libc::winsize>() };
        if unsafe { libc::ioctl(self.file.as_raw_fd(), libc::TIOCGWINSZ, &mut size) } != 0 {
            return Err(io::Error::last_os_error()).context("TIOCGWINSZ failed");
        }
        Ok(TerminalSize {
            columns: size.ws_col,
            rows: size.ws_row,
            width: size.ws_xpixel as u32,
            height: size.ws_ypixel as u32,
        })
    }

    pub(crate) fn spawn_input_thread(&self, sender: MainSender) -> Result<()> {
        let reader = self
            .file
            .try_clone()
            .context("cloning the terminal handle")?;
        let signal_read = install_signal_pipe()?;
        thread::Builder::new()
            .name("TerminalInput".to_owned())
            .spawn(move || {
                if let Err(error) = read_input(reader, signal_read, &sender) {
                    log::error!("terminal input stopped: {error:#}");
                    sender.send(MainMessage::Terminal(TerminalEvent::Quit)).ok();
                }
            })
            .context("spawning the terminal input thread")?;
        Ok(())
    }
}

impl Drop for Tty {
    fn drop(&mut self) {
        restore_terminal();
    }
}

pub(crate) fn restore_terminal() {
    let Some(restore) = RESTORE
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .take()
    else {
        return;
    };
    let sequence = if std::env::var_os("TMUX").is_some() {
        tmux_passthrough(EXIT_SEQUENCE.as_bytes())
    } else {
        EXIT_SEQUENCE.as_bytes().to_vec()
    };
    unsafe {
        libc::write(
            restore.fd,
            sequence.as_ptr() as *const libc::c_void,
            sequence.len(),
        );
        libc::tcsetattr(restore.fd, libc::TCSANOW, &restore.original);
    }
    if let Some(captured) = restore.captured {
        captured.finish();
    }
}

extern "C" fn restore_terminal_at_exit() {
    restore_terminal();
}

/// Panics, and `exit(3)` without unwinding (Node's `process.exit`), must still
/// leave raw mode and the alternate screen.
fn install_restore_hooks() {
    static INSTALLED: OnceLock<()> = OnceLock::new();
    INSTALLED.get_or_init(|| {
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            restore_terminal();
            previous(info);
        }));
        unsafe { libc::atexit(restore_terminal_at_exit) };
    });
}

const SIGNAL_RESIZE: u8 = b'w';
const SIGNAL_QUIT: u8 = b'q';

/// The SIGWINCH handler installed before ours, e.g. Node's libuv one when GPUI
/// is embedded. It keeps running so the host still sees resizes. Quit signals
/// are not forwarded: a host handler may exit on the spot, before the terminal
/// is restored. Quitting the app restores it, then the host exits.
static PREVIOUS_RESIZE_ACTION: OnceLock<libc::sigaction> = OnceLock::new();

extern "C" fn on_signal(
    signal: libc::c_int,
    info: *mut libc::siginfo_t,
    context: *mut libc::c_void,
) {
    let fd = SIGNAL_PIPE_WRITE.load(Ordering::Relaxed);
    if fd >= 0 {
        let byte = if signal == libc::SIGWINCH {
            SIGNAL_RESIZE
        } else {
            SIGNAL_QUIT
        };
        // write(2) is async-signal-safe; a full pipe already holds a pending wake.
        unsafe { libc::write(fd, &byte as *const u8 as *const libc::c_void, 1) };
    }

    if signal != libc::SIGWINCH {
        return;
    }
    let Some(previous) = PREVIOUS_RESIZE_ACTION.get() else {
        return;
    };
    let handler = previous.sa_sigaction;
    if handler == libc::SIG_DFL || handler == libc::SIG_IGN {
        return;
    }
    unsafe {
        if previous.sa_flags & libc::SA_SIGINFO != 0 {
            let handler: extern "C" fn(libc::c_int, *mut libc::siginfo_t, *mut libc::c_void) =
                std::mem::transmute(handler);
            handler(signal, info, context);
        } else {
            let handler: extern "C" fn(libc::c_int) = std::mem::transmute(handler);
            handler(signal);
        }
    }
}

fn install_signal_pipe() -> Result<RawFd> {
    let mut fds = [0 as RawFd; 2];
    if unsafe { libc::pipe(fds.as_mut_ptr()) } != 0 {
        return Err(io::Error::last_os_error()).context("creating the signal pipe");
    }
    unsafe {
        libc::fcntl(fds[1], libc::F_SETFL, libc::O_NONBLOCK);
    }
    SIGNAL_PIPE_WRITE.store(fds[1], Ordering::Relaxed);
    let mut action = unsafe { std::mem::zeroed::<libc::sigaction>() };
    action.sa_sigaction = on_signal as *const () as libc::sighandler_t;
    action.sa_flags = libc::SA_SIGINFO | libc::SA_RESTART;
    unsafe { libc::sigemptyset(&mut action.sa_mask) };
    for signal in [libc::SIGWINCH, libc::SIGTERM, libc::SIGHUP, libc::SIGINT] {
        let mut previous = unsafe { std::mem::zeroed::<libc::sigaction>() };
        unsafe { libc::sigaction(signal, &action, &mut previous) };
        if signal == libc::SIGWINCH {
            PREVIOUS_RESIZE_ACTION.get_or_init(|| previous);
        }
    }
    Ok(fds[0])
}

fn read_input(mut reader: File, signal_read: RawFd, sender: &MainSender) -> Result<()> {
    let tty_fd = reader.as_raw_fd();
    let mut parser = InputParser::default();
    let mut buffer = vec![0u8; 64 * 1024];
    let mut events = Vec::new();
    loop {
        // select(2), not poll(2): macOS poll reports POLLNVAL for /dev/tty.
        let mut read_set = unsafe { std::mem::zeroed::<libc::fd_set>() };
        unsafe {
            libc::FD_ZERO(&mut read_set);
            libc::FD_SET(tty_fd, &mut read_set);
            libc::FD_SET(signal_read, &mut read_set);
        }
        let mut timeout = libc::timeval {
            tv_sec: 0,
            tv_usec: ESCAPE_TIMEOUT.as_micros() as libc::suseconds_t,
        };
        let timeout = if parser.has_pending_escape() {
            &mut timeout as *mut libc::timeval
        } else {
            ptr::null_mut()
        };
        let ready = unsafe {
            libc::select(
                tty_fd.max(signal_read) + 1,
                &mut read_set,
                ptr::null_mut(),
                ptr::null_mut(),
                timeout,
            )
        };
        if ready < 0 {
            let error = io::Error::last_os_error();
            if error.kind() == io::ErrorKind::Interrupted {
                continue;
            }
            return Err(error).context("waiting for terminal input");
        }
        if ready == 0 {
            parser.flush_pending_escape(&mut events);
        }

        if unsafe { libc::FD_ISSET(signal_read, &read_set) } {
            let mut signals = [0u8; 16];
            let count = unsafe {
                libc::read(
                    signal_read,
                    signals.as_mut_ptr() as *mut libc::c_void,
                    signals.len(),
                )
            };
            let signals = &signals[..count.max(0) as usize];
            if signals.contains(&SIGNAL_QUIT) {
                events.push(TerminalEvent::Quit);
            } else if signals.contains(&SIGNAL_RESIZE) {
                let mut size = unsafe { std::mem::zeroed::<libc::winsize>() };
                if unsafe { libc::ioctl(tty_fd, libc::TIOCGWINSZ, &mut size) } == 0 {
                    events.push(TerminalEvent::Resize(TerminalSize {
                        columns: size.ws_col,
                        rows: size.ws_row,
                        width: size.ws_xpixel as u32,
                        height: size.ws_ypixel as u32,
                    }));
                }
            }
        }

        if unsafe { libc::FD_ISSET(tty_fd, &read_set) } {
            let count = match reader.read(&mut buffer) {
                Ok(0) => anyhow::bail!("the terminal closed"),
                Ok(count) => count,
                Err(error) if error.kind() == io::ErrorKind::Interrupted => 0,
                Err(error) => return Err(error).context("reading the terminal"),
            };
            parser.feed(&buffer[..count], &mut events);
        }

        for event in events.drain(..) {
            let quit = event == TerminalEvent::Quit;
            if sender.send(MainMessage::Terminal(event)).is_err() || quit {
                return Ok(());
            }
        }
    }
}

const PASTE_START: &[u8] = b"\x1b[200~";
const PASTE_END: &[u8] = b"\x1b[201~";
/// Longest escape sequence the parser waits for before discarding a prefix.
const MAX_SEQUENCE_LENGTH: usize = 4096;

#[derive(Default)]
pub(crate) struct InputParser {
    buffer: Vec<u8>,
    paste: Option<Vec<u8>>,
    pressed_button: Option<MouseButton>,
}

enum Parsed {
    Event(usize, Option<TerminalEvent>),
    Incomplete,
}

impl InputParser {
    pub(crate) fn feed(&mut self, bytes: &[u8], events: &mut Vec<TerminalEvent>) {
        self.buffer.extend_from_slice(bytes);
        let mut start = 0;
        while start < self.buffer.len() {
            if let Some(paste) = &mut self.paste {
                let rest = &self.buffer[start..];
                match find(rest, PASTE_END) {
                    Some(end) => {
                        paste.extend_from_slice(&rest[..end]);
                        let text = String::from_utf8_lossy(paste).replace("\r\n", "\n");
                        events.push(TerminalEvent::Paste(text.replace('\r', "\n")));
                        self.paste = None;
                        start += end + PASTE_END.len();
                    }
                    None => {
                        // Keep a tail that might be the start of the end marker.
                        let keep = PASTE_END.len().min(rest.len());
                        paste.extend_from_slice(&rest[..rest.len() - keep]);
                        start = self.buffer.len() - keep;
                        break;
                    }
                }
                continue;
            }
            match self.parse(start) {
                Parsed::Event(length, event) => {
                    if let Some(event) = event {
                        events.push(event);
                    }
                    start += length;
                }
                Parsed::Incomplete => {
                    if self.buffer.len() - start > MAX_SEQUENCE_LENGTH {
                        start += 1;
                        continue;
                    }
                    break;
                }
            }
        }
        self.buffer.drain(..start);
    }

    pub(crate) fn has_pending_escape(&self) -> bool {
        self.paste.is_none() && self.buffer.first() == Some(&0x1b)
    }

    /// Called when a partial sequence timed out: a lone ESC is the Escape key,
    /// and ESC + byte is Alt + that key.
    pub(crate) fn flush_pending_escape(&mut self, events: &mut Vec<TerminalEvent>) {
        if !self.has_pending_escape() {
            return;
        }
        if self.buffer.len() == 1 {
            events.push(key_event(named_keystroke("escape", Modifiers::default())));
            self.buffer.clear();
        } else {
            self.buffer.remove(0);
            let mut alt_events = Vec::new();
            self.feed(&[], &mut alt_events);
            for mut event in alt_events {
                if let TerminalEvent::Key { keystroke, .. } = &mut event {
                    keystroke.modifiers.alt = true;
                    keystroke.key_char = None;
                }
                events.push(event);
            }
        }
    }

    fn parse(&mut self, start: usize) -> Parsed {
        let bytes = &self.buffer[start..];
        let Some(&first) = bytes.first() else {
            return Parsed::Incomplete;
        };
        if first != 0x1b {
            return parse_legacy_byte(bytes);
        }
        let Some(&second) = bytes.get(1) else {
            return Parsed::Incomplete;
        };
        match second {
            b'[' => {
                if bytes.starts_with(PASTE_START) {
                    self.paste = Some(Vec::new());
                    return Parsed::Event(PASTE_START.len(), None);
                }
                // CSI: parameter bytes 0x30–0x3f, intermediates 0x20–0x2f, final 0x40–0x7e.
                let Some(final_index) = bytes[2..]
                    .iter()
                    .position(|byte| (0x40..=0x7e).contains(byte))
                    .map(|index| index + 2)
                else {
                    return Parsed::Incomplete;
                };
                let params = bytes[2..final_index].to_vec();
                let final_byte = bytes[final_index];
                let event = self.parse_csi(&params, final_byte);
                Parsed::Event(final_index + 1, event)
            }
            b']' | b'_' | b'P' => {
                // OSC, APC (kitty graphics replies) and DCS end with ST or BEL.
                for index in 2..bytes.len() {
                    let length = if bytes[index] == 0x07 {
                        index + 1
                    } else if bytes[index] == 0x1b && bytes.get(index + 1) == Some(&b'\\') {
                        index + 2
                    } else {
                        continue;
                    };
                    let event = if second == b'_' && bytes.get(2) == Some(&b'G') {
                        parse_graphics_reply(&bytes[3..index])
                    } else {
                        None
                    };
                    return Parsed::Event(length, event);
                }
                Parsed::Incomplete
            }
            b'O' => {
                let Some(&key) = bytes.get(2) else {
                    return Parsed::Incomplete;
                };
                let event = legacy_final_key(key)
                    .map(|name| key_event(named_keystroke(name, Modifiers::default())));
                Parsed::Event(3, event)
            }
            0x1b => Parsed::Event(
                1,
                Some(key_event(named_keystroke("escape", Modifiers::default()))),
            ),
            _ => match parse_legacy_byte(&bytes[1..]) {
                Parsed::Event(length, event) => Parsed::Event(
                    length + 1,
                    event.map(|mut event| {
                        if let TerminalEvent::Key { keystroke, .. } = &mut event {
                            keystroke.modifiers.alt = true;
                            keystroke.key_char = None;
                        }
                        event
                    }),
                ),
                Parsed::Incomplete => Parsed::Incomplete,
            },
        }
    }

    fn parse_csi(&mut self, params: &[u8], final_byte: u8) -> Option<TerminalEvent> {
        let params = std::str::from_utf8(params).ok()?;
        match final_byte {
            b'M' | b'm' if params.starts_with('<') => {
                self.parse_sgr_mouse(&params[1..], final_byte == b'M')
            }
            b'u' if params.starts_with('?') => None,
            b'u' => parse_kitty_key(params),
            b'~' => {
                let (code, modifiers, kind) = split_key_params(params)?;
                let name = match code {
                    2 => "insert",
                    3 => "delete",
                    5 => "pageup",
                    6 => "pagedown",
                    1 | 7 => "home",
                    4 | 8 => "end",
                    11 => "f1",
                    12 => "f2",
                    13 => "f3",
                    14 => "f4",
                    15 => "f5",
                    17 => "f6",
                    18 => "f7",
                    19 => "f8",
                    20 => "f9",
                    21 => "f10",
                    23 => "f11",
                    24 => "f12",
                    _ => return None,
                };
                Some(TerminalEvent::Key {
                    keystroke: named_keystroke(name, modifiers),
                    kind,
                })
            }
            b'I' if params.is_empty() => Some(TerminalEvent::Focus(true)),
            b'O' if params.is_empty() => Some(TerminalEvent::Focus(false)),
            b't' => {
                let numbers: Vec<u32> = params
                    .split(';')
                    .filter_map(|part| part.parse().ok())
                    .collect();
                match numbers.as_slice() {
                    [48, rows, columns, height, width] => {
                        Some(TerminalEvent::Resize(TerminalSize {
                            columns: *columns as u16,
                            rows: *rows as u16,
                            width: *width,
                            height: *height,
                        }))
                    }
                    _ => None,
                }
            }
            b'y' if params.starts_with("?1016;") => {
                // DECRPM: 0 means the mode is not recognized.
                let state = params.trim_start_matches("?1016;").trim_end_matches('$');
                Some(TerminalEvent::PixelMouseSupported(state != "0"))
            }
            b'c' if params.starts_with('?') => Some(TerminalEvent::DeviceAttributes),
            _ => {
                let name = legacy_final_key(final_byte)?;
                let (_, modifiers, kind) = if params.is_empty() {
                    (1, Modifiers::default(), KeyEventKind::Press)
                } else {
                    split_key_params(params)?
                };
                Some(TerminalEvent::Key {
                    keystroke: named_keystroke(name, modifiers),
                    kind,
                })
            }
        }
    }

    fn parse_sgr_mouse(&mut self, params: &str, press: bool) -> Option<TerminalEvent> {
        let mut parts = params.split(';');
        let code: u32 = parts.next()?.parse().ok()?;
        let x: f32 = parts.next()?.parse().ok()?;
        let y: f32 = parts.next()?.parse().ok()?;
        let modifiers = Modifiers {
            shift: code & 4 != 0,
            alt: code & 8 != 0,
            control: code & 16 != 0,
            ..Modifiers::default()
        };
        let button = match (code & 0b11) | ((code & 128) >> 5) {
            0 => Some(MouseButton::Left),
            1 => Some(MouseButton::Middle),
            2 => Some(MouseButton::Right),
            4 => Some(MouseButton::Navigate(NavigationDirection::Back)),
            5 => Some(MouseButton::Navigate(NavigationDirection::Forward)),
            _ => None,
        };
        let kind = if code & 64 != 0 {
            match code & 0b11 {
                0 => MouseEventKind::Wheel { dx: 0, dy: -1 },
                1 => MouseEventKind::Wheel { dx: 0, dy: 1 },
                2 => MouseEventKind::Wheel { dx: -1, dy: 0 },
                _ => MouseEventKind::Wheel { dx: 1, dy: 0 },
            }
        } else if code & 32 != 0 {
            MouseEventKind::Move(button.or(self.pressed_button))
        } else if press {
            let button = button?;
            self.pressed_button = Some(button);
            MouseEventKind::Press(button)
        } else {
            let button = button.or(self.pressed_button)?;
            self.pressed_button = None;
            MouseEventKind::Release(button)
        };
        // SGR coordinates are 1-based in both cell and pixel mode.
        Some(TerminalEvent::Mouse {
            kind,
            x: (x - 1.).max(0.),
            y: (y - 1.).max(0.),
            pixels: false,
            modifiers,
        })
    }
}

/// `i=31,p=1;OK` → the image id and whether the command succeeded.
fn parse_graphics_reply(body: &[u8]) -> Option<TerminalEvent> {
    let body = std::str::from_utf8(body).ok()?;
    let (keys, message) = body.split_once(';')?;
    let id = keys
        .split(',')
        .find_map(|key| key.strip_prefix("i="))?
        .parse()
        .ok()?;
    Some(TerminalEvent::GraphicsReply {
        id,
        ok: message == "OK",
    })
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

fn key_event(keystroke: Keystroke) -> TerminalEvent {
    TerminalEvent::Key {
        keystroke,
        kind: KeyEventKind::Press,
    }
}

fn named_keystroke(name: &str, modifiers: Modifiers) -> Keystroke {
    let key_char = match name {
        "space" => Some(" "),
        "tab" => Some("\t"),
        "enter" => Some("\n"),
        _ => None,
    }
    .filter(|_| !modifiers.control && !modifiers.platform)
    .map(str::to_owned);
    Keystroke {
        modifiers,
        key: name.to_owned(),
        key_char,
    }
}

fn legacy_final_key(final_byte: u8) -> Option<&'static str> {
    Some(match final_byte {
        b'A' => "up",
        b'B' => "down",
        b'C' => "right",
        b'D' => "left",
        b'H' => "home",
        b'F' => "end",
        b'P' => "f1",
        b'Q' => "f2",
        b'R' => "f3",
        b'S' => "f4",
        _ => return None,
    })
}

/// Bytes outside escape sequences: what a terminal sends without the kitty
/// protocol, or when it does not support it.
fn parse_legacy_byte(bytes: &[u8]) -> Parsed {
    let Some(&first) = bytes.first() else {
        return Parsed::Incomplete;
    };
    let control = Modifiers {
        control: true,
        ..Modifiers::default()
    };
    let keystroke = match first {
        b'\r' | b'\n' => named_keystroke("enter", Modifiers::default()),
        b'\t' => named_keystroke("tab", Modifiers::default()),
        0x7f | 0x08 => named_keystroke("backspace", Modifiers::default()),
        0x00 => named_keystroke("space", control),
        0x01..=0x1a => Keystroke {
            modifiers: control,
            key: ((first - 1 + b'a') as char).to_string(),
            key_char: None,
        },
        0x1b..=0x1f => return Parsed::Event(1, None),
        _ => {
            let length = utf8_length(first);
            if bytes.len() < length {
                return Parsed::Incomplete;
            }
            let Ok(text) = std::str::from_utf8(&bytes[..length]) else {
                return Parsed::Event(1, None);
            };
            return Parsed::Event(length, Some(key_event(text_keystroke(text, None))));
        }
    };
    Parsed::Event(1, Some(key_event(keystroke)))
}

fn utf8_length(first: u8) -> usize {
    match first {
        0xc0..=0xdf => 2,
        0xe0..=0xef => 3,
        0xf0..=0xf7 => 4,
        _ => 1,
    }
}

/// A key that types `text`. GPUI names a shifted letter by its lowercase key
/// with `shift`, and any other shifted symbol by the symbol itself.
fn text_keystroke(text: &str, modifiers: Option<Modifiers>) -> Keystroke {
    let mut modifiers = modifiers.unwrap_or_default();
    if text == " " {
        return named_keystroke("space", modifiers);
    }
    let lowercase = text.to_lowercase();
    if lowercase != text {
        modifiers.shift = true;
    }
    Keystroke {
        modifiers,
        key: lowercase,
        key_char: Some(text.to_owned()),
    }
}

/// `code ; modifiers[:event]` as used by `CSI ~` and the legacy letter finals.
fn split_key_params(params: &str) -> Option<(u32, Modifiers, KeyEventKind)> {
    let mut fields = params.split(';');
    let code = fields.next().unwrap_or("1");
    let code = if code.is_empty() {
        1
    } else {
        code.parse().ok()?
    };
    let (modifiers, kind) = parse_modifier_field(fields.next().unwrap_or(""));
    Some((code, modifiers, kind))
}

fn parse_modifier_field(field: &str) -> (Modifiers, KeyEventKind) {
    let mut parts = field.split(':');
    let mask = parts
        .next()
        .and_then(|part| part.parse::<u32>().ok())
        .unwrap_or(1)
        .saturating_sub(1);
    let kind = match parts.next() {
        Some("2") => KeyEventKind::Repeat,
        Some("3") => KeyEventKind::Release,
        _ => KeyEventKind::Press,
    };
    (
        Modifiers {
            shift: mask & 1 != 0,
            alt: mask & 2 != 0,
            control: mask & 4 != 0,
            platform: mask & 8 != 0,
            function: false,
        },
        kind,
    )
}

/// `CSI code[:shifted[:base]] ; modifiers[:event] ; text u`
/// https://sw.kovidgoyal.net/kitty/keyboard-protocol/#an-overview
fn parse_kitty_key(params: &str) -> Option<TerminalEvent> {
    let mut fields = params.split(';');
    let mut codes = fields.next()?.split(':');
    let code: u32 = codes.next()?.parse().ok()?;
    let shifted: Option<u32> = codes.next().and_then(|code| code.parse().ok());
    let (mut modifiers, kind) = parse_modifier_field(fields.next().unwrap_or(""));
    let text: Option<String> = fields.next().map(|field| {
        field
            .split(':')
            .filter_map(|codepoint| codepoint.parse().ok().and_then(char::from_u32))
            .collect()
    });

    if let Some(modifier_state) = modifier_key(code, modifiers, kind) {
        return Some(TerminalEvent::Modifiers(modifier_state));
    }

    let name = match code {
        13 | 57414 => "enter",
        27 => "escape",
        9 => "tab",
        127 => "backspace",
        32 => "space",
        57417 => "left",
        57418 => "right",
        57419 => "up",
        57420 => "down",
        57421 => "pageup",
        57422 => "pagedown",
        57423 => "home",
        57424 => "end",
        57425 => "insert",
        57426 => "delete",
        57376..=57398 => {
            return Some(TerminalEvent::Key {
                keystroke: named_keystroke(&format!("f{}", code - 57376 + 13), modifiers),
                kind,
            });
        }
        // Remaining private-use codes are lock, media and system keys GPUI has no name for.
        57344..=63743 if keypad_character(code).is_none() => return None,
        _ => {
            let character = keypad_character(code).or_else(|| char::from_u32(code))?;
            // Terminals without "report associated text" send no text field;
            // the key code, shifted if needed, is then the typed character.
            let typed = text.or_else(|| {
                (!modifiers.alt).then(|| match shifted.filter(|_| modifiers.shift) {
                    Some(shifted) => char::from_u32(shifted)
                        .map(String::from)
                        .unwrap_or_default(),
                    None => character.to_string(),
                })
            });
            let key_char = typed.filter(|text| {
                !text.is_empty()
                    && !modifiers.control
                    && !modifiers.platform
                    && kind != KeyEventKind::Release
            });
            let key = if modifiers.shift && !character.is_ascii_lowercase() {
                match shifted.and_then(char::from_u32) {
                    Some(shifted) => {
                        modifiers.shift = false;
                        shifted.to_string()
                    }
                    None => character.to_string(),
                }
            } else {
                character.to_string()
            };
            return Some(TerminalEvent::Key {
                keystroke: Keystroke {
                    modifiers,
                    key,
                    key_char,
                },
                kind,
            });
        }
    };
    let mut keystroke = named_keystroke(name, modifiers);
    // Associated text wins over the default, e.g. option-space types U+00A0.
    if keystroke.key_char.is_some()
        && let Some(text) = text.filter(|text| !text.is_empty())
    {
        keystroke.key_char = Some(text);
    }
    if kind == KeyEventKind::Release {
        keystroke.key_char = None;
    }
    Some(TerminalEvent::Key { keystroke, kind })
}

/// Keypad keys have their own codes so that bindings can tell them apart.
/// GPUI does not, so they type and bind like the main keys.
fn keypad_character(code: u32) -> Option<char> {
    Some(match code {
        57399..=57408 => char::from_digit(code - 57399, 10)?,
        57409 => '.',
        57410 => '/',
        57411 => '*',
        57412 => '-',
        57413 => '+',
        57415 => '=',
        57416 => ',',
        _ => return None,
    })
}

/// Kitty reports modifier keys themselves (57441–57452). The modifier field
/// already includes the key on press and excludes it on release.
fn modifier_key(code: u32, modifiers: Modifiers, _kind: KeyEventKind) -> Option<Modifiers> {
    if (57441..=57452).contains(&code) {
        Some(modifiers)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(bytes: &[u8]) -> Vec<TerminalEvent> {
        let mut parser = InputParser::default();
        let mut events = Vec::new();
        parser.feed(bytes, &mut events);
        events
    }

    fn keystrokes(bytes: &[u8]) -> Vec<String> {
        parse(bytes)
            .into_iter()
            .filter_map(|event| match event {
                TerminalEvent::Key {
                    keystroke,
                    kind: KeyEventKind::Press,
                } => Some(format!("{} {:?}", keystroke.unparse(), keystroke.key_char)),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn parses_kitty_keys() {
        assert_eq!(
            keystrokes(
                b"\x1b[97;;97u\x1b[97;2:1;65u\x1b[49:33;2;33u\x1b[99;5u\x1b[13u\x1b[57399;1;48u\x1b[32;3;160u"
            ),
            vec![
                "a Some(\"a\")",
                "shift-a Some(\"A\")",
                "! Some(\"!\")",
                "ctrl-c None",
                "enter Some(\"\\n\")",
                "0 Some(\"0\")",
                "alt-space Some(\"\\u{a0}\")",
            ]
        );
        assert_eq!(
            keystrokes(b"\x1b[1;5A\x1b[3;2~\x1b[27u"),
            vec!["ctrl-up None", "shift-delete None", "escape None",]
        );
    }

    #[test]
    fn parses_release_and_modifier_keys() {
        assert_eq!(
            parse(b"\x1b[97;1:3u\x1b[57441;2u"),
            vec![
                TerminalEvent::Key {
                    keystroke: Keystroke {
                        modifiers: Modifiers::default(),
                        key: "a".into(),
                        key_char: None,
                    },
                    kind: KeyEventKind::Release,
                },
                TerminalEvent::Modifiers(Modifiers {
                    shift: true,
                    ..Modifiers::default()
                }),
            ]
        );
    }

    #[test]
    fn parses_sgr_mouse_and_tracks_drag() {
        let events =
            parse(b"\x1b[<0;11;21M\x1b[<32;12;22M\x1b[<35;13;23M\x1b[<0;12;22m\x1b[<65;1;1M");
        let kinds: Vec<_> = events
            .into_iter()
            .map(|event| match event {
                TerminalEvent::Mouse { kind, x, y, .. } => (kind, x, y),
                other => panic!("unexpected {other:?}"),
            })
            .collect();
        assert_eq!(
            kinds,
            vec![
                (MouseEventKind::Press(MouseButton::Left), 10., 20.),
                (MouseEventKind::Move(Some(MouseButton::Left)), 11., 21.),
                (MouseEventKind::Move(Some(MouseButton::Left)), 12., 22.),
                (MouseEventKind::Release(MouseButton::Left), 11., 21.),
                (MouseEventKind::Wheel { dx: 0, dy: 1 }, 0., 0.),
            ]
        );
    }

    #[test]
    fn handles_split_sequences_paste_and_replies() {
        let mut parser = InputParser::default();
        let mut events = Vec::new();
        parser.feed(b"\x1b[200~hello\r\nwor", &mut events);
        parser.feed(b"ld\x1b[20", &mut events);
        parser.feed(
            b"1~\x1b_Gi=1;OK\x1b\\\x1b_Gi=7,p=1;ENOENT:No such file\x1b\\\x1b[?62;22c\
              \x1b[?1016;2$y\x1b[48;30;100;630;1000t\x1b[I\x1b[9",
            &mut events,
        );
        parser.feed(b"7u", &mut events);
        assert_eq!(
            events,
            vec![
                TerminalEvent::Paste("hello\nworld".into()),
                TerminalEvent::GraphicsReply { id: 1, ok: true },
                TerminalEvent::GraphicsReply { id: 7, ok: false },
                TerminalEvent::DeviceAttributes,
                TerminalEvent::PixelMouseSupported(true),
                TerminalEvent::Resize(TerminalSize {
                    columns: 100,
                    rows: 30,
                    width: 1000,
                    height: 630,
                }),
                TerminalEvent::Focus(true),
                key_event(text_keystroke("a", None)),
            ]
        );
    }

    #[test]
    fn parses_legacy_input() {
        let mut parser = InputParser::default();
        let mut events = Vec::new();
        parser.feed(b"x\x03\x7f\x1bb\xc3\xa9\x1b", &mut events);
        assert!(parser.has_pending_escape());
        parser.flush_pending_escape(&mut events);
        let names: Vec<_> = events
            .into_iter()
            .map(|event| match event {
                TerminalEvent::Key { keystroke, .. } => keystroke.unparse(),
                other => panic!("unexpected {other:?}"),
            })
            .collect();
        assert_eq!(
            names,
            vec!["x", "ctrl-c", "backspace", "alt-b", "é", "escape"]
        );
    }
}
