//! The wire schema: every type's JSON shape, with its documentation, for the SDKs' generators and checks.
//!
//! The derives describe each type ([`Describe`]) when the `schema` feature is on; a [`Registry`] collects the named
//! definitions a set of root types reaches, and encodes as JSON with the same derives.

use std::collections::BTreeMap;

use crate::Encode;

/// A JSON value's type, naming the definitions it refers to.
#[derive(Clone, Debug, PartialEq, Encode)]
#[wire(tag = "kind", rename_all = "camelCase")]
pub enum Ty {
    /// `true` or `false`.
    Boolean,
    /// An integer in `min..=max`.
    Integer {
        /// The least value.
        min: i128,
        /// The greatest value.
        max: i128,
    },
    /// A number.
    Number,
    /// A string.
    String,
    /// A string in a format: `color` (`#RRGGBB`), `base64`, `date-time` (ISO 8601) or `char` (one character).
    Format {
        /// The format's name.
        format: &'static str,
    },
    /// This string exactly.
    Literal {
        /// The string.
        value: &'static str,
    },
    /// `null`.
    Null,
    /// The value or `null`.
    Nullable {
        /// The value's type.
        of: Box<Ty>,
    },
    /// An array.
    Array {
        /// The items' type.
        of: Box<Ty>,
        /// The array's length, when it is fixed.
        #[wire(skip_serializing_if = "Option::is_none")]
        len: Option<usize>,
    },
    /// An array of these types in order.
    Tuple {
        /// The items' types.
        items: Vec<Ty>,
    },
    /// An object of keys to values.
    Map {
        /// The keys' type (a string type).
        key: Box<Ty>,
        /// The values' type.
        value: Box<Ty>,
    },
    /// The first of these that the value reads as.
    OneOf {
        /// The types, in the order they're tried.
        of: Vec<Ty>,
    },
    /// Any JSON value.
    Json,
    /// A named definition.
    Ref {
        /// The definition's name.
        name: String,
    },
}

/// A named type's definition.
#[derive(Clone, Debug, PartialEq, Encode)]
#[wire(rename_all = "camelCase")]
pub struct Def {
    /// The Rust type's name.
    pub name: String,
    /// The Rust module it is defined in.
    pub module: &'static str,
    /// Its documentation.
    #[wire(skip_serializing_if = "str::is_empty")]
    pub docs: String,
    /// Its JSON.
    pub shape: Shape,
}

/// How a named type is written.
#[derive(Clone, Debug, PartialEq, Encode)]
#[wire(tag = "kind", rename_all = "camelCase")]
pub enum Shape {
    /// One of these strings.
    Units {
        /// The variants.
        variants: Vec<Unit>,
    },
    /// An object of these fields.
    Object {
        /// The fields.
        fields: Vec<Field>,
        /// Whether a key it doesn't have is refused.
        closed: bool,
    },
    /// An object with its variant's name in `tag`.
    Tagged {
        /// The tag key.
        tag: &'static str,
        /// The variants.
        variants: Vec<Variant>,
    },
    /// The first variant the value reads as.
    Untagged {
        /// The variants, in the order they're tried.
        variants: Vec<Variant>,
    },
    /// A unit variant as its name; any other as `{ name: value }`.
    External {
        /// The variants.
        variants: Vec<Variant>,
    },
    /// Another type's JSON.
    Alias {
        /// The type.
        of: Ty,
    },
}

/// A unit variant.
#[derive(Clone, Debug, PartialEq, Encode)]
pub struct Unit {
    /// Its wire name.
    pub name: String,
    /// Its documentation.
    #[wire(skip_serializing_if = "str::is_empty")]
    pub docs: String,
}

/// An object's field.
#[derive(Clone, Debug, PartialEq, Encode)]
pub struct Field {
    /// Its key.
    pub name: String,
    /// Its documentation.
    #[wire(skip_serializing_if = "str::is_empty")]
    pub docs: String,
    /// Its value's type.
    pub ty: Ty,
    /// Whether the engine may leave it out (`skip_serializing_if`).
    pub omitted: bool,
    /// Whether it may be left out of what the engine reads (a default, or an `Option`'s `None`).
    pub defaulted: bool,
}

/// A data enum's variant.
#[derive(Clone, Debug, PartialEq, Encode)]
pub struct Variant {
    /// Its wire name (unused for an untagged variant).
    pub name: String,
    /// Its documentation.
    #[wire(skip_serializing_if = "str::is_empty")]
    pub docs: String,
    /// What it carries.
    pub payload: Payload,
}

/// What a variant carries.
#[derive(Clone, Debug, PartialEq, Encode)]
#[wire(tag = "kind", rename_all = "camelCase")]
pub enum Payload {
    /// Nothing (in a tagged enum, the tag alone; untagged, `null`).
    Unit,
    /// One value: in `value` beside a tag, under the name of an external variant, or alone untagged.
    Value {
        /// Its type.
        ty: Ty,
    },
    /// An object type's fields, beside the tag.
    Spread {
        /// The object type.
        ty: Ty,
    },
    /// These fields (beside the tag, when tagged).
    Fields {
        /// The fields.
        fields: Vec<Field>,
    },
}

/// The named definitions reached from the types described into it.
#[derive(Default)]
pub struct Registry {
    defs: BTreeMap<(&'static str, String), Option<Def>>,
}

impl Registry {
    /// A reference to the type `name` in `module`, defined by `define` the first time it is reached.
    pub fn named(&mut self, module: &'static str, name: &str, define: impl FnOnce(&mut Registry) -> Def) -> Ty {
        let key = (module, name.to_owned());
        if !self.defs.contains_key(&key) {
            // A placeholder first: a type that refers to itself finds it and stops.
            self.defs.insert(key.clone(), None);
            let def = define(self);
            self.defs.insert(key, Some(def));
        }
        Ty::Ref { name: name.to_owned() }
    }

    /// Describes `T` and everything it reaches.
    pub fn add<T: Describe + ?Sized>(&mut self) -> Ty {
        T::describe(self)
    }

    /// The definitions, by name; names that two modules define are errors.
    pub fn finish(self) -> Result<Schema, String> {
        let mut defs: BTreeMap<String, Def> = BTreeMap::new();
        for ((module, name), def) in self.defs {
            let def = def.ok_or_else(|| format!("{module}::{name} was never defined"))?;
            if let Some(other) = defs.get(&name) {
                return Err(format!("two wire types are named {name}: in {} and {module}", other.module));
            }
            defs.insert(name, def);
        }
        Ok(Schema { types: defs.into_values().collect() })
    }
}

/// Every named wire type, by name.
#[derive(Encode)]
pub struct Schema {
    /// The definitions, sorted by name.
    pub types: Vec<Def>,
}

/// A type that can say what its JSON is.
pub trait Describe {
    /// This type's JSON, registering the definitions it names.
    fn describe(reg: &mut Registry) -> Ty;
}

/// Joins a definition's `///` lines into one text.
pub fn docs(lines: &[&str]) -> String {
    let mut out = String::new();
    for l in lines {
        let l = l.strip_prefix(' ').unwrap_or(l);
        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str(l);
    }
    out.trim().to_owned()
}

macro_rules! leaf {
    ($ty:expr => $($t:ty),*) => {$(
        impl Describe for $t {
            fn describe(_: &mut Registry) -> Ty {
                $ty
            }
        }
    )*};
}

leaf!(Ty::Boolean => bool);
leaf!(Ty::Number => f32, f64);
leaf!(Ty::String => str, String, std::borrow::Cow<'_, str>);
leaf!(Ty::Format { format: "char" } => char);
leaf!(Ty::Null => ());
leaf!(Ty::Json => crate::RawJson);

macro_rules! ints {
    ($($t:ty)*) => {$(
        impl Describe for $t {
            fn describe(_: &mut Registry) -> Ty {
                Ty::Integer { min: <$t>::MIN as i128, max: <$t>::MAX as i128 }
            }
        }
    )*};
}

ints!(u8 u16 u32 u64 usize i8 i16 i32 i64 isize i128);

macro_rules! wrapper {
    ($($t:ty),*) => {$(
        impl<T: Describe + ?Sized> Describe for $t {
            fn describe(reg: &mut Registry) -> Ty {
                T::describe(reg)
            }
        }
    )*};
}

wrapper!(&T, Box<T>, std::rc::Rc<T>, std::sync::Arc<T>);

impl<T: Describe> Describe for Option<T> {
    fn describe(reg: &mut Registry) -> Ty {
        // `Option<Option<T>>` stays two deep: a field left out for the outer `None` is `null` for the inner one.
        Ty::Nullable { of: Box::new(T::describe(reg)) }
    }
}

impl<T: Describe> Describe for [T] {
    fn describe(reg: &mut Registry) -> Ty {
        Ty::Array { of: Box::new(T::describe(reg)), len: None }
    }
}

impl<T: Describe> Describe for Vec<T> {
    fn describe(reg: &mut Registry) -> Ty {
        Ty::Array { of: Box::new(T::describe(reg)), len: None }
    }
}

impl<T: Describe> Describe for std::collections::BTreeSet<T> {
    fn describe(reg: &mut Registry) -> Ty {
        Ty::Array { of: Box::new(T::describe(reg)), len: None }
    }
}

impl<T: Describe, const N: usize> Describe for [T; N] {
    fn describe(reg: &mut Registry) -> Ty {
        Ty::Array { of: Box::new(T::describe(reg)), len: Some(N) }
    }
}

impl<K: Describe, V: Describe> Describe for BTreeMap<K, V> {
    fn describe(reg: &mut Registry) -> Ty {
        Ty::Map { key: Box::new(K::describe(reg)), value: Box::new(V::describe(reg)) }
    }
}

impl<K: Describe, V: Describe, S> Describe for std::collections::HashMap<K, V, S> {
    fn describe(reg: &mut Registry) -> Ty {
        Ty::Map { key: Box::new(K::describe(reg)), value: Box::new(V::describe(reg)) }
    }
}

macro_rules! tuples {
    ($(($($t:ident),*))*) => {$(
        impl<$($t: Describe),*> Describe for ($($t,)*) {
            fn describe(reg: &mut Registry) -> Ty {
                Ty::Tuple { items: vec![$($t::describe(reg)),*] }
            }
        }
    )*};
}

tuples!((A, B)(A, B, C)(A, B, C, D));
