//! `#[derive(Encode, Decode)]` for twip-wire, reading `#[serde(...)]` and `#[wire(...)]` attributes.
//!
//! Supported, as serde means them: on a container `rename_all` (camelCase, kebab-case, snake_case), `tag`,
//! `untagged`, `transparent`, `deny_unknown_fields` and `default`; on a variant `rename`, `rename_all` and `skip`; on
//! a field `rename`, `default`, `default = "path"`, `skip`, `skip_serializing_if`, and `with`/`deserialize_with` naming
//! one of the wire's helpers (`value`, `nullable`, `present`). `#[wire(encode_with, decode_with)]` names a field's own
//! functions. Anything else is a compile error.

use proc_macro2::{Span, TokenStream};
use quote::{format_ident, quote};
use syn::ext::IdentExt;
use syn::{Data, DeriveInput, Fields, Ident, LitByteStr, LitStr, Path, Type, parse_macro_input};

/// Derives `twip_wire::Encode` (and `EncodeFields` for an object, `MapKey` for a unit enum).
#[proc_macro_derive(Encode, attributes(wire))]
pub fn derive_encode(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    expand(&input, true).unwrap_or_else(syn::Error::into_compile_error).into()
}

/// Derives `twip_wire::Decode` (and `DecodeFields` for an object).
#[proc_macro_derive(Decode, attributes(wire))]
pub fn derive_decode(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    expand(&input, false).unwrap_or_else(syn::Error::into_compile_error).into()
}

// Attributes

#[derive(Clone, Copy, PartialEq)]
enum Case {
    None,
    Camel,
    Kebab,
    Snake,
}

#[derive(Default)]
struct Container {
    rename_all: Option<Case>,
    tag: Option<String>,
    untagged: bool,
    transparent: bool,
    deny: bool,
    default: bool,
}

#[derive(Default)]
struct VariantAttrs {
    rename: Option<String>,
    rename_all: Option<Case>,
    skip: bool,
}

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Normal,
    /// `{ "value": v }` beside the tag.
    Value,
    /// `Option<Option<T>>`: absent, `null` or a value.
    Nullable,
    /// `Option<T>`: present means `Some`, even for `null` (which `T` then reads).
    Present,
}

enum FieldDefault {
    None,
    Default,
    Path(Path),
}

struct FieldAttrs {
    rename: Option<String>,
    default: FieldDefault,
    skip: bool,
    skip_if: Option<Path>,
    mode: Mode,
    encode_with: Option<Path>,
    decode_with: Option<Path>,
    /// A serde `serialize_with`/`deserialize_with` with no wire equivalent: only allowed beside `#[wire(...)]`'s.
    foreign_with: Option<Span>,
}

fn case(lit: &LitStr) -> syn::Result<Case> {
    match lit.value().as_str() {
        "camelCase" => Ok(Case::Camel),
        "kebab-case" => Ok(Case::Kebab),
        "snake_case" => Ok(Case::Snake),
        _ => Err(syn::Error::new(lit.span(), "twip-wire: unsupported rename_all")),
    }
}

fn is_ours(attr: &syn::Attribute) -> bool {
    attr.path().is_ident("serde") || attr.path().is_ident("wire")
}

fn container_attrs(attrs: &[syn::Attribute]) -> syn::Result<Container> {
    let mut c = Container::default();
    for attr in attrs.iter().filter(|a| is_ours(a)) {
        attr.parse_nested_meta(|m| {
            if m.path.is_ident("rename_all") {
                c.rename_all = Some(case(&m.value()?.parse()?)?);
            } else if m.path.is_ident("tag") {
                c.tag = Some(m.value()?.parse::<LitStr>()?.value());
            } else if m.path.is_ident("untagged") {
                c.untagged = true;
            } else if m.path.is_ident("transparent") {
                c.transparent = true;
            } else if m.path.is_ident("deny_unknown_fields") {
                c.deny = true;
            } else if m.path.is_ident("default") {
                c.default = true;
            } else {
                return Err(m.error("twip-wire: unsupported container attribute"));
            }
            Ok(())
        })?;
    }
    Ok(c)
}

fn variant_attrs(attrs: &[syn::Attribute]) -> syn::Result<VariantAttrs> {
    let mut v = VariantAttrs::default();
    for attr in attrs.iter().filter(|a| is_ours(a)) {
        attr.parse_nested_meta(|m| {
            if m.path.is_ident("rename") {
                v.rename = Some(m.value()?.parse::<LitStr>()?.value());
            } else if m.path.is_ident("rename_all") {
                v.rename_all = Some(case(&m.value()?.parse()?)?);
            } else if m.path.is_ident("skip") {
                v.skip = true;
            } else {
                return Err(m.error("twip-wire: unsupported variant attribute"));
            }
            Ok(())
        })?;
    }
    Ok(v)
}

/// The wire helper a serde `with`/`deserialize_with` path names, by its last segment.
fn helper(lit: &LitStr) -> Option<Mode> {
    match lit.value().rsplit("::").next() {
        Some("value") => Some(Mode::Value),
        Some("nullable") => Some(Mode::Nullable),
        Some("present") => Some(Mode::Present),
        _ => None,
    }
}

fn field_attrs(attrs: &[syn::Attribute]) -> syn::Result<FieldAttrs> {
    let mut f = FieldAttrs {
        rename: None,
        default: FieldDefault::None,
        skip: false,
        skip_if: None,
        mode: Mode::Normal,
        encode_with: None,
        decode_with: None,
        foreign_with: None,
    };
    for attr in attrs.iter().filter(|a| is_ours(a)) {
        attr.parse_nested_meta(|m| {
            if m.path.is_ident("rename") {
                f.rename = Some(m.value()?.parse::<LitStr>()?.value());
            } else if m.path.is_ident("default") {
                f.default =
                    if m.input.peek(syn::Token![=]) { FieldDefault::Path(m.value()?.parse::<LitStr>()?.parse()?) } else { FieldDefault::Default };
            } else if m.path.is_ident("skip") {
                f.skip = true;
            } else if m.path.is_ident("skip_serializing_if") {
                f.skip_if = Some(m.value()?.parse::<LitStr>()?.parse()?);
            } else if m.path.is_ident("with") || m.path.is_ident("deserialize_with") || m.path.is_ident("serialize_with") {
                let lit: LitStr = m.value()?.parse()?;
                match helper(&lit) {
                    Some(mode) => f.mode = mode,
                    None => f.foreign_with = Some(lit.span()),
                }
            } else if m.path.is_ident("encode_with") {
                f.encode_with = Some(m.value()?.parse::<LitStr>()?.parse()?);
            } else if m.path.is_ident("decode_with") {
                f.decode_with = Some(m.value()?.parse::<LitStr>()?.parse()?);
            } else {
                return Err(m.error("twip-wire: unsupported field attribute"));
            }
            Ok(())
        })?;
    }
    if let Some(span) = f.foreign_with
        && (f.encode_with.is_none() || f.decode_with.is_none())
    {
        return Err(syn::Error::new(span, "twip-wire: a custom serde `with` needs #[wire(encode_with, decode_with)]"));
    }
    Ok(f)
}

// Names

fn rename_variant(name: &str, case: Case) -> String {
    let sep = match case {
        Case::None => return name.to_owned(),
        Case::Camel => return name[..1].to_ascii_lowercase() + &name[1..],
        Case::Kebab => '-',
        Case::Snake => '_',
    };
    let mut out = String::new();
    for (i, c) in name.char_indices() {
        if c.is_uppercase() && i > 0 {
            out.push(sep);
        }
        out.push(c.to_ascii_lowercase());
    }
    out
}

fn rename_field(name: &str, case: Case) -> String {
    match case {
        Case::None | Case::Snake => name.to_owned(),
        Case::Kebab => name.replace('_', "-"),
        Case::Camel => {
            let mut out = String::new();
            let mut upper = false;
            for c in name.chars() {
                if c == '_' {
                    upper = true;
                } else if upper {
                    out.push(c.to_ascii_uppercase());
                    upper = false;
                } else {
                    out.push(c);
                }
            }
            out
        }
    }
}

/// `"name":` as a byte string literal.
fn key_bytes(name: &str) -> LitByteStr {
    LitByteStr::new(format!("\"{name}\":").as_bytes(), Span::call_site())
}

// Fields

struct Field<'a> {
    ident: &'a Ident,
    ty: &'a Type,
    name: String,
    attrs: FieldAttrs,
}

fn named_fields(fields: &syn::FieldsNamed, case: Case) -> syn::Result<Vec<Field<'_>>> {
    fields
        .named
        .iter()
        .map(|f| {
            let ident = f.ident.as_ref().expect("named");
            let attrs = field_attrs(&f.attrs)?;
            let name = attrs.rename.clone().unwrap_or_else(|| rename_field(&ident.unraw().to_string(), case));
            Ok(Field { ident, ty: &f.ty, name, attrs })
        })
        .collect()
}

/// Statements writing `fields` with `out` and `first` in scope; `access(field)` is a reference to its value.
fn encode_fields(fields: &[Field<'_>], access: impl Fn(&Field<'_>) -> TokenStream) -> TokenStream {
    let stmts = fields.iter().filter(|f| !f.attrs.skip).map(|f| {
        let key = key_bytes(&f.name);
        let v = access(f);
        let write = match (&f.attrs.encode_with, f.attrs.mode) {
            (Some(p), _) => quote!(#p(#v, out)),
            (None, Mode::Nullable) => quote!(::twip_wire::encode_nullable(#v, out)),
            (None, _) => quote!(::twip_wire::Encode::encode(#v, out)),
        };
        let stmt = quote! {
            ::twip_wire::field(out, first, #key);
            #write;
        };
        match &f.attrs.skip_if {
            Some(p) => quote!(if !#p(#v) { #stmt }),
            None => stmt,
        }
    });
    quote!(#(#stmts)*)
}

/// An expression reading one object into `build { fields }`, with `r` and `tags` in scope.
fn decode_object(fields: &[Field<'_>], build: TokenStream, deny: bool, container_default: bool) -> TokenStream {
    let live: Vec<&Field<'_>> = fields.iter().filter(|f| !f.attrs.skip).collect();
    let slots: Vec<Ident> = live.iter().map(|f| format_ident!("__{}", f.ident.unraw())).collect();
    let tys = live.iter().map(|f| f.ty);
    let arms = live.iter().zip(&slots).map(|(f, slot)| {
        let name = &f.name;
        let read = match (&f.attrs.decode_with, f.attrs.mode) {
            (Some(p), _) => quote!(#p(r)?),
            (None, Mode::Nullable | Mode::Present) => quote!(::core::option::Option::Some(::twip_wire::Decode::decode(r)?)),
            (None, _) => quote!(::twip_wire::Decode::decode(r)?),
        };
        quote! {
            #name => {
                if #slot.is_some() {
                    return ::core::result::Result::Err(::twip_wire::duplicate(r, #name));
                }
                #slot = ::core::option::Option::Some(#read);
            }
        }
    });
    let unknown = if deny { quote!(return ::core::result::Result::Err(::twip_wire::unknown_field(r, k))) } else { quote!(r.skip()?) };
    let finish = fields.iter().map(|f| {
        let ident = f.ident;
        if f.attrs.skip {
            return quote!(#ident: ::core::default::Default::default());
        }
        let slot = format_ident!("__{}", f.ident.unraw());
        let name = &f.name;
        let fallback = match &f.attrs.default {
            FieldDefault::Path(p) => quote!(#p()),
            FieldDefault::Default => quote!(::core::default::Default::default()),
            FieldDefault::None if container_default => quote!(__default.#ident),
            FieldDefault::None => {
                let ty = f.ty;
                quote!(match <#ty as ::twip_wire::Decode>::absent() {
                    ::core::option::Option::Some(v) => v,
                    ::core::option::Option::None => return ::core::result::Result::Err(::twip_wire::missing(r, #name)),
                })
            }
        };
        quote!(#ident: match #slot { ::core::option::Option::Some(v) => v, ::core::option::Option::None => #fallback })
    });
    let default = container_default.then(|| quote!(let __default: Self = ::core::default::Default::default();));
    quote! {{
        let mut o = r.object()?;
        #(let mut #slots: ::core::option::Option<#tys> = ::core::option::Option::None;)*
        while let ::core::option::Option::Some(k) = r.key(&mut o)? {
            match &*k {
                #(#arms)*
                k => {
                    if ::twip_wire::is_tag(tags, k) {
                        r.skip()?;
                    } else {
                        #unknown;
                    }
                }
            }
        }
        #default
        ::core::result::Result::Ok(#build { #(#finish,)* })
    }}
}

// Expansion

fn expand(input: &DeriveInput, encode: bool) -> syn::Result<TokenStream> {
    let c = container_attrs(&input.attrs)?;
    let name = &input.ident;
    let mut generics = input.generics.clone();
    for p in generics.type_params_mut() {
        p.bounds.push(if encode { syn::parse_quote!(::twip_wire::Encode) } else { syn::parse_quote!(::twip_wire::Decode) });
    }
    let (imp, ty, wh) = generics.split_for_impl();
    let case = c.rename_all.unwrap_or(Case::None);
    match &input.data {
        Data::Struct(s) => match &s.fields {
            Fields::Named(named) if !c.transparent => {
                let fields = named_fields(named, case)?;
                Ok(if encode {
                    let body = encode_fields(&fields, |f| {
                        let i = f.ident;
                        quote!(&self.#i)
                    });
                    quote! {
                        impl #imp ::twip_wire::EncodeFields for #name #ty #wh {
                            fn encode_fields(&self, out: &mut ::std::vec::Vec<u8>, first: &mut bool) {
                                #body
                            }
                        }
                        impl #imp ::twip_wire::Encode for #name #ty #wh {
                            fn encode(&self, out: &mut ::std::vec::Vec<u8>) {
                                out.push(b'{');
                                ::twip_wire::EncodeFields::encode_fields(self, out, &mut true);
                                out.push(b'}');
                            }
                        }
                    }
                } else {
                    let body = decode_object(&fields, quote!(Self), c.deny, c.default);
                    quote! {
                        impl #imp ::twip_wire::DecodeFields for #name #ty #wh {
                            fn decode_fields(r: &mut ::twip_wire::Reader<'_>, tags: ::core::option::Option<&::twip_wire::Tags<'_>>) -> ::core::result::Result<Self, ::twip_wire::Error> {
                                #body
                            }
                        }
                        impl #imp ::twip_wire::Decode for #name #ty #wh {
                            fn decode(r: &mut ::twip_wire::Reader<'_>) -> ::core::result::Result<Self, ::twip_wire::Error> {
                                <Self as ::twip_wire::DecodeFields>::decode_fields(r, ::core::option::Option::None)
                            }
                        }
                    }
                })
            }
            Fields::Named(named) if named.named.len() == 1 => {
                let i = named.named[0].ident.as_ref().expect("named");
                Ok(newtype(name, &generics, quote!(self.#i), quote!(|v| Self { #i: v }), encode))
            }
            Fields::Unnamed(u) if u.unnamed.len() == 1 => Ok(newtype(name, &generics, quote!(self.0), quote!(Self), encode)),
            _ => Err(syn::Error::new_spanned(name, "twip-wire: unsupported struct shape")),
        },
        Data::Enum(e) => {
            let variants: Vec<(&syn::Variant, VariantAttrs)> =
                e.variants.iter().map(|v| Ok((v, variant_attrs(&v.attrs)?))).collect::<syn::Result<_>>()?;
            let wire_name = |v: &syn::Variant, a: &VariantAttrs| a.rename.clone().unwrap_or_else(|| rename_variant(&v.ident.to_string(), case));
            if let Some(tag) = &c.tag {
                tagged(name, &generics, tag, &variants, &wire_name, c.deny, encode)
            } else if c.untagged {
                untagged(name, &generics, &variants, encode)
            } else if variants.iter().all(|(v, a)| a.skip || matches!(v.fields, Fields::Unit)) {
                Ok(unit_enum(name, &variants, &wire_name, encode))
            } else {
                external(name, &generics, &variants, &wire_name, encode)
            }
        }
        Data::Union(_) => Err(syn::Error::new_spanned(name, "twip-wire: unions aren't supported")),
    }
}

fn newtype(name: &Ident, generics: &syn::Generics, inner: TokenStream, build: TokenStream, encode: bool) -> TokenStream {
    let (imp, ty, wh) = generics.split_for_impl();
    if encode {
        quote! {
            impl #imp ::twip_wire::Encode for #name #ty #wh {
                fn encode(&self, out: &mut ::std::vec::Vec<u8>) {
                    ::twip_wire::Encode::encode(&#inner, out)
                }
            }
        }
    } else {
        quote! {
            impl #imp ::twip_wire::Decode for #name #ty #wh {
                fn decode(r: &mut ::twip_wire::Reader<'_>) -> ::core::result::Result<Self, ::twip_wire::Error> {
                    ::twip_wire::Decode::decode(r).map(#build)
                }
            }
        }
    }
}

type WireName<'f> = dyn Fn(&syn::Variant, &VariantAttrs) -> String + 'f;

fn unit_enum(name: &Ident, variants: &[(&syn::Variant, VariantAttrs)], wire_name: &WireName<'_>, encode: bool) -> TokenStream {
    let live: Vec<_> = variants.iter().filter(|(_, a)| !a.skip).collect();
    let names: Vec<String> = live.iter().map(|(v, a)| wire_name(v, a)).collect();
    let idents: Vec<&Ident> = live.iter().map(|(v, _)| &v.ident).collect();
    if encode {
        let skipped: Vec<&Ident> = variants.iter().filter(|(_, a)| a.skip).map(|(v, _)| &v.ident).collect();
        let n = idents.len();
        let index = (0..n).map(|i| if i + 1 == n { quote!(_) } else { quote!(#i) });
        quote! {
            impl ::twip_wire::Named for #name {
                fn name(&self) -> &'static str {
                    match self {
                        #(Self::#idents => #names,)*
                        // serde refuses to write a skipped variant; there's no error path here, so it has no name.
                        #(Self::#skipped { .. } => "",)*
                    }
                }
                fn from_name(name: &str) -> ::core::option::Option<Self> {
                    const NAMES: &[&str] = &[#(#names),*];
                    ::core::option::Option::Some(match NAMES.iter().position(|n| *n == name)? {
                        #(#index => Self::#idents,)*
                    })
                }
            }
            impl ::twip_wire::Encode for #name {
                fn encode(&self, out: &mut ::std::vec::Vec<u8>) {
                    #(if let Self::#skipped { .. } = self {
                        return out.extend_from_slice(b"null");
                    })*
                    out.push(b'"');
                    out.extend_from_slice(::twip_wire::Named::name(self).as_bytes());
                    out.push(b'"');
                }
            }
            impl ::twip_wire::MapKey for #name {
                fn encode_key(&self, out: &mut ::std::vec::Vec<u8>) {
                    ::twip_wire::Encode::encode(self, out)
                }
                fn decode_key(key: &str) -> ::core::result::Result<Self, ::twip_wire::Error> {
                    <Self as ::twip_wire::Named>::from_name(key).ok_or_else(|| ::twip_wire::Error::new(::std::format!("unknown variant `{key}`"), 0))
                }
            }
        }
    } else {
        let n = idents.len();
        let index = (0..n).map(|i| if i + 1 == n { quote!(_) } else { quote!(#i) });
        quote! {
            impl ::twip_wire::Decode for #name {
                fn decode(r: &mut ::twip_wire::Reader<'_>) -> ::core::result::Result<Self, ::twip_wire::Error> {
                    const NAMES: &[&str] = &[#(#names),*];
                    ::core::result::Result::Ok(match ::twip_wire::unit_index(r, NAMES)? {
                        #(#index => Self::#idents,)*
                    })
                }
            }
        }
    }
}

fn tagged(
    name: &Ident,
    generics: &syn::Generics,
    tag: &str,
    variants: &[(&syn::Variant, VariantAttrs)],
    wire_name: &WireName<'_>,
    deny: bool,
    encode: bool,
) -> syn::Result<TokenStream> {
    let (imp, ty, wh) = generics.split_for_impl();
    let mut arms = Vec::new();
    for (v, a) in variants {
        let ident = &v.ident;
        let vname = wire_name(v, a);
        if a.skip {
            if encode {
                arms.push(quote!(Self::#ident { .. } => {}));
            }
            continue;
        }
        let tag_bytes = LitByteStr::new(format!("\"{tag}\":\"{vname}\"").as_bytes(), Span::call_site());
        let arm = match &v.fields {
            Fields::Unit if encode => quote!(Self::#ident => ::twip_wire::field(out, first, #tag_bytes),),
            Fields::Unit => quote!(#vname => { ::twip_wire::decode_unit(r)?; ::core::result::Result::Ok(Self::#ident) }),
            Fields::Unnamed(u) if u.unnamed.len() == 1 => {
                let value = field_attrs(&u.unnamed[0].attrs)?.mode == Mode::Value;
                match (encode, value) {
                    (true, true) => quote!(Self::#ident(v) => {
                        ::twip_wire::field(out, first, #tag_bytes);
                        ::twip_wire::field(out, first, b"\"value\":");
                        ::twip_wire::Encode::encode(v, out);
                    }),
                    (true, false) => quote!(Self::#ident(v) => {
                        ::twip_wire::field(out, first, #tag_bytes);
                        ::twip_wire::EncodeFields::encode_fields(v, out, first);
                    }),
                    (false, true) => quote!(#vname => ::twip_wire::decode_value(r, tags).map(Self::#ident),),
                    (false, false) => quote!(#vname => ::twip_wire::DecodeFields::decode_fields(r, tags).map(Self::#ident),),
                }
            }
            Fields::Named(named) => {
                let fields = named_fields(named, a.rename_all.unwrap_or(Case::None))?;
                if encode {
                    let binds = fields.iter().map(|f| f.ident);
                    let body = encode_fields(&fields, |f| {
                        let i = f.ident;
                        quote!(#i)
                    });
                    quote!(Self::#ident { #(#binds,)* } => {
                        ::twip_wire::field(out, first, #tag_bytes);
                        #body
                    })
                } else {
                    let body = decode_object(&fields, quote!(Self::#ident), deny, false);
                    quote!(#vname => #body)
                }
            }
            _ => return Err(syn::Error::new_spanned(ident, "twip-wire: unsupported variant shape")),
        };
        arms.push(arm);
    }
    Ok(if encode {
        quote! {
            impl #imp ::twip_wire::EncodeFields for #name #ty #wh {
                fn encode_fields(&self, out: &mut ::std::vec::Vec<u8>, first: &mut bool) {
                    match self {
                        #(#arms)*
                    }
                }
            }
            impl #imp ::twip_wire::Encode for #name #ty #wh {
                fn encode(&self, out: &mut ::std::vec::Vec<u8>) {
                    out.push(b'{');
                    ::twip_wire::EncodeFields::encode_fields(self, out, &mut true);
                    out.push(b'}');
                }
            }
        }
    } else {
        quote! {
            impl #imp ::twip_wire::DecodeFields for #name #ty #wh {
                fn decode_fields(r: &mut ::twip_wire::Reader<'_>, tags: ::core::option::Option<&::twip_wire::Tags<'_>>) -> ::core::result::Result<Self, ::twip_wire::Error> {
                    let tag = r.tag(#tag)?;
                    let inner = ::twip_wire::Tags { key: #tag, outer: tags };
                    let tags = ::core::option::Option::Some(&inner);
                    match &*tag {
                        #(#arms)*
                        other => ::core::result::Result::Err(::twip_wire::unknown_variant(r, other)),
                    }
                }
            }
            impl #imp ::twip_wire::Decode for #name #ty #wh {
                fn decode(r: &mut ::twip_wire::Reader<'_>) -> ::core::result::Result<Self, ::twip_wire::Error> {
                    <Self as ::twip_wire::DecodeFields>::decode_fields(r, ::core::option::Option::None)
                }
            }
        }
    })
}

/// serde's default: a unit variant as its name, a newtype variant as `{ "name": value }`.
fn external(
    name: &Ident,
    generics: &syn::Generics,
    variants: &[(&syn::Variant, VariantAttrs)],
    wire_name: &WireName<'_>,
    encode: bool,
) -> syn::Result<TokenStream> {
    let (imp, ty, wh) = generics.split_for_impl();
    let mut units = Vec::new();
    let mut newtypes = Vec::new();
    for (v, a) in variants.iter().filter(|(_, a)| !a.skip) {
        let vname = wire_name(v, a);
        match &v.fields {
            Fields::Unit => units.push((&v.ident, vname)),
            Fields::Unnamed(u) if u.unnamed.len() == 1 => newtypes.push((&v.ident, vname)),
            _ => return Err(syn::Error::new_spanned(&v.ident, "twip-wire: unsupported variant shape")),
        }
    }
    let skipped: Vec<&Ident> = variants.iter().filter(|(_, a)| a.skip).map(|(v, _)| &v.ident).collect();
    Ok(if encode {
        let unit_arms = units.iter().map(|(i, n)| {
            let q = LitByteStr::new(format!("\"{n}\"").as_bytes(), Span::call_site());
            quote!(Self::#i => out.extend_from_slice(#q),)
        });
        let newtype_arms = newtypes.iter().map(|(i, n)| {
            let q = LitByteStr::new(format!("{{\"{n}\":").as_bytes(), Span::call_site());
            quote!(Self::#i(v) => {
                out.extend_from_slice(#q);
                ::twip_wire::Encode::encode(v, out);
                out.push(b'}');
            })
        });
        quote! {
            impl #imp ::twip_wire::Encode for #name #ty #wh {
                fn encode(&self, out: &mut ::std::vec::Vec<u8>) {
                    match self {
                        #(#unit_arms)*
                        #(#newtype_arms)*
                        #(Self::#skipped { .. } => out.extend_from_slice(b"null"),)*
                    }
                }
            }
        }
    } else {
        let unit_arms = units.iter().map(|(i, n)| quote!(#n => ::core::result::Result::Ok(Self::#i),));
        let newtype_arms = newtypes.iter().map(|(i, n)| quote!(#n => Self::#i(::twip_wire::Decode::decode(r)?),));
        quote! {
            impl #imp ::twip_wire::Decode for #name #ty #wh {
                fn decode(r: &mut ::twip_wire::Reader<'_>) -> ::core::result::Result<Self, ::twip_wire::Error> {
                    if r.peek()? == ::twip_wire::Kind::String {
                        let at = r.clone();
                        let s = r.str()?;
                        return match &*s {
                            #(#unit_arms)*
                            other => ::core::result::Result::Err(::twip_wire::unknown_variant(&at, other)),
                        };
                    }
                    let mut o = r.object()?;
                    let k = r.key(&mut o)?.ok_or_else(|| r.error("expected a variant"))?;
                    let v = match &*k {
                        #(#newtype_arms)*
                        other => return ::core::result::Result::Err(::twip_wire::unknown_variant(r, other)),
                    };
                    if r.key(&mut o)?.is_some() {
                        return ::core::result::Result::Err(r.error("expected one variant"));
                    }
                    ::core::result::Result::Ok(v)
                }
            }
        }
    })
}

fn untagged(name: &Ident, generics: &syn::Generics, variants: &[(&syn::Variant, VariantAttrs)], encode: bool) -> syn::Result<TokenStream> {
    let (imp, ty, wh) = generics.split_for_impl();
    let mut arms = Vec::new();
    for (v, a) in variants.iter().filter(|(_, a)| !a.skip) {
        let ident = &v.ident;
        let arm = match &v.fields {
            Fields::Unit if encode => quote!(Self::#ident => out.extend_from_slice(b"null"),),
            Fields::Unit => {
                quote!(if r.null()? { ::core::result::Result::Ok(Self::#ident) } else { ::core::result::Result::Err(r.error("expected null")) })
            }
            Fields::Unnamed(u) if u.unnamed.len() == 1 => {
                if encode {
                    quote!(Self::#ident(v) => ::twip_wire::Encode::encode(v, out),)
                } else {
                    quote!(::twip_wire::Decode::decode(r).map(Self::#ident))
                }
            }
            Fields::Named(named) => {
                let fields = named_fields(named, a.rename_all.unwrap_or(Case::None))?;
                if encode {
                    let binds = fields.iter().map(|f| f.ident);
                    let body = encode_fields(&fields, |f| {
                        let i = f.ident;
                        quote!(#i)
                    });
                    quote!(Self::#ident { #(#binds,)* } => {
                        out.push(b'{');
                        let first = &mut true;
                        #body
                        out.push(b'}');
                    })
                } else {
                    let body = decode_object(&fields, quote!(Self::#ident), false, false);
                    quote!({
                        let tags: ::core::option::Option<&::twip_wire::Tags<'_>> = ::core::option::Option::None;
                        #body
                    })
                }
            }
            _ => return Err(syn::Error::new_spanned(ident, "twip-wire: unsupported variant shape")),
        };
        arms.push(arm);
    }
    let skipped = variants.iter().filter(|(_, a)| a.skip).map(|(v, _)| {
        let i = &v.ident;
        quote!(Self::#i { .. } => out.extend_from_slice(b"null"),)
    });
    let ty_name = name.to_string();
    Ok(if encode {
        quote! {
            impl #imp ::twip_wire::Encode for #name #ty #wh {
                fn encode(&self, out: &mut ::std::vec::Vec<u8>) {
                    match self {
                        #(#arms)*
                        #(#skipped)*
                    }
                }
            }
        }
    } else {
        // Each attempt reads from the saved position; the first that reads wins, as serde's untagged does.
        let attempts = arms.iter().map(|arm| {
            quote! {
                let attempt = |r: &mut ::twip_wire::Reader<'_>| -> ::core::result::Result<Self, ::twip_wire::Error> { #arm };
                if let ::core::result::Result::Ok(v) = attempt(r) {
                    return ::core::result::Result::Ok(v);
                }
                *r = save.clone();
            }
        });
        quote! {
            impl #imp ::twip_wire::Decode for #name #ty #wh {
                fn decode(r: &mut ::twip_wire::Reader<'_>) -> ::core::result::Result<Self, ::twip_wire::Error> {
                    let save = r.clone();
                    #(#attempts)*
                    ::core::result::Result::Err(::twip_wire::no_variant(r, #ty_name))
                }
            }
        }
    })
}
