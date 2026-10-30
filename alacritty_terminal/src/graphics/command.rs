use std::fmt;

use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;

/// The action to perform.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Action {
    /// Query terminal for support (a=q). Load + validate, don't store.
    Query,
    /// Transmit image data (a=t).
    #[default]
    Transmit,
    /// Transmit and display (a=T).
    TransmitAndDisplay,
    /// Display a previously transmitted image (a=p).
    Display,
    /// Delete images/placements (a=d).
    Delete,
    /// Transmit animation frame (a=f).
    TransmitAnimationFrame,
    /// Control animation (a=a).
    ControlAnimation,
    /// Compose animation frame (a=c).
    ComposeAnimation,
}

/// Image data format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Format {
    /// 24-bit RGB (f=24).
    Rgb,
    /// 32-bit RGBA (f=32).
    #[default]
    Rgba,
    /// PNG (f=100).
    Png,
}

/// Transmission medium.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Medium {
    /// Direct data in payload (t=d).
    #[default]
    Direct,
    /// File path in payload (t=f).
    File,
    /// Temporary file (t=t).
    TempFile,
    /// Shared memory object name (t=s).
    SharedMemory,
}

/// Compression type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Compression {
    #[default]
    None,
    /// zlib deflate (o=z).
    Zlib,
}

/// How quiet the response should be.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Quiet {
    /// Send all responses (q=0).
    #[default]
    No,
    /// Suppress OK responses, send errors (q=1).
    SuppressOk,
    /// Suppress all responses (q=2).
    SuppressAll,
}

/// Cursor movement policy after display.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CursorMovement {
    /// Move cursor (C=0, default).
    #[default]
    Move,
    /// Don't move cursor (C=1).
    DoNotMove,
}

/// Placement for virtual/unicode mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum VirtualPlacement {
    /// Not virtual (U=0, default).
    #[default]
    No,
    /// Virtual placement using unicode placeholders (U=1).
    Yes,
}

/// Transmission parameters (for a=t, a=T, a=f).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Transmission {
    pub format: Format,
    pub medium: Medium,
    pub width: u32,
    pub height: u32,
    pub size: u32,
    pub offset: u32,
    pub image_id: u32,
    pub image_number: u32,
    pub placement_id: u32,
    pub compression: Compression,
    pub more_chunks: bool,
}

/// Display parameters (for a=p, a=T).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DisplayCmd {
    pub columns: u32,
    pub rows: u32,
    pub x_offset: u32,
    pub y_offset: u32,
    pub width: u32,
    pub height: u32,
    pub z_index: i32,
    pub placement_id: u32,
    pub cursor_movement: CursorMovement,
    pub virtual_placement: VirtualPlacement,
    pub parent_id: u32,
    pub parent_placement_id: u32,
    pub horizontal_offset: i32,
    pub vertical_offset: i32,
}

/// Delete action variants.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeleteAction {
    /// Delete all images (d=a or d=A).
    All { delete_data: bool },
    /// Delete by image id (d=i or d=I).
    ById { image_id: u32, delete_data: bool },
    /// Delete by image number (d=n or d=N).
    ByNumber { image_number: u32, delete_data: bool },
    /// Delete at cursor position (d=c or d=C).
    AtCursor { delete_data: bool },
    /// Delete animation frames (d=f).
    AnimationFrames { image_id: u32 },
    /// Delete by cell position (d=p or d=P).
    AtPosition { x: u32, y: u32, z: i32, delete_data: bool },
    /// Delete by z-index (d=z or d=Z).
    ByZIndex { z: i32, delete_data: bool },
    /// Delete in column (d=x or d=X).
    InColumn { x: u32, delete_data: bool },
    /// Delete in row (d=y or d=Y).
    InRow { y: u32, delete_data: bool },
    /// Delete by range (d=r or d=R).
    ByRange { image_id: u32, placement_id: u32, delete_data: bool },
}

impl Default for DeleteAction {
    fn default() -> Self {
        DeleteAction::All { delete_data: true }
    }
}

/// Animation control parameters (for a=a).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AnimationControl {
    pub image_id: u32,
    pub image_number: u32,
    pub placement_id: u32,
    pub state: u32,
    pub frame_number: u32,
    pub gap: u32,
    pub loops: u32,
}

/// A parsed Kitty Graphics command.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Command {
    pub action: Action,
    pub quiet: Quiet,
    pub transmission: Transmission,
    pub display: DisplayCmd,
    pub delete: DeleteAction,
    pub animation: AnimationControl,
    pub data: Vec<u8>,
}

/// A response to send back to the application.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Response {
    pub image_id: u32,
    pub image_number: u32,
    pub placement_id: u32,
    pub message: String,
}

impl Response {
    pub fn ok(image_id: u32) -> Self {
        Self { image_id, image_number: 0, placement_id: 0, message: "OK".into() }
    }

    pub fn error(image_id: u32, msg: impl Into<String>) -> Self {
        Self { image_id, image_number: 0, placement_id: 0, message: msg.into() }
    }

    pub fn is_ok(&self) -> bool {
        self.message == "OK"
    }

    /// Encode as APC response: `\x1b_Gi=<id>;message\x1b\\`
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::from(b"\x1b_G");
        if self.image_id > 0 {
            out.extend_from_slice(format!("i={}", self.image_id).as_bytes());
        }
        if self.image_number > 0 {
            if self.image_id > 0 {
                out.push(b',');
            }
            out.extend_from_slice(format!("I={}", self.image_number).as_bytes());
        }
        if self.placement_id > 0 {
            if self.image_id > 0 || self.image_number > 0 {
                out.push(b',');
            }
            out.extend_from_slice(format!("p={}", self.placement_id).as_bytes());
        }
        out.push(b';');
        out.extend_from_slice(self.message.as_bytes());
        out.extend_from_slice(b"\x1b\\");
        out
    }
}

/// Parser errors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseError {
    NotKitty,
    InvalidFormat,
    InvalidData,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParseError::NotKitty => write!(f, "not a kitty graphics command"),
            ParseError::InvalidFormat => write!(f, "invalid command format"),
            ParseError::InvalidData => write!(f, "invalid payload data"),
        }
    }
}

impl std::error::Error for ParseError {}

/// Key-value table for control data (52 single-letter keys).
#[derive(Clone)]
struct Kv {
    values: [u32; 52],
    present: u64,
}

impl Default for Kv {
    fn default() -> Self {
        Self { values: [0; 52], present: 0 }
    }
}

impl Kv {
    fn index(key: u8) -> Option<usize> {
        match key {
            b'a'..=b'z' => Some((key - b'a') as usize),
            b'A'..=b'Z' => Some(26 + (key - b'A') as usize),
            _ => None,
        }
    }

    fn get(&self, key: u8) -> Option<u32> {
        let idx = Self::index(key)?;
        if self.present & (1 << idx) == 0 {
            return None;
        }
        Some(self.values[idx])
    }

    fn put(&mut self, key: u8, value: u32) {
        if let Some(idx) = Self::index(key) {
            self.values[idx] = value;
            self.present |= 1 << idx;
        }
    }
}

/// Parser states.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ParserState {
    WaitingForG,
    ControlKey,
    ControlKeyIgnore,
    ControlValue,
    ControlValueIgnore,
    Payload,
    Done,
}

const DEFAULT_MAX_PAYLOAD_BYTES: usize = 64 * 1024 * 1024;

/// Streaming Kitty graphics command parser.
pub struct Parser {
    state: ParserState,
    current_key: u8,
    value_buf: [u8; 11],
    value_len: u8,
    current_value: u32,
    payload_buf: Vec<u8>,
    kv: Kv,
    finished: bool,
    parse_error: Option<ParseError>,
    pub max_payload_bytes: usize,
}

impl Default for Parser {
    fn default() -> Self {
        Self::new()
    }
}

impl Parser {
    pub fn new() -> Self {
        Self {
            state: ParserState::WaitingForG,
            current_key: 0,
            value_buf: [0; 11],
            value_len: 0,
            current_value: 0,
            payload_buf: Vec::new(),
            kv: Kv::default(),
            finished: false,
            parse_error: None,
            max_payload_bytes: DEFAULT_MAX_PAYLOAD_BYTES,
        }
    }

    /// Feed a single byte (called from `apc_put`).
    pub fn feed(&mut self, byte: u8) {
        if self.finished || self.parse_error.is_some() {
            return;
        }

        match self.state {
            ParserState::WaitingForG => {
                if byte == b'G' {
                    self.state = ParserState::ControlKey;
                } else {
                    self.state = ParserState::Done;
                    self.finished = true;
                }
            },

            ParserState::ControlKey => match byte {
                b'=' => {
                    if self.value_len == 1 {
                        self.current_key = self.value_buf[0];
                        self.value_len = 0;
                        self.current_value = 0;
                        self.state = ParserState::ControlValue;
                    } else {
                        self.state = ParserState::ControlValueIgnore;
                        self.value_len = 0;
                    }
                },
                b';' => self.state = ParserState::Payload,
                _ => self.accumulate_key(byte),
            },

            ParserState::ControlKeyIgnore => {
                if byte == b'=' {
                    self.state = ParserState::ControlValueIgnore;
                }
            },

            ParserState::ControlValue => match byte {
                b',' => self.finish_value(ParserState::ControlKey),
                b';' => self.finish_value(ParserState::Payload),
                _ => self.accumulate_value(byte, ParserState::ControlValueIgnore),
            },

            ParserState::ControlValueIgnore => match byte {
                b',' => {
                    self.state = ParserState::ControlKey;
                    self.value_len = 0;
                },
                b';' => self.state = ParserState::Payload,
                _ => {},
            },

            ParserState::Payload => {
                if self.payload_buf.len() >= self.max_payload_bytes {
                    self.parse_error = Some(ParseError::InvalidFormat);
                    self.finished = true;
                    self.state = ParserState::Done;
                } else {
                    self.payload_buf.push(byte);
                }
            },

            ParserState::Done => {},
        }
    }

    /// Signal end of APC (called from `apc_end`).
    pub fn finish(&mut self) -> Result<Command, ParseError> {
        if let Some(err) = self.parse_error.take() {
            self.finished = true;
            self.state = ParserState::Done;
            return Err(err);
        }

        if self.state == ParserState::WaitingForG && !self.finished {
            return Err(ParseError::InvalidFormat);
        }
        if self.finished && self.state == ParserState::Done {
            return Err(ParseError::NotKitty);
        }

        match self.state {
            ParserState::ControlKey | ParserState::ControlKeyIgnore => {
                return Err(ParseError::InvalidFormat);
            },
            ParserState::ControlValue => self.finish_value(ParserState::Payload),
            ParserState::ControlValueIgnore => {},
            ParserState::WaitingForG | ParserState::Payload | ParserState::Done => {},
        }

        self.finished = true;
        self.state = ParserState::Done;
        build_command(&self.kv, &self.payload_buf)
    }

    /// Reset for reuse.
    pub fn reset(&mut self) {
        *self = Self::new();
    }

    fn accumulate_key(&mut self, byte: u8) {
        if self.value_len as usize >= self.value_buf.len() {
            self.state = ParserState::ControlKeyIgnore;
            self.value_len = 0;
            return;
        }
        self.value_buf[self.value_len as usize] = byte;
        self.value_len += 1;
    }

    fn accumulate_value(&mut self, byte: u8, overflow: ParserState) {
        let signed_key = matches!(self.current_key, b'z' | b'H' | b'V');
        if !signed_key && byte.is_ascii_digit() {
            self.current_value = self
                .current_value
                .saturating_mul(10)
                .saturating_add(u32::from(byte - b'0'));
        }

        if self.value_len as usize >= self.value_buf.len() {
            if byte.is_ascii_digit() {
                return;
            }
            self.state = overflow;
            self.value_len = 0;
            return;
        }
        self.value_buf[self.value_len as usize] = byte;
        self.value_len += 1;
    }

    fn finish_value(&mut self, next: ParserState) {
        self.state = next;
        if self.value_len == 0 {
            return;
        }

        let slice = &self.value_buf[..self.value_len as usize];
        if slice.len() == 1 {
            let c = slice[0];
            if !c.is_ascii_digit() {
                self.kv.put(self.current_key, u32::from(c));
                self.value_len = 0;
                return;
            }
        }

        let parsed = match self.current_key {
            b'z' | b'H' | b'V' => std::str::from_utf8(slice)
                .ok()
                .and_then(|s| s.parse::<i32>().ok().map(|v| v as u32)),
            _ if slice.iter().all(u8::is_ascii_digit) => Some(self.current_value),
            _ => parse_u32_saturating(slice),
        };

        if let Some(v) = parsed {
            self.kv.put(self.current_key, v);
        }

        self.value_len = 0;
        self.current_value = 0;
    }
}

fn parse_u32_saturating(slice: &[u8]) -> Option<u32> {
    if slice.is_empty() || !slice.iter().all(|b| b.is_ascii_digit()) {
        return None;
    }

    let mut val = 0u32;
    for &byte in slice {
        val = val.saturating_mul(10).saturating_add(u32::from(byte - b'0'));
    }
    Some(val)
}

fn build_command(kv: &Kv, payload: &[u8]) -> Result<Command, ParseError> {
    let action = parse_action(kv)?;
    let quiet = parse_quiet(kv);
    let transmission = parse_transmission(kv);
    let display = parse_display(kv);
    let delete = parse_delete(kv);
    let animation = parse_animation(kv);

    let data = if payload.is_empty() {
        Vec::new()
    } else {
        BASE64.decode(payload).map_err(|_| ParseError::InvalidData)?
    };

    Ok(Command { action, quiet, transmission, display, delete, animation, data })
}

fn parse_action(kv: &Kv) -> Result<Action, ParseError> {
    let action_byte = match kv.get(b'a') {
        Some(v) if v <= u32::from(u8::MAX) => v as u8,
        Some(_) => return Err(ParseError::InvalidFormat),
        None => return Ok(Action::Transmit),
    };

    match action_byte {
        b'q' => Ok(Action::Query),
        b't' => Ok(Action::Transmit),
        b'T' => Ok(Action::TransmitAndDisplay),
        b'p' => Ok(Action::Display),
        b'd' => Ok(Action::Delete),
        b'f' => Ok(Action::TransmitAnimationFrame),
        b'a' => Ok(Action::ControlAnimation),
        b'c' => Ok(Action::ComposeAnimation),
        _ => Err(ParseError::InvalidFormat),
    }
}

fn parse_quiet(kv: &Kv) -> Quiet {
    match kv.get(b'q') {
        Some(0) | None => Quiet::No,
        Some(1) => Quiet::SuppressOk,
        Some(2) => Quiet::SuppressAll,
        Some(_) => Quiet::SuppressAll,
    }
}

fn parse_transmission(kv: &Kv) -> Transmission {
    let mut t = Transmission::default();

    if let Some(v) = kv.get(b'f') {
        t.format = match v {
            24 => Format::Rgb,
            32 => Format::Rgba,
            100 => Format::Png,
            _ => Format::Rgba,
        };
    }

    if let Some(v) = kv.get(b't') {
        if v <= u32::from(u8::MAX) {
            t.medium = match v as u8 {
                b'd' => Medium::Direct,
                b'f' => Medium::File,
                b't' => Medium::TempFile,
                b's' => Medium::SharedMemory,
                _ => Medium::Direct,
            };
        }
    }

    if let Some(v) = kv.get(b's') {
        t.width = v;
    }
    if let Some(v) = kv.get(b'v') {
        t.height = v;
    }
    if let Some(v) = kv.get(b'S') {
        t.size = v;
    }
    if let Some(v) = kv.get(b'O') {
        t.offset = v;
    }
    if let Some(v) = kv.get(b'i') {
        t.image_id = v;
    }
    if let Some(v) = kv.get(b'I') {
        t.image_number = v;
    }
    if let Some(v) = kv.get(b'p') {
        t.placement_id = v;
    }
    if let Some(v) = kv.get(b'o') {
        if v == u32::from(b'z') {
            t.compression = Compression::Zlib;
        }
    }
    if t.medium == Medium::Direct {
        if let Some(v) = kv.get(b'm') {
            t.more_chunks = v > 0;
        }
    }

    t
}

fn parse_display(kv: &Kv) -> DisplayCmd {
    let mut d = DisplayCmd::default();

    if let Some(v) = kv.get(b'c') {
        d.columns = v;
    }
    if let Some(v) = kv.get(b'r') {
        d.rows = v;
    }
    if let Some(v) = kv.get(b'x') {
        d.x_offset = v;
    }
    if let Some(v) = kv.get(b'y') {
        d.y_offset = v;
    }
    if let Some(v) = kv.get(b'w') {
        d.width = v;
    }
    if let Some(v) = kv.get(b'h') {
        d.height = v;
    }
    if let Some(v) = kv.get(b'z') {
        d.z_index = v as i32;
    }
    if let Some(v) = kv.get(b'p') {
        d.placement_id = v;
    }
    if let Some(v) = kv.get(b'C') {
        d.cursor_movement = if v == 1 { CursorMovement::DoNotMove } else { CursorMovement::Move };
    }
    if let Some(v) = kv.get(b'U') {
        d.virtual_placement = if v != 0 { VirtualPlacement::Yes } else { VirtualPlacement::No };
    }
    if let Some(v) = kv.get(b'P') {
        d.parent_id = v;
    }
    if let Some(v) = kv.get(b'Q') {
        d.parent_placement_id = v;
    }
    if let Some(v) = kv.get(b'H') {
        d.horizontal_offset = v as i32;
    }
    if let Some(v) = kv.get(b'V') {
        d.vertical_offset = v as i32;
    }

    d
}

fn parse_delete(kv: &Kv) -> DeleteAction {
    let what = match kv.get(b'd') {
        Some(v) if v <= u32::from(u8::MAX) => v as u8,
        None => return DeleteAction::All { delete_data: true },
        Some(_) => return DeleteAction::All { delete_data: true },
    };

    match what {
        b'a' => DeleteAction::All { delete_data: false },
        b'A' => DeleteAction::All { delete_data: true },
        b'i' => DeleteAction::ById { image_id: kv.get(b'i').unwrap_or(0), delete_data: false },
        b'I' => DeleteAction::ById { image_id: kv.get(b'i').unwrap_or(0), delete_data: true },
        b'n' => {
            DeleteAction::ByNumber { image_number: kv.get(b'I').unwrap_or(0), delete_data: false }
        },
        b'N' => {
            DeleteAction::ByNumber { image_number: kv.get(b'I').unwrap_or(0), delete_data: true }
        },
        b'c' => DeleteAction::AtCursor { delete_data: false },
        b'C' => DeleteAction::AtCursor { delete_data: true },
        b'f' => DeleteAction::AnimationFrames { image_id: kv.get(b'i').unwrap_or(0) },
        b'p' => DeleteAction::AtPosition {
            x: kv.get(b'x').unwrap_or(0),
            y: kv.get(b'y').unwrap_or(0),
            z: kv.get(b'z').map(|v| v as i32).unwrap_or(0),
            delete_data: false,
        },
        b'P' => DeleteAction::AtPosition {
            x: kv.get(b'x').unwrap_or(0),
            y: kv.get(b'y').unwrap_or(0),
            z: kv.get(b'z').map(|v| v as i32).unwrap_or(0),
            delete_data: true,
        },
        b'z' => DeleteAction::ByZIndex { z: kv.get(b'z').map(|v| v as i32).unwrap_or(0), delete_data: false },
        b'Z' => DeleteAction::ByZIndex { z: kv.get(b'z').map(|v| v as i32).unwrap_or(0), delete_data: true },
        b'x' => DeleteAction::InColumn { x: kv.get(b'x').unwrap_or(0), delete_data: false },
        b'X' => DeleteAction::InColumn { x: kv.get(b'x').unwrap_or(0), delete_data: true },
        b'y' => DeleteAction::InRow { y: kv.get(b'y').unwrap_or(0), delete_data: false },
        b'Y' => DeleteAction::InRow { y: kv.get(b'y').unwrap_or(0), delete_data: true },
        b'r' => DeleteAction::ByRange {
            image_id: kv.get(b'i').unwrap_or(0),
            placement_id: kv.get(b'p').unwrap_or(0),
            delete_data: false,
        },
        b'R' => DeleteAction::ByRange {
            image_id: kv.get(b'i').unwrap_or(0),
            placement_id: kv.get(b'p').unwrap_or(0),
            delete_data: true,
        },
        _ => DeleteAction::All { delete_data: true },
    }
}

fn parse_animation(kv: &Kv) -> AnimationControl {
    let mut a = AnimationControl::default();
    if let Some(v) = kv.get(b'i') {
        a.image_id = v;
    }
    if let Some(v) = kv.get(b'I') {
        a.image_number = v;
    }
    if let Some(v) = kv.get(b'p') {
        a.placement_id = v;
    }
    if let Some(v) = kv.get(b's') {
        a.state = v;
    }
    if let Some(v) = kv.get(b'r') {
        a.frame_number = v;
    }
    if let Some(v) = kv.get(b'z') {
        a.gap = v;
    }
    if let Some(v) = kv.get(b'v') {
        a.loops = v;
    }
    a
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(s: &str) -> Command {
        let mut parser = Parser::new();
        for byte in s.bytes() {
            parser.feed(byte);
        }
        parser.finish().expect("parse failed")
    }

    fn parse_after_g(s: &str) -> Command {
        let mut parser = Parser::new();
        parser.feed(b'G');
        for byte in s.bytes() {
            parser.feed(byte);
        }
        parser.finish().expect("parse failed")
    }

    #[test]
    fn test_parse_simple_transmit() {
        let cmd = parse_after_g("a=t,f=32,s=10,v=20,i=1;AAAA");
        assert_eq!(cmd.action, Action::Transmit);
        assert_eq!(cmd.transmission.format, Format::Rgba);
        assert_eq!(cmd.transmission.width, 10);
        assert_eq!(cmd.transmission.height, 20);
        assert_eq!(cmd.transmission.image_id, 1);
        assert_eq!(cmd.data, BASE64.decode(b"AAAA").unwrap());
    }

    #[test]
    fn test_parse_transmit_and_display() {
        let payload = BASE64.encode(b"base64data");
        let input = format!("a=T,f=100,i=5,c=80,r=24;{payload}");
        let cmd = parse_after_g(&input);
        assert_eq!(cmd.action, Action::TransmitAndDisplay);
        assert_eq!(cmd.transmission.format, Format::Png);
        assert_eq!(cmd.transmission.image_id, 5);
        assert_eq!(cmd.display.columns, 80);
        assert_eq!(cmd.display.rows, 24);
        assert_eq!(cmd.data, b"base64data");
    }

    #[test]
    fn test_parse_display_only() {
        let cmd = parse_after_g("a=p,i=1,p=2,z=-1");
        assert_eq!(cmd.action, Action::Display);
        assert_eq!(cmd.transmission.image_id, 1);
        assert_eq!(cmd.display.placement_id, 2);
        assert_eq!(cmd.display.z_index, -1);
    }

    #[test]
    fn test_parse_delete_all() {
        let cmd = parse_after_g("a=d,d=a");
        assert_eq!(cmd.action, Action::Delete);
        assert_eq!(cmd.delete, DeleteAction::All { delete_data: false });
    }

    #[test]
    fn test_parse_delete_by_id() {
        let cmd = parse_after_g("a=d,d=i,i=42");
        assert_eq!(cmd.delete, DeleteAction::ById { image_id: 42, delete_data: false });
    }

    #[test]
    fn test_parse_delete_by_id_data_only() {
        let cmd = parse_after_g("a=d,d=I,i=42");
        assert_eq!(cmd.delete, DeleteAction::ById { image_id: 42, delete_data: true });
    }

    #[test]
    fn test_parse_query() {
        let cmd = parse_after_g("a=q,t=d,f=24,s=1,v=1,i=99;AAAA");
        assert_eq!(cmd.action, Action::Query);
        assert_eq!(cmd.transmission.format, Format::Rgb);
        assert_eq!(cmd.transmission.medium, Medium::Direct);
        assert_eq!(cmd.transmission.image_id, 99);
        assert_eq!(cmd.data, BASE64.decode(b"AAAA").unwrap());
    }

    #[test]
    fn test_parse_default_action() {
        let cmd = parse_after_g("f=32,s=5,v=5,i=1");
        assert_eq!(cmd.action, Action::Transmit);
    }

    #[test]
    fn test_parse_unknown_keys_ignored() {
        let cmd = parse_after_g("a=t,f=32,UNKNOWN_KEY=999,s=5,v=5,i=1");
        assert_eq!(cmd.transmission.width, 5);
        assert_eq!(cmd.transmission.height, 5);
        assert_eq!(cmd.transmission.image_id, 1);
    }

    #[test]
    fn test_parse_empty_payload() {
        let cmd = parse_after_g("a=t,f=32,s=1,v=1,i=1");
        assert!(cmd.data.is_empty());
    }

    #[test]
    fn test_parse_signed_z_index() {
        let cmd = parse_after_g("a=p,i=1,z=-100");
        assert_eq!(cmd.display.z_index, -100);
    }

    #[test]
    fn test_parse_quiet_modes() {
        assert_eq!(parse_after_g("a=t,q=0").quiet, Quiet::No);
        assert_eq!(parse_after_g("a=t,q=1").quiet, Quiet::SuppressOk);
        assert_eq!(parse_after_g("a=t,q=2").quiet, Quiet::SuppressAll);
    }

    #[test]
    fn test_parse_chunked_transmission() {
        let cmd = parse_after_g("a=t,m=1,i=1");
        assert!(cmd.transmission.more_chunks);
    }

    #[test]
    fn test_parse_all_formats() {
        assert_eq!(parse_after_g("a=t,f=24").transmission.format, Format::Rgb);
        assert_eq!(parse_after_g("a=t,f=32").transmission.format, Format::Rgba);
        assert_eq!(parse_after_g("a=t,f=100").transmission.format, Format::Png);
    }

    #[test]
    fn test_parse_all_mediums() {
        assert_eq!(parse_after_g("a=t,t=d").transmission.medium, Medium::Direct);
        assert_eq!(parse_after_g("a=t,t=f").transmission.medium, Medium::File);
        assert_eq!(parse_after_g("a=t,t=t").transmission.medium, Medium::TempFile);
        assert_eq!(parse_after_g("a=t,t=s").transmission.medium, Medium::SharedMemory);
    }

    #[test]
    fn test_parse_animation_frame() {
        let cmd = parse_after_g("a=f,i=1,f=32,s=10,v=10");
        assert_eq!(cmd.action, Action::TransmitAnimationFrame);
        assert_eq!(cmd.transmission.image_id, 1);
    }

    #[test]
    fn test_parse_animation_control() {
        let cmd = parse_after_g("a=a,i=1,s=2,r=1,v=0");
        assert_eq!(cmd.action, Action::ControlAnimation);
        assert_eq!(cmd.animation.image_id, 1);
        assert_eq!(cmd.animation.state, 2);
        assert_eq!(cmd.animation.frame_number, 1);
        assert_eq!(cmd.animation.loops, 0);
    }

    #[test]
    fn test_parse_compose_animation() {
        let cmd = parse_after_g("a=c,i=1,c=2,r=3");
        assert_eq!(cmd.action, Action::ComposeAnimation);
        assert_eq!(cmd.transmission.image_id, 1);
    }

    #[test]
    fn test_response_ok_encode() {
        assert_eq!(Response::ok(42).encode(), b"\x1b_Gi=42;OK\x1b\\");
    }

    #[test]
    fn test_response_error_encode() {
        let resp = Response::error(7, "EINVAL: invalid image");
        assert_eq!(resp.encode(), b"\x1b_Gi=7;EINVAL: invalid image\x1b\\");
        assert!(!resp.is_ok());
    }

    #[test]
    fn test_parse_streaming() {
        let s = "a=t,f=32,s=10,v=20,i=1;AAAA";
        let mut parser = Parser::new();
        parser.feed(b'G');
        for byte in s.bytes() {
            parser.feed(byte);
        }
        let cmd = parser.finish().unwrap();
        assert_eq!(cmd.transmission.image_id, 1);
        assert_eq!(cmd.data, BASE64.decode(b"AAAA").unwrap());
    }

    #[test]
    fn test_parse_not_kitty() {
        let mut parser = Parser::new();
        parser.feed(b'X');
        assert_eq!(parser.finish(), Err(ParseError::NotKitty));
    }

    #[test]
    fn test_parse_with_g_prefix() {
        let cmd = parse("Ga=t,f=32,i=1");
        assert_eq!(cmd.action, Action::Transmit);
        assert_eq!(cmd.transmission.image_id, 1);
    }

    #[test]
    fn test_parse_large_payload() {
        let raw = vec![0xABu8; 75_000];
        let encoded = BASE64.encode(&raw);
        let mut control = String::from("a=t,f=32,s=100,v=100,i=1;");
        control.push_str(&encoded);
        let cmd = parse_after_g(&control);
        assert_eq!(cmd.data, raw);
    }

    #[test]
    fn test_parse_virtual_placement() {
        let cmd = parse_after_g("a=p,U=1");
        assert_eq!(cmd.display.virtual_placement, VirtualPlacement::Yes);
    }

    #[test]
    fn test_parse_cursor_movement() {
        let cmd = parse_after_g("a=p,C=1");
        assert_eq!(cmd.display.cursor_movement, CursorMovement::DoNotMove);
    }

    #[test]
    fn test_parse_parent_placement() {
        let cmd = parse_after_g("a=p,P=1,Q=2,H=10,V=20");
        assert_eq!(cmd.display.parent_id, 1);
        assert_eq!(cmd.display.parent_placement_id, 2);
        assert_eq!(cmd.display.horizontal_offset, 10);
        assert_eq!(cmd.display.vertical_offset, 20);
    }

    #[test]
    fn test_parse_delete_variants() {
        assert_eq!(
            parse_after_g("a=d,d=A").delete,
            DeleteAction::All { delete_data: true }
        );
        assert_eq!(
            parse_after_g("a=d,d=n,I=3").delete,
            DeleteAction::ByNumber { image_number: 3, delete_data: false }
        );
        assert_eq!(
            parse_after_g("a=d,d=N,I=3").delete,
            DeleteAction::ByNumber { image_number: 3, delete_data: true }
        );
        assert_eq!(
            parse_after_g("a=d,d=c").delete,
            DeleteAction::AtCursor { delete_data: false }
        );
        assert_eq!(
            parse_after_g("a=d,d=C").delete,
            DeleteAction::AtCursor { delete_data: true }
        );
        assert_eq!(
            parse_after_g("a=d,d=f,i=9").delete,
            DeleteAction::AnimationFrames { image_id: 9 }
        );
        assert_eq!(
            parse_after_g("a=d,d=p,x=1,y=2,z=-3").delete,
            DeleteAction::AtPosition { x: 1, y: 2, z: -3, delete_data: false }
        );
        assert_eq!(
            parse_after_g("a=d,d=P,x=1,y=2").delete,
            DeleteAction::AtPosition { x: 1, y: 2, z: 0, delete_data: true }
        );
        assert_eq!(
            parse_after_g("a=d,d=z,z=5").delete,
            DeleteAction::ByZIndex { z: 5, delete_data: false }
        );
        assert_eq!(
            parse_after_g("a=d,d=Z,z=5").delete,
            DeleteAction::ByZIndex { z: 5, delete_data: true }
        );
        assert_eq!(
            parse_after_g("a=d,d=x,x=4").delete,
            DeleteAction::InColumn { x: 4, delete_data: false }
        );
        assert_eq!(
            parse_after_g("a=d,d=X,x=4").delete,
            DeleteAction::InColumn { x: 4, delete_data: true }
        );
        assert_eq!(
            parse_after_g("a=d,d=y,y=6").delete,
            DeleteAction::InRow { y: 6, delete_data: false }
        );
        assert_eq!(
            parse_after_g("a=d,d=Y,y=6").delete,
            DeleteAction::InRow { y: 6, delete_data: true }
        );
        assert_eq!(
            parse_after_g("a=d,d=r,i=1,p=2").delete,
            DeleteAction::ByRange { image_id: 1, placement_id: 2, delete_data: false }
        );
        assert_eq!(
            parse_after_g("a=d,d=R,i=1,p=2").delete,
            DeleteAction::ByRange { image_id: 1, placement_id: 2, delete_data: true }
        );
    }

    #[test]
    fn test_parse_negative_hv_offsets() {
        let cmd = parse_after_g("a=p,P=1,Q=2,H=-10,V=-20");
        assert_eq!(cmd.display.parent_id, 1);
        assert_eq!(cmd.display.parent_placement_id, 2);
        assert_eq!(cmd.display.horizontal_offset, -10);
        assert_eq!(cmd.display.vertical_offset, -20);
    }

    #[test]
    fn test_parse_saturating_overflow() {
        let cmd = parse_after_g("a=t,i=99999999999999");
        assert_eq!(cmd.transmission.image_id, u32::MAX);
    }

    fn try_parse_after_g(s: &str) -> Result<Command, ParseError> {
        let mut parser = Parser::new();
        parser.feed(b'G');
        for byte in s.bytes() {
            parser.feed(byte);
        }
        parser.finish()
    }

    #[test]
    fn test_parse_invalid_base64() {
        assert_eq!(try_parse_after_g("a=t,f=32,i=1;!!!"), Err(ParseError::InvalidData));
    }

    #[test]
    fn test_parse_payload_only() {
        let cmd = try_parse_after_g(";AAAA").expect("parse failed");
        assert_eq!(cmd.action, Action::Transmit);
        assert_eq!(cmd.data, BASE64.decode(b"AAAA").unwrap());
    }

    #[test]
    fn test_parser_reset_reuse() {
        let s = "a=t,f=32,s=10,v=20,i=1;AAAA";
        let mut parser = Parser::new();
        parser.feed(b'G');
        for byte in s.bytes() {
            parser.feed(byte);
        }
        let cmd1 = parser.finish().unwrap();
        assert_eq!(cmd1.transmission.image_id, 1);

        parser.reset();
        parser.feed(b'G');
        for byte in s.bytes() {
            parser.feed(byte);
        }
        let cmd2 = parser.finish().unwrap();
        assert_eq!(cmd2.transmission.image_id, 1);
        assert_eq!(cmd2.data, BASE64.decode(b"AAAA").unwrap());
    }

    #[test]
    fn test_parse_payload_limit() {
        let mut parser = Parser::new();
        parser.max_payload_bytes = 4;
        parser.feed(b'G');
        parser.feed(b';');
        for byte in b"AAAAAA".iter().copied() {
            parser.feed(byte);
        }
        assert_eq!(parser.finish(), Err(ParseError::InvalidFormat));
    }
}
