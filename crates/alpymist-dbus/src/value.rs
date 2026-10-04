//! What a D-Bus message carries.
//!
//! One type for every value, each knowing its own signature. The interfaces
//! spoken to here are a handful, and what is read from them is picked out by
//! name and kind as it is used, so a value is looked into with the `as_*`
//! methods and not turned into a Rust type of its own.

/// A D-Bus value.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// `y`
    U8(u8),
    /// `b`
    Bool(bool),
    /// `n`
    I16(i16),
    /// `q`
    U16(u16),
    /// `i`
    I32(i32),
    /// `u`
    U32(u32),
    /// `x`
    I64(i64),
    /// `t`
    U64(u64),
    /// `d`
    F64(f64),
    /// `s`
    Str(String),
    /// `o`: an object's path.
    Path(String),
    /// `g`: a signature.
    Signature(String),
    /// `a…`: what each item is, as a signature, and the items. A dictionary
    /// is an array of [`Value::Entry`].
    Array(String, Vec<Value>),
    /// `(…)`
    Struct(Vec<Value>),
    /// `{…}`: a key and what it has, inside an array.
    Entry(Box<Value>, Box<Value>),
    /// `v`: a value that says itself what it is.
    Variant(Box<Value>),
}

impl Value {
    /// An object's path.
    #[must_use]
    pub fn path(path: &str) -> Self {
        Self::Path(path.to_owned())
    }

    /// `value` as a variant.
    #[must_use]
    pub fn variant(value: Self) -> Self {
        Self::Variant(Box::new(value))
    }

    /// An array of strings.
    #[must_use]
    pub fn strings<'a>(items: impl IntoIterator<Item = &'a str>) -> Self {
        Self::Array("s".into(), items.into_iter().map(Self::from).collect())
    }

    /// `a{sv}`: named values of any kind, as properties and options are
    /// passed.
    #[must_use]
    pub fn named<'a>(entries: impl IntoIterator<Item = (&'a str, Self)>) -> Self {
        let entries = entries
            .into_iter()
            .map(|(name, value)| Self::Entry(Box::new(name.into()), Box::new(Self::variant(value))))
            .collect();
        Self::Array("{sv}".into(), entries)
    }

    /// The signature of this value.
    #[must_use]
    pub fn signature(&self) -> String {
        match self {
            Self::U8(_) => "y".into(),
            Self::Bool(_) => "b".into(),
            Self::I16(_) => "n".into(),
            Self::U16(_) => "q".into(),
            Self::I32(_) => "i".into(),
            Self::U32(_) => "u".into(),
            Self::I64(_) => "x".into(),
            Self::U64(_) => "t".into(),
            Self::F64(_) => "d".into(),
            Self::Str(_) => "s".into(),
            Self::Path(_) => "o".into(),
            Self::Signature(_) => "g".into(),
            Self::Array(of, _) => format!("a{of}"),
            Self::Struct(fields) => {
                let inner: String = fields.iter().map(Self::signature).collect();
                format!("({inner})")
            }
            Self::Entry(key, value) => format!("{{{}{}}}", key.signature(), value.signature()),
            Self::Variant(_) => "v".into(),
        }
    }

    /// The value itself, out of however many variants it is wrapped in.
    #[must_use]
    pub fn plain(&self) -> &Self {
        let mut value = self;
        while let Self::Variant(inner) = value {
            value = inner;
        }
        value
    }

    /// The text of a string or an object's path.
    #[must_use]
    pub fn as_str(&self) -> Option<&str> {
        match self.plain() {
            Self::Str(text) | Self::Path(text) => Some(text),
            _ => None,
        }
    }

    /// An object's path, and not a string that happens to look like one.
    #[must_use]
    pub fn as_path(&self) -> Option<&str> {
        match self.plain() {
            Self::Path(path) => Some(path),
            _ => None,
        }
    }

    /// A boolean.
    #[must_use]
    pub fn as_bool(&self) -> Option<bool> {
        match self.plain() {
            Self::Bool(b) => Some(*b),
            _ => None,
        }
    }

    /// A whole number of any width, where it fits.
    #[must_use]
    pub fn as_i64(&self) -> Option<i64> {
        Some(match self.plain() {
            Self::U8(n) => (*n).into(),
            Self::I16(n) => (*n).into(),
            Self::U16(n) => (*n).into(),
            Self::I32(n) => (*n).into(),
            Self::U32(n) => (*n).into(),
            Self::I64(n) => *n,
            Self::U64(n) => i64::try_from(*n).ok()?,
            _ => return None,
        })
    }

    /// A whole number of any width, where it fits in a `u32`.
    #[must_use]
    pub fn as_u32(&self) -> Option<u32> {
        self.as_i64().and_then(|n| n.try_into().ok())
    }

    /// The items of an array, or the fields of a struct.
    #[must_use]
    pub fn items(&self) -> &[Self] {
        match self.plain() {
            Self::Array(_, items) | Self::Struct(items) => items,
            _ => &[],
        }
    }

    /// The entries of a dictionary, each a key and what it has.
    pub fn entries(&self) -> impl Iterator<Item = (&Self, &Self)> {
        self.items().iter().filter_map(|item| match item {
            Self::Entry(key, value) => Some((&**key, &**value)),
            _ => None,
        })
    }

    /// What a dictionary keyed by strings has under `key`.
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&Self> {
        self.entries()
            .find_map(|(k, v)| (k.as_str() == Some(key)).then_some(v))
    }
}

impl From<&str> for Value {
    fn from(text: &str) -> Self {
        Self::Str(text.to_owned())
    }
}

impl From<String> for Value {
    fn from(text: String) -> Self {
        Self::Str(text)
    }
}

impl From<bool> for Value {
    fn from(b: bool) -> Self {
        Self::Bool(b)
    }
}

impl From<u32> for Value {
    fn from(n: u32) -> Self {
        Self::U32(n)
    }
}

impl From<i32> for Value {
    fn from(n: i32) -> Self {
        Self::I32(n)
    }
}

impl From<u64> for Value {
    fn from(n: u64) -> Self {
        Self::U64(n)
    }
}
