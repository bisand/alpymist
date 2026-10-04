//! Messages as bytes, and bytes as messages.
//!
//! The wire format of the D-Bus specification: a header, the header's fields,
//! and a body laid out as the body's signature says, each value on the
//! boundary its kind is aligned to.
//!
//! What is read here comes from a daemon that runs as root, by way of a bus
//! that checks every message it passes on. It is read as if neither were
//! true: every length is held against what is actually there, text is checked
//! to be text, nesting has a bottom, and nothing is allocated on a number's
//! say-so. A message that does not hold together is an error, never a panic.

use crate::value::Value;

/// The largest message read: thirty-two mebibytes. The specification allows
/// four times that; nothing spoken to here sends a thousandth of it.
pub const MAX_MESSAGE: usize = 32 << 20;

/// How deep arrays, structs and variants may nest; the specification's limit.
const MAX_DEPTH: usize = 64;

/// What a message is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// A method called.
    Call,
    /// A call's answer.
    Return,
    /// A call's failure.
    Error,
    /// Something announced to whoever listens.
    Signal,
}

/// A D-Bus message.
#[derive(Debug, Clone, PartialEq)]
pub struct Message {
    /// What it is.
    pub kind: Kind,
    /// Whether the sender wants no answer.
    pub no_reply: bool,
    /// The sender's number for it.
    pub serial: u32,
    /// The number of the call this answers.
    pub reply_serial: Option<u32>,
    /// The object it is about.
    pub path: Option<String>,
    /// The interface the member belongs to.
    pub interface: Option<String>,
    /// The method or signal.
    pub member: Option<String>,
    /// The name of the error, when it is one.
    pub error_name: Option<String>,
    /// Who it is for.
    pub destination: Option<String>,
    /// Who it is from, as the bus says.
    pub sender: Option<String>,
    /// What it carries.
    pub body: Vec<Value>,
}

impl Message {
    /// A message of `kind` with nothing in it yet.
    #[must_use]
    pub const fn new(kind: Kind) -> Self {
        Self {
            kind,
            no_reply: false,
            serial: 0,
            reply_serial: None,
            path: None,
            interface: None,
            member: None,
            error_name: None,
            destination: None,
            sender: None,
            body: Vec::new(),
        }
    }

    /// The first value of the body, where the body is text: an error's
    /// message.
    #[must_use]
    pub fn text(&self) -> Option<&str> {
        self.body.first().and_then(Value::as_str)
    }

    /// The message as the bytes that go on the wire, numbered `serial`.
    ///
    /// # Errors
    /// A value that cannot be written: an array holding what its signature
    /// does not say, or text with a NUL in it.
    pub fn encode(&self, serial: u32) -> Result<Vec<u8>, String> {
        let mut body = Writer::default();
        let mut signature = String::new();
        for value in &self.body {
            signature.push_str(&value.signature());
            body.put(value)?;
        }
        check_signature(&signature)?;

        let text = |code: u8, value: &Option<String>, path: bool| {
            value.as_ref().map(|text| {
                let value = if path {
                    Value::Path(text.clone())
                } else {
                    Value::Str(text.clone())
                };
                field(code, value)
            })
        };
        let mut fields: Vec<Value> = [
            text(1, &self.path, true),
            text(2, &self.interface, false),
            text(3, &self.member, false),
            text(4, &self.error_name, false),
            self.reply_serial.map(|n| field(5, Value::U32(n))),
            text(6, &self.destination, false),
        ]
        .into_iter()
        .flatten()
        .collect();
        if !signature.is_empty() {
            fields.push(field(8, Value::Signature(signature)));
        }

        let mut out = Writer::default();
        out.buf.extend_from_slice(&[
            b'l',
            match self.kind {
                Kind::Call => 1,
                Kind::Return => 2,
                Kind::Error => 3,
                Kind::Signal => 4,
            },
            u8::from(self.no_reply),
            1,
        ]);
        let length = u32::try_from(body.buf.len()).map_err(|_| "the body is too large")?;
        out.buf.extend_from_slice(&length.to_le_bytes());
        out.buf.extend_from_slice(&serial.to_le_bytes());
        out.put(&Value::Array("(yv)".into(), fields))?;
        out.align(8);
        out.buf.extend_from_slice(&body.buf);
        if out.buf.len() > MAX_MESSAGE {
            return Err("the message is too large".into());
        }
        Ok(out.buf)
    }

    /// Read one message from `stream`.
    ///
    /// # Errors
    /// The stream ended or failed, or what came is not a message.
    pub fn read(stream: &mut impl std::io::Read) -> Result<Self, String> {
        let mut start = [0u8; 16];
        stream.read_exact(&mut start).map_err(|e| e.to_string())?;
        let little = match start[0] {
            b'l' => true,
            b'B' => false,
            other => return Err(format!("not a message: it begins with {other:#04x}")),
        };
        let number = |at: usize| {
            let bytes = [start[at], start[at + 1], start[at + 2], start[at + 3]];
            let n = if little {
                u32::from_le_bytes(bytes)
            } else {
                u32::from_be_bytes(bytes)
            };
            usize::try_from(n).map_err(|_| "a length too large for this machine".to_owned())
        };
        let (body_length, fields_length) = (number(4)?, number(12)?);
        // The fields are padded to a multiple of eight, where the body starts.
        let header = 16usize
            .checked_add(fields_length)
            .and_then(|n| n.checked_next_multiple_of(8))
            .filter(|n| *n <= MAX_MESSAGE)
            .ok_or("the header is too large")?;
        let total = header
            .checked_add(body_length)
            .filter(|n| *n <= MAX_MESSAGE)
            .ok_or("the message is too large")?;
        let mut data = vec![0u8; total];
        data[..16].copy_from_slice(&start);
        stream
            .read_exact(&mut data[16..])
            .map_err(|e| e.to_string())?;
        Self::decode(&data, header, little)
    }

    /// A whole message's bytes, its body starting at `header`.
    fn decode(data: &[u8], header: usize, little: bool) -> Result<Self, String> {
        let mut message = Self::new(match data[1] {
            1 => Kind::Call,
            2 => Kind::Return,
            3 => Kind::Error,
            4 => Kind::Signal,
            other => return Err(format!("a message of kind {other}")),
        });
        if data[3] != 1 {
            return Err(format!("protocol version {}", data[3]));
        }
        message.no_reply = data[2] & 1 != 0;
        let mut reader = Reader {
            data: &data[..header],
            at: 8,
            little,
            depth: 0,
        };
        message.serial = match reader.value("u")? {
            Value::U32(n) if n != 0 => n,
            _ => return Err("a message numbered zero".into()),
        };
        let mut signature = String::new();
        let fields = reader.value("a(yv)")?;
        for entry in fields.items() {
            let [Value::U8(code), value] = entry.items() else {
                continue;
            };
            let text = || value.as_str().map(str::to_owned);
            match (code, value.plain()) {
                (1, Value::Path(_)) => message.path = text(),
                (2, Value::Str(_)) => message.interface = text(),
                (3, Value::Str(_)) => message.member = text(),
                (4, Value::Str(_)) => message.error_name = text(),
                (5, Value::U32(n)) => message.reply_serial = Some(*n),
                (6, Value::Str(_)) => message.destination = text(),
                (7, Value::Str(_)) => message.sender = text(),
                (8, Value::Signature(s)) => signature.clone_from(s),
                // File descriptors are never asked for, so never come.
                (9, _) => return Err("a message carrying file descriptors".into()),
                // A field of a kind not known, or of the wrong type for its
                // number: skipped, as the specification asks of the first.
                _ => {}
            }
        }
        let mut body = Reader {
            data: &data[header..],
            at: 0,
            little,
            depth: 0,
        };
        let mut rest = signature.as_str();
        while !rest.is_empty() {
            let (one, after) = first_type(rest)?;
            message.body.push(body.value(one)?);
            rest = after;
        }
        if body.at != body.data.len() {
            return Err("a body longer than its signature".into());
        }
        Ok(message)
    }
}

/// One header field: its number and its value.
fn field(code: u8, value: Value) -> Value {
    Value::Struct(vec![Value::U8(code), Value::variant(value)])
}

/// The boundary a value whose signature starts with `kind` is aligned to.
const fn alignment(kind: u8) -> usize {
    match kind {
        b'y' | b'g' | b'v' => 1,
        b'n' | b'q' => 2,
        b'x' | b't' | b'd' | b'(' | b'{' => 8,
        _ => 4,
    }
}

/// The first complete type of `signature`, and what follows it.
fn first_type(signature: &str) -> Result<(&str, &str), String> {
    let bytes = signature.as_bytes();
    let mut at = 0;
    // Any number of arrays, then one thing.
    while bytes.get(at) == Some(&b'a') {
        at += 1;
    }
    let end = match bytes.get(at) {
        Some(
            b'y' | b'b' | b'n' | b'q' | b'i' | b'u' | b'x' | b't' | b'd' | b's' | b'o' | b'g'
            | b'v' | b'h',
        ) => at + 1,
        Some(open @ (b'(' | b'{')) => {
            let close = if *open == b'(' { b')' } else { b'}' };
            let mut depth = 0usize;
            let mut end = None;
            for (i, byte) in bytes.iter().enumerate().skip(at) {
                if byte == open {
                    depth += 1;
                } else if *byte == close {
                    depth -= 1;
                    if depth == 0 {
                        end = Some(i + 1);
                        break;
                    }
                }
            }
            end.ok_or_else(|| format!("an unclosed bracket in {signature:?}"))?
        }
        _ => return Err(format!("not a signature: {signature:?}")),
    };
    Ok(signature.split_at(end))
}

/// Whether `signature` is one: complete types end to end, nested no deeper
/// than may be.
fn check_signature(signature: &str) -> Result<(), String> {
    fn check(signature: &str, depth: usize) -> Result<(), String> {
        if depth > MAX_DEPTH {
            return Err("a signature nested too deep".into());
        }
        let mut rest = signature;
        while !rest.is_empty() {
            let (one, after) = first_type(rest)?;
            let inner = one.trim_start_matches('a');
            let arrays = one.len() - inner.len();
            if let Some(fields) = inner.strip_prefix('(').and_then(|s| s.strip_suffix(')')) {
                if fields.is_empty() {
                    return Err("an empty struct".into());
                }
                check(fields, depth + arrays + 1)?;
            } else if let Some(entry) = inner.strip_prefix('{').and_then(|s| s.strip_suffix('}')) {
                let (key, value) = first_type(entry)?;
                let (_, nothing) = first_type(value)?;
                let basic = key.len() == 1 && !matches!(key, "v" | "(" | "{");
                if arrays == 0 || !basic || !nothing.is_empty() {
                    return Err(format!("not a dictionary entry: {inner:?}"));
                }
                check(value, depth + arrays + 1)?;
            } else if depth + arrays > MAX_DEPTH {
                return Err("a signature nested too deep".into());
            }
            rest = after;
        }
        Ok(())
    }
    if signature.len() > 255 {
        return Err("a signature too long".into());
    }
    check(signature, 0)
}

/// Bytes being written, little end first.
#[derive(Default)]
struct Writer {
    buf: Vec<u8>,
}

impl Writer {
    fn align(&mut self, to: usize) {
        self.buf.resize(self.buf.len().next_multiple_of(to), 0);
    }

    fn text(&mut self, text: &str) -> Result<(), String> {
        if text.contains('\0') {
            return Err("text with a NUL in it".into());
        }
        let length = u32::try_from(text.len()).map_err(|_| "text too long")?;
        self.align(4);
        self.buf.extend_from_slice(&length.to_le_bytes());
        self.buf.extend_from_slice(text.as_bytes());
        self.buf.push(0);
        Ok(())
    }

    fn signature(&mut self, signature: &str) -> Result<(), String> {
        check_signature(signature)?;
        // Checked to be at most 255 long.
        self.buf
            .push(u8::try_from(signature.len()).unwrap_or(u8::MAX));
        self.buf.extend_from_slice(signature.as_bytes());
        self.buf.push(0);
        Ok(())
    }

    fn put(&mut self, value: &Value) -> Result<(), String> {
        match value {
            Value::U8(n) => self.buf.push(*n),
            Value::Bool(b) => {
                self.align(4);
                self.buf.extend_from_slice(&u32::from(*b).to_le_bytes());
            }
            Value::I16(n) => {
                self.align(2);
                self.buf.extend_from_slice(&n.to_le_bytes());
            }
            Value::U16(n) => {
                self.align(2);
                self.buf.extend_from_slice(&n.to_le_bytes());
            }
            Value::I32(n) => {
                self.align(4);
                self.buf.extend_from_slice(&n.to_le_bytes());
            }
            Value::U32(n) => {
                self.align(4);
                self.buf.extend_from_slice(&n.to_le_bytes());
            }
            Value::I64(n) => {
                self.align(8);
                self.buf.extend_from_slice(&n.to_le_bytes());
            }
            Value::U64(n) => {
                self.align(8);
                self.buf.extend_from_slice(&n.to_le_bytes());
            }
            Value::F64(n) => {
                self.align(8);
                self.buf.extend_from_slice(&n.to_le_bytes());
            }
            Value::Str(text) => self.text(text)?,
            Value::Path(path) => {
                if !valid_path(path) {
                    return Err(format!("not an object's path: {path:?}"));
                }
                self.text(path)?;
            }
            Value::Signature(signature) => self.signature(signature)?,
            Value::Array(of, items) => {
                let (one, nothing) = first_type(of)?;
                if !nothing.is_empty() {
                    return Err(format!("an array of more than one type: {of:?}"));
                }
                self.align(4);
                let length_at = self.buf.len();
                self.buf.extend_from_slice(&[0; 4]);
                // The padding before the first item is not part of the length.
                self.align(alignment(one.as_bytes()[0]));
                let start = self.buf.len();
                for item in items {
                    if item.signature() != *of {
                        return Err(format!("{} in an array of {of}", item.signature()));
                    }
                    self.put(item)?;
                }
                let length = u32::try_from(self.buf.len() - start)
                    .ok()
                    .filter(|n| *n <= 1 << 26)
                    .ok_or("an array too long")?;
                self.buf[length_at..length_at + 4].copy_from_slice(&length.to_le_bytes());
            }
            Value::Struct(fields) => {
                self.align(8);
                for field in fields {
                    self.put(field)?;
                }
            }
            Value::Entry(key, value) => {
                self.align(8);
                self.put(key)?;
                self.put(value)?;
            }
            Value::Variant(inner) => {
                self.signature(&inner.signature())?;
                self.put(inner)?;
            }
        }
        Ok(())
    }
}

/// Whether `path` is an object's path as the bus will have it: a slash, or
/// names of letters, digits and underscores, each after a slash.
#[must_use]
pub fn valid_path(path: &str) -> bool {
    path == "/"
        || path.strip_prefix('/').is_some_and(|rest| {
            rest.split('/').all(|part| {
                !part.is_empty() && part.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
            })
        })
}

/// Bytes being read.
struct Reader<'a> {
    data: &'a [u8],
    at: usize,
    little: bool,
    depth: usize,
}

impl Reader<'_> {
    fn align(&mut self, to: usize) -> Result<(), String> {
        let at = self.at.next_multiple_of(to);
        if at > self.data.len() {
            return Err("a message cut short".into());
        }
        self.at = at;
        Ok(())
    }

    fn take<const N: usize>(&mut self) -> Result<[u8; N], String> {
        self.align(N)?;
        let bytes = self
            .at
            .checked_add(N)
            .and_then(|end| self.data.get(self.at..end))
            .ok_or("a message cut short")?;
        self.at += N;
        // The slice is N long, so this cannot fail.
        bytes
            .try_into()
            .map_err(|_| "a message cut short".to_owned())
    }

    fn u32(&mut self) -> Result<u32, String> {
        let bytes = self.take::<4>()?;
        Ok(if self.little {
            u32::from_le_bytes(bytes)
        } else {
            u32::from_be_bytes(bytes)
        })
    }

    /// `length` bytes of text and the NUL after them.
    fn text(&mut self, length: usize) -> Result<String, String> {
        let end = self
            .at
            .checked_add(length)
            .filter(|end| *end < self.data.len())
            .ok_or("text longer than its message")?;
        if self.data[end] != 0 {
            return Err("text that does not end where it says".into());
        }
        let text =
            std::str::from_utf8(&self.data[self.at..end]).map_err(|_| "text that is not UTF-8")?;
        if text.contains('\0') {
            return Err("text with a NUL in it".into());
        }
        self.at = end + 1;
        Ok(text.to_owned())
    }

    fn signature(&mut self) -> Result<String, String> {
        let length = self.take::<1>()?[0];
        let signature = self.text(length.into())?;
        check_signature(&signature)?;
        Ok(signature)
    }

    /// The value of one complete type, `signature`.
    fn value(&mut self, signature: &str) -> Result<Value, String> {
        if self.depth >= MAX_DEPTH {
            return Err("values nested too deep".into());
        }
        self.depth += 1;
        let value = self.value_at_depth(signature);
        self.depth -= 1;
        value
    }

    fn value_at_depth(&mut self, signature: &str) -> Result<Value, String> {
        macro_rules! number {
            ($kind:ident, $type:ty) => {{
                let bytes = self.take()?;
                Value::$kind(if self.little {
                    <$type>::from_le_bytes(bytes)
                } else {
                    <$type>::from_be_bytes(bytes)
                })
            }};
        }
        Ok(match signature.as_bytes().first() {
            Some(b'y') => Value::U8(self.take::<1>()?[0]),
            Some(b'b') => match self.u32()? {
                0 => Value::Bool(false),
                1 => Value::Bool(true),
                other => return Err(format!("a boolean of {other}")),
            },
            Some(b'n') => number!(I16, i16),
            Some(b'q') => number!(U16, u16),
            Some(b'i') => number!(I32, i32),
            Some(b'u') => number!(U32, u32),
            Some(b'x') => number!(I64, i64),
            Some(b't') => number!(U64, u64),
            Some(b'd') => number!(F64, f64),
            Some(b's') => {
                let length = self.u32()?;
                Value::Str(self.text(usize::try_from(length).map_err(|_| "text too long")?)?)
            }
            Some(b'o') => {
                let length = self.u32()?;
                let path = self.text(usize::try_from(length).map_err(|_| "text too long")?)?;
                if !valid_path(&path) {
                    return Err(format!("not an object's path: {path:?}"));
                }
                Value::Path(path)
            }
            Some(b'g') => Value::Signature(self.signature()?),
            Some(b'v') => {
                let signature = self.signature()?;
                let (one, nothing) = first_type(&signature)?;
                if !nothing.is_empty() {
                    return Err("a variant of more than one value".into());
                }
                Value::variant(self.value(one)?)
            }
            Some(b'a') => {
                let of = &signature[1..];
                let length = usize::try_from(self.u32()?).map_err(|_| "an array too long")?;
                if length > 1 << 26 {
                    return Err("an array too long".into());
                }
                self.align(alignment(of.as_bytes().first().copied().unwrap_or(b'y')))?;
                let end = self
                    .at
                    .checked_add(length)
                    .filter(|end| *end <= self.data.len())
                    .ok_or("an array longer than its message")?;
                let mut items = Vec::new();
                // Every kind of value takes at least a byte, so this ends.
                while self.at < end {
                    items.push(self.value(of)?);
                }
                if self.at != end {
                    return Err("an array that ends inside an item".into());
                }
                Value::Array(of.to_owned(), items)
            }
            Some(b'(') => {
                self.align(8)?;
                let mut rest = &signature[1..signature.len() - 1];
                let mut fields = Vec::new();
                while !rest.is_empty() {
                    let (one, after) = first_type(rest)?;
                    fields.push(self.value(one)?);
                    rest = after;
                }
                Value::Struct(fields)
            }
            Some(b'{') => {
                self.align(8)?;
                let (key, value) = first_type(&signature[1..signature.len() - 1])?;
                Value::Entry(Box::new(self.value(key)?), Box::new(self.value(value)?))
            }
            Some(b'h') => return Err("a file descriptor, which was never asked for".into()),
            _ => return Err(format!("not a signature: {signature:?}")),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{Kind, MAX_MESSAGE, Message, check_signature, first_type, valid_path};
    use crate::value::Value;

    fn round(message: &Message) -> Message {
        let bytes = message.encode(7).unwrap();
        let mut back = Message::read(&mut bytes.as_slice()).unwrap();
        assert_eq!(back.serial, 7);
        back.serial = message.serial;
        back
    }

    /// One of everything, nested: what iwd's `GetManagedObjects` is made of,
    /// and the rest.
    fn everything() -> Vec<Value> {
        let properties = Value::named([
            ("Name", "Fjellheim".into()),
            ("Connected", true.into()),
            ("Frequency", Value::U32(5180)),
            ("RSSI", Value::I16(-61)),
            ("KnownNetwork", Value::path("/net/connman/iwd/known_1")),
            ("Modes", Value::strings(["station", "ap"])),
            ("Nothing", Value::Array("s".into(), Vec::new())),
        ]);
        let interfaces = Value::Array(
            "{sa{sv}}".into(),
            vec![Value::Entry(
                Box::new("net.connman.iwd.Network".into()),
                Box::new(properties),
            )],
        );
        vec![
            Value::Array(
                "{oa{sa{sv}}}".into(),
                vec![Value::Entry(
                    Box::new(Value::path("/net/connman/iwd/0/4")),
                    Box::new(interfaces),
                )],
            ),
            Value::U8(200),
            Value::U16(65535),
            Value::I32(-2),
            Value::I64(i64::MIN),
            Value::U64(u64::MAX),
            Value::F64(1.5),
            Value::Signature("a{sv}(io)".into()),
            Value::Struct(vec![Value::U8(1), Value::I64(2), "x".into(), Value::U8(3)]),
            Value::Array(
                "(os)".into(),
                vec![Value::Struct(vec![Value::path("/"), "æøå".into()])],
            ),
            Value::Array(
                "ay".into(),
                vec![Value::Array("y".into(), vec![Value::U8(9)])],
            ),
            Value::Array("x".into(), Vec::new()),
            Value::variant(Value::variant("deep".into())),
        ]
    }

    #[test]
    fn a_message_comes_back_as_it_went() {
        let mut call = Message::new(Kind::Call);
        call.path = Some("/net/connman/iwd".into());
        call.interface = Some("net.connman.iwd.AgentManager".into());
        call.member = Some("RegisterAgent".into());
        call.destination = Some("net.connman.iwd".into());
        call.body = everything();
        assert_eq!(round(&call), call);

        let mut error = Message::new(Kind::Error);
        error.no_reply = true;
        error.reply_serial = Some(41);
        error.error_name = Some("net.connman.iwd.Failed".into());
        error.body = vec!["Operation failed".into()];
        assert_eq!(round(&error), error);
        assert_eq!(round(&error).text(), Some("Operation failed"));

        // Nothing in the body is no signature at all.
        let mut signal = Message::new(Kind::Signal);
        signal.path = Some("/".into());
        signal.interface = Some("a.b".into());
        signal.member = Some("C".into());
        assert_eq!(round(&signal), signal);
    }

    /// The bytes of `Hello`, by hand from the specification: what every
    /// connection sends first, so what any bus has to accept.
    #[test]
    fn hello_is_the_bytes_the_specification_says() {
        let mut hello = Message::new(Kind::Call);
        hello.path = Some("/org/freedesktop/DBus".into());
        hello.interface = Some("org.freedesktop.DBus".into());
        hello.member = Some("Hello".into());
        hello.destination = Some("org.freedesktop.DBus".into());
        let bytes = hello.encode(1).unwrap();
        let mut expected = vec![b'l', 1, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0, 109, 0, 0, 0];
        // (yv) PATH: 1, "o", then the text aligned to four.
        expected.extend_from_slice(&[1, 1, b'o', 0, 21, 0, 0, 0]);
        expected.extend_from_slice(b"/org/freedesktop/DBus\0\0\0");
        expected.extend_from_slice(&[2, 1, b's', 0, 20, 0, 0, 0]);
        expected.extend_from_slice(b"org.freedesktop.DBus\0\0\0\0");
        expected.extend_from_slice(&[3, 1, b's', 0, 5, 0, 0, 0]);
        expected.extend_from_slice(b"Hello\0\0\0");
        expected.extend_from_slice(&[6, 1, b's', 0, 20, 0, 0, 0]);
        expected.extend_from_slice(b"org.freedesktop.DBus\0");
        // And the whole header padded to a multiple of eight.
        expected.extend_from_slice(&[0, 0, 0]);
        assert_eq!(bytes, expected);
        assert_eq!(bytes.len() % 8, 0);
    }

    #[test]
    fn a_message_with_the_big_end_first_is_read_too() {
        let mut bytes = vec![b'B', 4, 0, 1, 0, 0, 0, 6, 0, 0, 0, 5, 0, 0, 0, 8];
        bytes.extend_from_slice(&[8, 1, b'g', 0, 2, b'u', b'n', 0]);
        bytes.extend_from_slice(&[0, 0, 1, 2, 0xff, 0xfe]);
        let message = Message::read(&mut bytes.as_slice()).unwrap();
        assert_eq!(message.kind, Kind::Signal);
        assert_eq!(message.serial, 5);
        assert_eq!(message.body, [Value::U32(258), Value::I16(-2)]);
    }

    #[test]
    fn signatures_are_taken_apart_where_their_types_end() {
        assert_eq!(first_type("a{sv}ii").unwrap(), ("a{sv}", "ii"));
        assert_eq!(first_type("aa(i(ss))s").unwrap(), ("aa(i(ss))", "s"));
        assert_eq!(first_type("s").unwrap(), ("s", ""));
        for wrong in ["", "a", "(i", "{sv", "z", ")", "a}"] {
            assert!(first_type(wrong).is_err(), "{wrong}");
        }
        for right in ["", "a{sv}", "a{oa{sa{sv}}}", "(ii)a(os)v", "aay"] {
            assert!(check_signature(right).is_ok(), "{right}");
        }
        let deep = format!("{}i", "a".repeat(65));
        for wrong in [
            "()",
            "{sv}",
            "a{vs}",
            "a{sii}",
            "a{(i)s}",
            "a{s}",
            deep.as_str(),
        ] {
            assert!(check_signature(wrong).is_err(), "{wrong}");
        }
        assert!(check_signature(&"i".repeat(256)).is_err());
    }

    #[test]
    fn what_cannot_be_written_is_refused_and_not_sent() {
        let mut call = Message::new(Kind::Call);
        for wrong in [
            Value::Str("a\0b".into()),
            Value::Path("not/a/path".into()),
            Value::Path("/trailing/".into()),
            Value::Array("s".into(), vec![Value::U32(1)]),
            Value::Array("si".into(), Vec::new()),
            Value::Signature("(".into()),
            Value::Array("{vs}".into(), Vec::new()),
        ] {
            call.body = vec![wrong.clone()];
            assert!(call.encode(1).is_err(), "{wrong:?}");
        }
        assert!(valid_path("/") && valid_path("/a/b_1"));
        assert!(!valid_path("") && !valid_path("//") && !valid_path("/a-b") && !valid_path("a"));
    }

    #[test]
    fn a_message_that_does_not_hold_together_is_an_error() {
        let mut call = Message::new(Kind::Call);
        call.path = Some("/a".into());
        call.member = Some("M".into());
        call.body = everything();
        let good = call.encode(3).unwrap();

        // Cut short anywhere.
        for cut in [0, 1, 15, 16, 40, good.len() / 2, good.len() - 1] {
            assert!(Message::read(&mut &good[..cut]).is_err(), "cut at {cut}");
        }
        let broken = |change: &dyn Fn(&mut Vec<u8>)| {
            let mut bytes = good.clone();
            change(&mut bytes);
            Message::read(&mut bytes.as_slice())
        };
        assert!(broken(&|b| b[0] = b'x').is_err());
        assert!(broken(&|b| b[1] = 9).is_err());
        assert!(broken(&|b| b[3] = 2).is_err());
        // Numbered zero.
        assert!(broken(&|b| b[8..12].fill(0)).is_err());
        // Lengths that promise more than there is, or more than is read at all.
        assert!(broken(&|b| b[4..8].copy_from_slice(&u32::MAX.to_le_bytes())).is_err());
        assert!(broken(&|b| b[12..16].copy_from_slice(&u32::MAX.to_le_bytes())).is_err());
        let huge = u32::try_from(MAX_MESSAGE).unwrap().to_le_bytes();
        assert!(broken(&|b| b[4..8].copy_from_slice(&huge)).is_err());
        // A body with more in it than its signature says.
        assert!(
            broken(&|b| {
                let length = u32::from_le_bytes([b[4], b[5], b[6], b[7]]) + 8;
                b[4..8].copy_from_slice(&length.to_le_bytes());
                b.extend_from_slice(&[0; 8]);
            })
            .is_err()
        );
    }

    /// Every byte of a good message changed, one at a time and several at
    /// once: read as a message or refused, and never a panic or a hang.
    #[test]
    fn no_damage_to_a_message_is_worse_than_an_error() {
        let mut call = Message::new(Kind::Return);
        call.reply_serial = Some(2);
        call.body = everything();
        let good = call.encode(3).unwrap();
        let mut seed = 0x9e37_79b9_7f4a_7c15_u64;
        let mut next = move || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed
        };
        let mut read = 0u32;
        for at in 0..good.len() {
            for value in [0, 1, 0x7f, 0x80, 0xff, good[at].wrapping_add(1)] {
                let mut bytes = good.clone();
                bytes[at] = value;
                read += u32::from(Message::read(&mut bytes.as_slice()).is_ok());
            }
        }
        for _ in 0..20_000 {
            let mut bytes = good.clone();
            for _ in 0..=next() % 6 {
                let at = usize::try_from(next()).unwrap_or(0) % bytes.len();
                bytes[at] = next().to_le_bytes()[0];
            }
            read += u32::from(Message::read(&mut bytes.as_slice()).is_ok());
        }
        // Some damage is to padding and to text, and reads; most does not.
        assert!(read > 0);
    }

    #[test]
    fn values_nested_past_the_limit_are_refused_before_the_stack_is() {
        // A variant in a variant in a variant, a thousand deep.
        let mut body = Vec::new();
        for _ in 0..1000 {
            body.extend_from_slice(&[1, b'v', 0]);
        }
        body.extend_from_slice(&[1, b'y', 0, 7]);
        let mut bytes = vec![b'l', 2, 0, 1];
        bytes.extend_from_slice(&u32::try_from(body.len()).unwrap().to_le_bytes());
        bytes.extend_from_slice(&1u32.to_le_bytes());
        bytes.extend_from_slice(&7u32.to_le_bytes());
        bytes.extend_from_slice(&[8, 1, b'g', 0, 1, b'v', 0, 0]);
        bytes.extend_from_slice(&body);
        let error = Message::read(&mut bytes.as_slice()).unwrap_err();
        assert!(error.contains("nested too deep"), "{error}");
    }
}
