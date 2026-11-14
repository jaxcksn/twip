//! Twip's JSON wire: one concrete encoder and decoder per API type.
//!
//! The derives read the same `#[serde(...)]` (or `#[wire(...)]`) attributes serde does and write the same JSON, but
//! each type gets one non-generic [`Encode::encode`] into a `Vec<u8>` and one [`Decode::decode`] from a [`Reader`]
//! over the input slice. An internally tagged enum finds its tag by scanning ahead in the slice instead of buffering
//! the object; an untagged enum tries its variants from a saved position.

use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt;
use std::hash::{BuildHasher, Hash};
use std::rc::Rc;
use std::sync::Arc;

pub use twip_wire_derive::{Decode, Encode};

/// How deep arrays and objects may nest before the input is refused (serde_json's limit).
pub const MAX_DEPTH: u32 = 128;

/// Writes itself as JSON.
pub trait Encode {
    /// Appends this value's JSON to `out`.
    fn encode(&self, out: &mut Vec<u8>);
}

/// Reads itself from JSON.
pub trait Decode: Sized {
    /// Reads one value at the reader's position.
    fn decode(r: &mut Reader<'_>) -> Result<Self, Error>;

    /// The value of a field that isn't there, for a type that has one (`Option`'s `None`), as serde's.
    fn absent() -> Option<Self> {
        None
    }
}

/// A type whose JSON is an object, written field by field so a tagged enum can put the fields beside its tag.
pub trait EncodeFields {
    /// Appends this value's fields, each preceded by a comma unless `first`.
    fn encode_fields(&self, out: &mut Vec<u8>, first: &mut bool);
}

/// A type whose JSON is an object, read with the enclosing enums' tag keys passed over.
pub trait DecodeFields: Sized {
    /// Reads one object at the reader's position, skipping the keys in `tags`.
    fn decode_fields(r: &mut Reader<'_>, tags: Option<&Tags<'_>>) -> Result<Self, Error>;
}

/// A map key: written and read as a JSON string.
pub trait MapKey: Sized {
    /// Appends this key as a JSON string.
    fn encode_key(&self, out: &mut Vec<u8>);
    /// Reads a key from its string.
    fn decode_key(key: &str) -> Result<Self, Error>;
}

/// A unit-variant enum's variants by their wire names (`setBold`), derived with [`Encode`].
pub trait Named: Sized {
    /// This variant's name.
    fn name(&self) -> &'static str;
    /// The variant with this name.
    fn from_name(name: &str) -> Option<Self>;
}

/// The tag keys of the internally tagged enums an object is read inside, innermost first.
#[derive(Clone, Copy)]
pub struct Tags<'s> {
    /// This enum's tag key.
    pub key: &'s str,
    /// The enclosing enum's.
    pub outer: Option<&'s Tags<'s>>,
}

/// Whether `key` is one of the tag keys.
pub fn is_tag(tags: Option<&Tags<'_>>, key: &str) -> bool {
    let mut t = tags;
    while let Some(x) = t {
        if x.key == key {
            return true;
        }
        t = x.outer;
    }
    false
}

/// Encodes `value` to a JSON byte vector.
pub fn to_vec<T: Encode + ?Sized>(value: &T) -> Vec<u8> {
    // serde_json's starting capacity: most messages fit without growing.
    let mut out = Vec::with_capacity(128);
    value.encode(&mut out);
    out
}

/// Decodes one `T` from all of `json`.
pub fn from_slice<T: Decode>(json: &[u8]) -> Result<T, Error> {
    let mut r = Reader::new(json)?;
    let v = T::decode(&mut r)?;
    r.finish()?;
    Ok(v)
}

/// Decodes one `T` from all of `json`.
pub fn from_str<T: Decode>(json: &str) -> Result<T, Error> {
    from_slice(json.as_bytes())
}

// Errors

/// Why an input didn't decode: a message and the byte offset it was found at.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Error {
    message: String,
    offset: usize,
}

impl Error {
    /// An error with this message at `offset`.
    #[cold]
    pub fn new(message: impl Into<String>, offset: usize) -> Error {
        Error { message: message.into(), offset }
    }

    /// The message, without the offset.
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} at byte {}", self.message, self.offset)
    }
}

impl std::error::Error for Error {}

// Writing

/// Writes `"key":`, after a comma unless `first`. `raw` is the key already quoted, with its colon.
#[inline]
pub fn field(out: &mut Vec<u8>, first: &mut bool, raw: &[u8]) {
    if !std::mem::take(first) {
        out.push(b',');
    }
    out.extend_from_slice(raw);
}

/// Writes `s` as a JSON string, escaped as serde_json escapes.
pub fn write_str(out: &mut Vec<u8>, s: &str) {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    out.push(b'"');
    let b = s.as_bytes();
    let mut start = 0;
    for (i, &c) in b.iter().enumerate() {
        let esc: &[u8] = match c {
            b'"' => b"\\\"",
            b'\\' => b"\\\\",
            b'\n' => b"\\n",
            b'\r' => b"\\r",
            b'\t' => b"\\t",
            0x08 => b"\\b",
            0x0c => b"\\f",
            0..=0x1f => &[b'\\', b'u', b'0', b'0', HEX[(c >> 4) as usize], HEX[(c & 15) as usize]],
            _ => continue,
        };
        out.extend_from_slice(&b[start..i]);
        out.extend_from_slice(esc);
        start = i + 1;
    }
    out.extend_from_slice(&b[start..]);
    out.push(b'"');
}

/// Writes `Some` as its value and `None` as `null` (an `Option<Option<T>>` field that is present).
pub fn encode_nullable<T: Encode>(v: &Option<Option<T>>, out: &mut Vec<u8>) {
    match v {
        Some(v) => v.encode(out),
        None => out.extend_from_slice(b"null"),
    }
}

// Reading

/// The kind of value at a position.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// `null`.
    Null,
    /// `true` or `false`.
    Bool,
    /// A number.
    Number,
    /// A string.
    String,
    /// An array.
    Array,
    /// An object.
    Object,
}

/// A pull parser over one JSON input. Cloning saves its position.
#[derive(Clone)]
pub struct Reader<'a> {
    s: &'a str,
    pos: usize,
    depth: u32,
}

/// Iterates an object's keys; see [`Reader::object`].
pub struct Object {
    first: bool,
}

/// Iterates an array's items; see [`Reader::array`].
pub struct Array {
    first: bool,
}

impl<'a> Reader<'a> {
    /// A reader at the start of `json`, which must be UTF-8.
    pub fn new(json: &'a [u8]) -> Result<Reader<'a>, Error> {
        let s = std::str::from_utf8(json).map_err(|e| Error::new("invalid UTF-8", e.valid_up_to()))?;
        Ok(Reader { s, pos: 0, depth: 0 })
    }

    /// An error at the current position.
    #[cold]
    pub fn error(&self, message: impl Into<String>) -> Error {
        Error::new(message, self.pos)
    }

    /// Refuses anything after the value but whitespace.
    pub fn finish(&mut self) -> Result<(), Error> {
        match self.peek_byte() {
            None => Ok(()),
            Some(_) => Err(self.error("trailing characters")),
        }
    }

    fn bytes(&self) -> &'a [u8] {
        self.s.as_bytes()
    }

    fn peek_byte(&mut self) -> Option<u8> {
        let b = self.bytes();
        while let Some(&c) = b.get(self.pos) {
            if !matches!(c, b' ' | b'\n' | b'\r' | b'\t') {
                return Some(c);
            }
            self.pos += 1;
        }
        None
    }

    fn expect_byte(&mut self, want: u8, what: &str) -> Result<(), Error> {
        if self.peek_byte() == Some(want) {
            self.pos += 1;
            Ok(())
        } else {
            Err(self.error(format!("expected {what}")))
        }
    }

    fn literal(&mut self, word: &str) -> Result<(), Error> {
        if self.s[self.pos..].starts_with(word) {
            self.pos += word.len();
            Ok(())
        } else {
            Err(self.error("expected value"))
        }
    }

    /// The kind of the next value.
    pub fn peek(&mut self) -> Result<Kind, Error> {
        Ok(match self.peek_byte() {
            Some(b'n') => Kind::Null,
            Some(b't' | b'f') => Kind::Bool,
            Some(b'-' | b'0'..=b'9') => Kind::Number,
            Some(b'"') => Kind::String,
            Some(b'[') => Kind::Array,
            Some(b'{') => Kind::Object,
            Some(_) => return Err(self.error("expected value")),
            None => return Err(self.error("EOF while parsing a value")),
        })
    }

    /// Reads `null` if it is next.
    pub fn null(&mut self) -> Result<bool, Error> {
        if self.peek_byte() == Some(b'n') {
            self.literal("null")?;
            return Ok(true);
        }
        Ok(false)
    }

    /// Reads a boolean.
    pub fn bool(&mut self) -> Result<bool, Error> {
        match self.peek_byte() {
            Some(b't') => self.literal("true").map(|_| true),
            Some(b'f') => self.literal("false").map(|_| false),
            _ => Err(self.error("expected a boolean")),
        }
    }

    /// Reads a number's text, checking its grammar; whether it is an integer (no fraction or exponent).
    fn number(&mut self) -> Result<(&'a str, bool), Error> {
        if !matches!(self.peek_byte(), Some(b'-' | b'0'..=b'9')) {
            return Err(self.error("expected a number"));
        }
        let b = self.bytes();
        let start = self.pos;
        let mut i = start;
        let digits = |i: &mut usize| {
            let s = *i;
            while b.get(*i).is_some_and(u8::is_ascii_digit) {
                *i += 1;
            }
            *i > s
        };
        if b.get(i) == Some(&b'-') {
            i += 1;
        }
        let int_start = i;
        if !digits(&mut i) || (b[int_start] == b'0' && i - int_start > 1) {
            self.pos = i;
            return Err(self.error("invalid number"));
        }
        let mut integer = true;
        if b.get(i) == Some(&b'.') {
            i += 1;
            integer = false;
            if !digits(&mut i) {
                self.pos = i;
                return Err(self.error("invalid number"));
            }
        }
        if matches!(b.get(i), Some(b'e' | b'E')) {
            i += 1;
            integer = false;
            if matches!(b.get(i), Some(b'+' | b'-')) {
                i += 1;
            }
            if !digits(&mut i) {
                self.pos = i;
                return Err(self.error("invalid number"));
            }
        }
        self.pos = i;
        Ok((&self.s[start..i], integer))
    }

    /// Reads an integer (no fraction or exponent), refusing one outside `T`'s range.
    pub fn int<T: std::str::FromStr>(&mut self) -> Result<T, Error> {
        let at = self.pos;
        let (text, integer) = self.number()?;
        if !integer {
            return Err(Error::new("expected an integer", at));
        }
        text.parse().map_err(|_| Error::new("integer out of range", at))
    }

    /// Reads a number as `f64`.
    pub fn f64(&mut self) -> Result<f64, Error> {
        let at = self.pos;
        let (text, _) = self.number()?;
        text.parse().map_err(|_| Error::new("invalid number", at))
    }

    /// Reads a string, borrowed from the input unless it has escapes.
    pub fn str(&mut self) -> Result<Cow<'a, str>, Error> {
        self.expect_byte(b'"', "a string")?;
        let b = self.bytes();
        let start = self.pos;
        let mut i = start;
        loop {
            match b.get(i) {
                None => {
                    self.pos = i;
                    return Err(self.error("EOF while parsing a string"));
                }
                Some(b'"') => {
                    self.pos = i + 1;
                    return Ok(Cow::Borrowed(&self.s[start..i]));
                }
                Some(b'\\') => break,
                Some(0..=0x1f) => {
                    self.pos = i;
                    return Err(self.error("control character in string"));
                }
                Some(_) => i += 1,
            }
        }
        let mut out = String::from(&self.s[start..i]);
        self.pos = i;
        loop {
            let run = self.pos;
            while let Some(&c) = b.get(self.pos) {
                if c == b'"' || c == b'\\' || c < 0x20 {
                    break;
                }
                self.pos += 1;
            }
            out.push_str(&self.s[run..self.pos]);
            match b.get(self.pos) {
                Some(b'"') => {
                    self.pos += 1;
                    return Ok(Cow::Owned(out));
                }
                Some(b'\\') => {
                    let e = b.get(self.pos + 1).copied();
                    self.pos += 2;
                    let c = match e {
                        Some(b'"') => '"',
                        Some(b'\\') => '\\',
                        Some(b'/') => '/',
                        Some(b'b') => '\u{8}',
                        Some(b'f') => '\u{c}',
                        Some(b'n') => '\n',
                        Some(b'r') => '\r',
                        Some(b't') => '\t',
                        Some(b'u') => self.unicode_escape()?,
                        _ => return Err(self.error("invalid escape")),
                    };
                    out.push(c);
                }
                None => return Err(self.error("EOF while parsing a string")),
                Some(_) => return Err(self.error("control character in string")),
            }
        }
    }

    fn hex4(&mut self) -> Result<u16, Error> {
        let digits = self.s.get(self.pos..self.pos + 4).ok_or_else(|| self.error("EOF while parsing a string"))?;
        // from_str_radix alone would take a leading `+`.
        if !digits.bytes().all(|c| c.is_ascii_hexdigit()) {
            return Err(self.error("invalid escape"));
        }
        let n = u16::from_str_radix(digits, 16).map_err(|_| self.error("invalid escape"))?;
        self.pos += 4;
        Ok(n)
    }

    /// The character of a `\u` escape (its digits at `pos`), with a surrogate pair's second half.
    fn unicode_escape(&mut self) -> Result<char, Error> {
        let hi = self.hex4()?;
        let code = if (0xD800..0xDC00).contains(&hi) {
            if !self.s[self.pos..].starts_with("\\u") {
                return Err(self.error("lone leading surrogate in hex escape"));
            }
            self.pos += 2;
            let lo = self.hex4()?;
            if !(0xDC00..0xE000).contains(&lo) {
                return Err(self.error("lone leading surrogate in hex escape"));
            }
            0x10000 + ((u32::from(hi) - 0xD800) << 10) + (u32::from(lo) - 0xDC00)
        } else {
            u32::from(hi)
        };
        char::from_u32(code).ok_or_else(|| self.error("invalid unicode code point"))
    }

    fn enter(&mut self) -> Result<(), Error> {
        self.depth += 1;
        if self.depth > MAX_DEPTH {
            return Err(self.error("recursion limit exceeded"));
        }
        self.pos += 1;
        Ok(())
    }

    /// Opens an object; read its keys with [`Reader::key`].
    pub fn object(&mut self) -> Result<Object, Error> {
        if self.peek_byte() != Some(b'{') {
            return Err(self.error("expected an object"));
        }
        self.enter()?;
        Ok(Object { first: true })
    }

    /// The object's next key, with the reader at its value; `None` once the object is closed.
    pub fn key(&mut self, o: &mut Object) -> Result<Option<Cow<'a, str>>, Error> {
        match self.peek_byte() {
            Some(b'}') => {
                self.pos += 1;
                self.depth -= 1;
                return Ok(None);
            }
            Some(b',') if !o.first => self.pos += 1,
            Some(b'"') if o.first => {}
            _ => return Err(self.error("expected `,` or `}`")),
        }
        o.first = false;
        let k = self.str()?;
        self.expect_byte(b':', "`:`")?;
        Ok(Some(k))
    }

    /// Opens an array; read its items with [`Reader::item`].
    pub fn array(&mut self) -> Result<Array, Error> {
        if self.peek_byte() != Some(b'[') {
            return Err(self.error("expected an array"));
        }
        self.enter()?;
        Ok(Array { first: true })
    }

    /// Whether the array has another item, with the reader at it.
    pub fn item(&mut self, a: &mut Array) -> Result<bool, Error> {
        match self.peek_byte() {
            Some(b']') => {
                self.pos += 1;
                self.depth -= 1;
                return Ok(false);
            }
            Some(b',') if !a.first => self.pos += 1,
            Some(_) if a.first => {}
            _ => return Err(self.error("expected `,` or `]`")),
        }
        a.first = false;
        Ok(true)
    }

    /// Passes over one value of any kind.
    pub fn skip(&mut self) -> Result<(), Error> {
        match self.peek()? {
            Kind::Null => self.literal("null"),
            Kind::Bool => self.bool().map(drop),
            Kind::Number => self.number().map(drop),
            Kind::String => self.str().map(drop),
            Kind::Array => {
                let mut a = self.array()?;
                while self.item(&mut a)? {
                    self.skip()?;
                }
                Ok(())
            }
            Kind::Object => {
                let mut o = self.object()?;
                while self.key(&mut o)?.is_some() {
                    self.skip()?;
                }
                Ok(())
            }
        }
    }

    /// The string value of the object's `key`, found ahead without moving the reader. Scanning is linear in the
    /// object, and each nested tagged enum rescans only its own object, so input of n bytes costs at most
    /// n × [`MAX_DEPTH`].
    pub fn tag(&self, key: &str) -> Result<Cow<'a, str>, Error> {
        let mut r = self.clone();
        let mut o = r.object()?;
        while let Some(k) = r.key(&mut o)? {
            if k == key {
                return r.str();
            }
            r.skip()?;
        }
        Err(self.error(format!("missing field `{key}`")))
    }

    /// Reads the string `word` if it is next.
    pub fn keyword(&mut self, word: &str) -> Result<bool, Error> {
        if self.peek_byte() != Some(b'"') {
            return Ok(false);
        }
        let save = self.pos;
        if self.str()? == word {
            return Ok(true);
        }
        self.pos = save;
        Ok(false)
    }
}

// Decoding helpers the derives call

/// A required field that wasn't there.
#[cold]
pub fn missing(r: &Reader<'_>, name: &str) -> Error {
    r.error(format!("missing field `{name}`"))
}

/// A field given twice.
#[cold]
pub fn duplicate(r: &Reader<'_>, name: &str) -> Error {
    r.error(format!("duplicate field `{name}`"))
}

/// A field the type doesn't have.
#[cold]
pub fn unknown_field(r: &Reader<'_>, name: &str) -> Error {
    r.error(format!("unknown field `{name}`"))
}

/// A variant the enum doesn't have.
#[cold]
pub fn unknown_variant(r: &Reader<'_>, name: &str) -> Error {
    r.error(format!("unknown variant `{name}`"))
}

/// An untagged enum that no variant matched.
#[cold]
pub fn no_variant(r: &Reader<'_>, ty: &str) -> Error {
    r.error(format!("data did not match any variant of untagged enum {ty}"))
}

/// A unit enum's variant, by name: its index in `names`.
pub fn unit_index(r: &mut Reader<'_>, names: &[&str]) -> Result<usize, Error> {
    let at = r.clone();
    let s = r.str()?;
    names.iter().position(|n| *n == s).ok_or_else(|| unknown_variant(&at, &s))
}

/// A unit variant of an internally tagged enum: an object, whose other keys serde ignores even under
/// `deny_unknown_fields`.
pub fn decode_unit(r: &mut Reader<'_>) -> Result<(), Error> {
    r.skip()
}

/// A variant's single argument in `value` beside its tag (`#[serde(with = "value")]`).
pub fn decode_value<T: Decode>(r: &mut Reader<'_>, tags: Option<&Tags<'_>>) -> Result<T, Error> {
    let mut o = r.object()?;
    let mut v = None;
    while let Some(k) = r.key(&mut o)? {
        if k == "value" {
            if v.is_some() {
                return Err(duplicate(r, "value"));
            }
            v = Some(T::decode(r)?);
        } else if is_tag(tags, &k) {
            r.skip()?;
        } else {
            return Err(unknown_field(r, &k));
        }
    }
    v.or_else(T::absent).ok_or_else(|| missing(r, "value"))
}

// Primitive and container impls

impl Encode for () {
    fn encode(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(b"null");
    }
}

impl Encode for bool {
    fn encode(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(if *self { b"true" } else { b"false" });
    }
}

impl Decode for bool {
    fn decode(r: &mut Reader<'_>) -> Result<bool, Error> {
        r.bool()
    }
}

macro_rules! ints {
    ($($t:ty)*) => {$(
        impl Encode for $t {
            fn encode(&self, out: &mut Vec<u8>) {
                out.extend_from_slice(itoa::Buffer::new().format(*self).as_bytes());
            }
        }

        impl Decode for $t {
            fn decode(r: &mut Reader<'_>) -> Result<$t, Error> {
                r.int()
            }
        }

        impl MapKey for $t {
            fn encode_key(&self, out: &mut Vec<u8>) {
                out.push(b'"');
                self.encode(out);
                out.push(b'"');
            }

            fn decode_key(key: &str) -> Result<$t, Error> {
                key.parse().map_err(|_| Error::new("expected an integer key", 0))
            }
        }
    )*};
}

ints!(u8 u16 u32 u64 usize i8 i16 i32 i64 isize);

impl Encode for f64 {
    fn encode(&self, out: &mut Vec<u8>) {
        if self.is_finite() {
            out.extend_from_slice(ryu::Buffer::new().format_finite(*self).as_bytes());
        } else {
            out.extend_from_slice(b"null");
        }
    }
}

impl Decode for f64 {
    fn decode(r: &mut Reader<'_>) -> Result<f64, Error> {
        r.f64()
    }
}

impl Encode for f32 {
    fn encode(&self, out: &mut Vec<u8>) {
        if self.is_finite() {
            out.extend_from_slice(ryu::Buffer::new().format_finite(*self).as_bytes());
        } else {
            out.extend_from_slice(b"null");
        }
    }
}

impl Decode for f32 {
    fn decode(r: &mut Reader<'_>) -> Result<f32, Error> {
        r.f64().map(|v| v as f32)
    }
}

impl Encode for str {
    fn encode(&self, out: &mut Vec<u8>) {
        write_str(out, self);
    }
}

impl Encode for String {
    fn encode(&self, out: &mut Vec<u8>) {
        write_str(out, self);
    }
}

impl Decode for String {
    fn decode(r: &mut Reader<'_>) -> Result<String, Error> {
        r.str().map(Cow::into_owned)
    }
}

impl MapKey for String {
    fn encode_key(&self, out: &mut Vec<u8>) {
        write_str(out, self);
    }

    fn decode_key(key: &str) -> Result<String, Error> {
        Ok(key.to_owned())
    }
}

impl Encode for char {
    fn encode(&self, out: &mut Vec<u8>) {
        write_str(out, self.encode_utf8(&mut [0; 4]));
    }
}

impl Decode for char {
    fn decode(r: &mut Reader<'_>) -> Result<char, Error> {
        let at = r.clone();
        let s = r.str()?;
        let mut cs = s.chars();
        match (cs.next(), cs.next()) {
            (Some(c), None) => Ok(c),
            _ => Err(at.error("expected a single character")),
        }
    }
}

impl MapKey for char {
    fn encode_key(&self, out: &mut Vec<u8>) {
        self.encode(out);
    }

    fn decode_key(key: &str) -> Result<char, Error> {
        let mut cs = key.chars();
        match (cs.next(), cs.next()) {
            (Some(c), None) => Ok(c),
            _ => Err(Error::new("expected a single character", 0)),
        }
    }
}

impl Encode for Rc<str> {
    fn encode(&self, out: &mut Vec<u8>) {
        write_str(out, self);
    }
}

impl Decode for Rc<str> {
    fn decode(r: &mut Reader<'_>) -> Result<Rc<str>, Error> {
        r.str().map(|s| Rc::from(&*s))
    }
}

impl Encode for Arc<str> {
    fn encode(&self, out: &mut Vec<u8>) {
        write_str(out, self);
    }
}

impl Decode for Arc<str> {
    fn decode(r: &mut Reader<'_>) -> Result<Arc<str>, Error> {
        r.str().map(|s| Arc::from(&*s))
    }
}

impl Encode for Cow<'_, str> {
    fn encode(&self, out: &mut Vec<u8>) {
        write_str(out, self);
    }
}

impl<T: Encode + ?Sized> Encode for &T {
    fn encode(&self, out: &mut Vec<u8>) {
        (**self).encode(out);
    }
}

impl<T: Encode> Encode for Option<T> {
    fn encode(&self, out: &mut Vec<u8>) {
        match self {
            Some(v) => v.encode(out),
            None => out.extend_from_slice(b"null"),
        }
    }
}

impl<T: Decode> Decode for Option<T> {
    fn decode(r: &mut Reader<'_>) -> Result<Option<T>, Error> {
        if r.null()? { Ok(None) } else { T::decode(r).map(Some) }
    }

    fn absent() -> Option<Option<T>> {
        Some(None)
    }
}

impl<T: Encode> Encode for Box<T> {
    fn encode(&self, out: &mut Vec<u8>) {
        (**self).encode(out);
    }
}

impl<T: Decode> Decode for Box<T> {
    fn decode(r: &mut Reader<'_>) -> Result<Box<T>, Error> {
        T::decode(r).map(Box::new)
    }
}

impl<T: EncodeFields> EncodeFields for Box<T> {
    fn encode_fields(&self, out: &mut Vec<u8>, first: &mut bool) {
        (**self).encode_fields(out, first);
    }
}

impl<T: DecodeFields> DecodeFields for Box<T> {
    fn decode_fields(r: &mut Reader<'_>, tags: Option<&Tags<'_>>) -> Result<Box<T>, Error> {
        T::decode_fields(r, tags).map(Box::new)
    }
}

impl<T: Encode> Encode for Rc<T> {
    fn encode(&self, out: &mut Vec<u8>) {
        (**self).encode(out);
    }
}

impl<T: Decode> Decode for Rc<T> {
    fn decode(r: &mut Reader<'_>) -> Result<Rc<T>, Error> {
        T::decode(r).map(Rc::new)
    }
}

fn encode_seq<'v, T: Encode + 'v>(items: impl IntoIterator<Item = &'v T>, out: &mut Vec<u8>) {
    out.push(b'[');
    for (i, v) in items.into_iter().enumerate() {
        if i > 0 {
            out.push(b',');
        }
        v.encode(out);
    }
    out.push(b']');
}

impl<T: Encode> Encode for [T] {
    fn encode(&self, out: &mut Vec<u8>) {
        encode_seq(self, out);
    }
}

impl<T: Encode> Encode for Vec<T> {
    fn encode(&self, out: &mut Vec<u8>) {
        encode_seq(self, out);
    }
}

impl<T: Decode> Decode for Vec<T> {
    fn decode(r: &mut Reader<'_>) -> Result<Vec<T>, Error> {
        let mut a = r.array()?;
        let mut v = Vec::new();
        while r.item(&mut a)? {
            v.push(T::decode(r)?);
        }
        Ok(v)
    }
}

impl<T: Encode, const N: usize> Encode for [T; N] {
    fn encode(&self, out: &mut Vec<u8>) {
        encode_seq(self, out);
    }
}

impl<T: Decode, const N: usize> Decode for [T; N] {
    fn decode(r: &mut Reader<'_>) -> Result<[T; N], Error> {
        let at = r.clone();
        let v = Vec::<T>::decode(r)?;
        v.try_into().map_err(|v: Vec<T>| at.error(format!("expected an array of {N}, got {}", v.len())))
    }
}

impl<T: Encode + Ord> Encode for BTreeSet<T> {
    fn encode(&self, out: &mut Vec<u8>) {
        encode_seq(self, out);
    }
}

impl<T: Decode + Ord> Decode for BTreeSet<T> {
    fn decode(r: &mut Reader<'_>) -> Result<BTreeSet<T>, Error> {
        let mut a = r.array()?;
        let mut v = BTreeSet::new();
        while r.item(&mut a)? {
            v.insert(T::decode(r)?);
        }
        Ok(v)
    }
}

macro_rules! tuples {
    ($(($($n:tt $t:ident),*))*) => {$(
        impl<$($t: Encode),*> Encode for ($($t,)*) {
            fn encode(&self, out: &mut Vec<u8>) {
                out.push(b'[');
                $(
                    if $n > 0 {
                        out.push(b',');
                    }
                    self.$n.encode(out);
                )*
                out.push(b']');
            }
        }

        impl<$($t: Decode),*> Decode for ($($t,)*) {
            fn decode(r: &mut Reader<'_>) -> Result<($($t,)*), Error> {
                let mut a = r.array()?;
                let v = ($(
                    if r.item(&mut a)? { $t::decode(r)? } else { return Err(r.error(concat!("expected an item ", $n))) },
                )*);
                if r.item(&mut a)? {
                    return Err(r.error("trailing items"));
                }
                Ok(v)
            }
        }
    )*};
}

tuples!((0 A, 1 B) (0 A, 1 B, 2 C) (0 A, 1 B, 2 C, 3 D));

fn encode_map<'v, K: MapKey + 'v, V: Encode + 'v>(entries: impl IntoIterator<Item = (&'v K, &'v V)>, out: &mut Vec<u8>) {
    out.push(b'{');
    for (i, (k, v)) in entries.into_iter().enumerate() {
        if i > 0 {
            out.push(b',');
        }
        k.encode_key(out);
        out.push(b':');
        v.encode(out);
    }
    out.push(b'}');
}

impl<K: MapKey + Ord, V: Encode> Encode for BTreeMap<K, V> {
    fn encode(&self, out: &mut Vec<u8>) {
        encode_map(self, out);
    }
}

impl<K: MapKey + Ord, V: Decode> Decode for BTreeMap<K, V> {
    fn decode(r: &mut Reader<'_>) -> Result<BTreeMap<K, V>, Error> {
        let mut o = r.object()?;
        let mut m = BTreeMap::new();
        while let Some(k) = r.key(&mut o)? {
            let k = K::decode_key(&k).map_err(|e| r.error(e.message))?;
            m.insert(k, V::decode(r)?);
        }
        Ok(m)
    }
}

impl<K: MapKey + Eq + Hash, V: Encode, S: BuildHasher> Encode for HashMap<K, V, S> {
    fn encode(&self, out: &mut Vec<u8>) {
        encode_map(self, out);
    }
}

impl<K: MapKey + Eq + Hash, V: Decode, S: BuildHasher + Default> Decode for HashMap<K, V, S> {
    fn decode(r: &mut Reader<'_>) -> Result<HashMap<K, V, S>, Error> {
        let mut o = r.object()?;
        let mut m = HashMap::default();
        while let Some(k) = r.key(&mut o)? {
            let k = K::decode_key(&k).map_err(|e| r.error(e.message))?;
            m.insert(k, V::decode(r)?);
        }
        Ok(m)
    }
}

#[cfg(feature = "serde_json")]
mod json_value {
    use super::*;
    use serde_json::{Map, Number, Value};

    impl Encode for Value {
        fn encode(&self, out: &mut Vec<u8>) {
            match self {
                Value::Null => out.extend_from_slice(b"null"),
                Value::Bool(b) => b.encode(out),
                Value::Number(n) => match (n.as_u64(), n.as_i64(), n.as_f64()) {
                    (Some(u), _, _) => u.encode(out),
                    (None, Some(i), _) => i.encode(out),
                    (None, None, Some(f)) => f.encode(out),
                    _ => out.extend_from_slice(b"null"),
                },
                Value::String(s) => write_str(out, s),
                Value::Array(a) => a.encode(out),
                Value::Object(m) => {
                    out.push(b'{');
                    for (i, (k, v)) in m.iter().enumerate() {
                        if i > 0 {
                            out.push(b',');
                        }
                        write_str(out, k);
                        out.push(b':');
                        v.encode(out);
                    }
                    out.push(b'}');
                }
            }
        }
    }

    impl Decode for Value {
        fn decode(r: &mut Reader<'_>) -> Result<Value, Error> {
            Ok(match r.peek()? {
                Kind::Null => {
                    r.null()?;
                    Value::Null
                }
                Kind::Bool => Value::Bool(r.bool()?),
                Kind::Number => {
                    let at = r.pos;
                    let (text, integer) = r.number()?;
                    let n =
                        if integer { text.parse::<u64>().map(Number::from).or_else(|_| text.parse::<i64>().map(Number::from)).ok() } else { None };
                    match n {
                        Some(n) => Value::Number(n),
                        None => {
                            text.parse::<f64>().ok().and_then(Number::from_f64).map(Value::Number).ok_or_else(|| Error::new("invalid number", at))?
                        }
                    }
                }
                Kind::String => Value::String(r.str()?.into_owned()),
                Kind::Array => Value::Array(Vec::decode(r)?),
                Kind::Object => {
                    let mut o = r.object()?;
                    let mut m = Map::new();
                    while let Some(k) = r.key(&mut o)? {
                        m.insert(k.into_owned(), Value::decode(r)?);
                    }
                    Value::Object(m)
                }
            })
        }
    }
}

#[cfg(test)]
mod tests;
