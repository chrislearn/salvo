use std::fmt::Display;

use proc_macro2::TokenStream;
use quote::{quote, ToTokens};
use syn::spanned::Spanned;
use syn::{parse::Parse, Error, Ident, LitStr, Path};

use crate::{DiagLevel, DiagResult, Diagnostic, TryToTokens};

/// Represents data type of [`Schema`].
#[cfg_attr(feature = "debug", derive(Debug))]
pub(crate) enum SchemaTypeInner {
    /// Generic schema type allows "properties" with custom types
    Object,
    /// Indicates string type of content.
    String,
    /// Indicates integer type of content.    
    Integer,
    /// Indicates floating point number type of content.
    Number,
    /// Indicates boolean type of content.
    Boolean,
    /// Indicates array type of content.
    Array,
    /// Null type. Used together with other type to indicate nullable values.
    Null,
}

impl Display for SchemaTypeInner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let ty = match self {
            Self::Object => "utoipa::openapi::schema::Type::Object",
            Self::String => "utoipa::openapi::schema::Type::String",
            Self::Integer => "utoipa::openapi::schema::Type::Integer",
            Self::Number => "utoipa::openapi::schema::Type::Number",
            Self::Boolean => "utoipa::openapi::schema::Type::Boolean",
            Self::Array => "utoipa::openapi::schema::Type::Array",
            Self::Null => "utoipa::openapi::schema::Type::Null",
        };
        write!(f, "{ty}");
        Ok(())
    }
}

impl ToTokens for SchemaTypeInner {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        self.to_string().to_tokens(tokens);
    }
}

/// Tokenizes OpenAPI data type correctly according to the Rust type
// pub struct SchemaType<'a>(pub &'a syn::Path);
pub struct SchemaType<'a> {
    path: &'a syn::Path,
    format: SchemaFormat<'a>,
    nullable: bool,
}
impl<'a> SchemaType<'a> {
    pub fn new(path: &'a syn::Path, nullable: bool) -> SchemaType<'a> {
        Self {
            path,
            format: SchemaFormat::from(path),
            nullable,
        }
    }

    fn last_segment_to_string(&self) -> String {
        self.path
            .segments
            .last()
            .expect("Expected at least one segment is_integer")
            .ident
            .to_string()
    }

    /// Check whether type is known to be primitive in which case returns true.
    pub(crate) fn is_primitive(&self) -> bool {
        let SchemaType { path, .. } = self;
        let last_segment = match path.segments.last() {
            Some(segment) => segment,
            None => return false,
        };
        let name = &*last_segment.ident.to_string();

        #[cfg(not(any(
            feature = "chrono",
            feature = "decimal",
            feature = "decimal-float",
            feature = "url",
            feature = "ulid",
            feature = "uuid",
            feature = "time",
        )))]
        {
            is_primitive(name)
        }

        #[cfg(any(
            feature = "chrono",
            feature = "decimal",
            feature = "decimal-float",
            feature = "url",
            feature = "ulid",
            feature = "uuid",
            feature = "time",
        ))]
        {
            let mut primitive = is_primitive(name);

            #[cfg(feature = "chrono")]
            if !primitive {
                primitive = matches!(name, "DateTime" | "NaiveDate" | "Duration" | "NaiveDateTime");
            }
            #[cfg(any(feature = "decimal", feature = "decimal-float"))]
            if !primitive {
                primitive = matches!(name, "Decimal")
            }
            #[cfg(feature = "url")]
            if !primitive {
                primitive = matches!(name, "Url");
            }
            #[cfg(feature = "uuid")]
            if !primitive {
                primitive = matches!(name, "Uuid");
            }
            #[cfg(feature = "ulid")]
            if !primitive {
                primitive = matches!(name, "Ulid");
            }
            #[cfg(feature = "time")]
            if !primitive {
                primitive = matches!(name, "Date" | "PrimitiveDateTime" | "OffsetDateTime" | "Duration");
            }

            primitive
        }
    }

    pub(crate) fn is_integer(&self) -> bool {
        matches!(
            &*self.last_segment_to_string(),
            "i8" | "i16" | "i32" | "i64" | "i128" | "isize" | "u8" | "u16" | "u32" | "u64" | "u128" | "usize"
        )
    }

    pub(crate) fn is_unsigned_integer(&self) -> bool {
        matches!(
            &*self.last_segment_to_string(),
            "u8" | "u16" | "u32" | "u64" | "u128" | "usize"
        )
    }

    pub(crate) fn is_number(&self) -> bool {
        match &*self.last_segment_to_string() {
            "f32" | "f64" => true,
            _ if self.is_integer() => true,
            _ => false,
        }
    }

    pub(crate) fn is_string(&self) -> bool {
        matches!(&*self.last_segment_to_string(), "str" | "String")
    }

    pub(crate) fn is_byte(&self) -> bool {
        matches!(&*self.last_segment_to_string(), "u8")
    }
}

#[inline]
fn is_primitive(name: &str) -> bool {
    matches!(
        name,
        "String"
            | "str"
            | "char"
            | "bool"
            | "usize"
            | "u8"
            | "u16"
            | "u32"
            | "u64"
            | "u128"
            | "isize"
            | "i8"
            | "i16"
            | "i32"
            | "i64"
            | "i128"
            | "f32"
            | "f64"
    )
}

impl TryToTokens for SchemaType<'_> {
    fn try_to_tokens(&self, tokens: &mut TokenStream) -> DiagResult<()> {
        let oapi = crate::oapi_crate();
        let last_segment = self.path.segments.last().ok_or_else(|| {
            Diagnostic::spanned(
                self.path.span(),
                DiagLevel::Error,
                "schema type should have at least one segment in the path",
            )
        })?;
        let name = &*last_segment.ident.to_string();

        let inner_type = match name {
            "String" | "str" | "char" => SchemaTypeInner::String,
            "bool" => SchemaTypeInner::Boolean,
            "i8" | "i16" | "i32" | "i64" | "i128" | "isize" | "u8" | "u16" | "u32" | "u64" | "u128" | "usize" => {
                SchemaTypeInner::Integer
            }
            "f32" | "f64" => SchemaTypeInner::Number,
            #[cfg(feature = "chrono")]
            "DateTime" | "NaiveDateTime" | "NaiveDate" | "NaiveTime" => SchemaTypeInner::String,
            #[cfg(any(feature = "chrono", feature = "time"))]
            "Date" | "Duration" => SchemaTypeInner::String,
            #[cfg(feature = "decimal")]
            "Decimal" => SchemaTypeInner::String,
            #[cfg(feature = "decimal_float")]
            "Decimal" => SchemaTypeInner::Number,
            #[cfg(feature = "url")]
            "Url" => SchemaTypeInner::String,
            #[cfg(feature = "ulid")]
            "Ulid" => SchemaTypeInner::String,
            #[cfg(feature = "uuid")]
            "Uuid" => SchemaTypeInner::String,
            #[cfg(feature = "time")]
            "PrimitiveDateTime" | "OffsetDateTime" => SchemaTypeInner::String,
            _ => SchemaTypeInner::Object,
        };
        let schema_type = if self.nullable {
            quote! {
                .schema_type(#oapi::oapi::schema::SchemaType::from_iter([#inner_type, #oapi::oapi::schema::Type::Null]));
            }
        } else {
            quote! {
                .schema_type(#oapi::oapi::schema::SchemaType::new(#inner_type));
            }
        };
        let format = if self.format.is_known_format() {
            let format = &self.format;
            Some(quote! {
                .format(#format)
            })
        } else {
            None
        };

        tokens.extend(quote! {
            #oapi::oapi::schema::Object::new()
                #schema_type
                #format
        });
        Ok(())
    }
}

/// Either Rust type component variant or enum variant schema variant.
#[derive(Clone, Debug)]
pub(crate) enum SchemaFormat<'c> {
    /// [`utoipa::openapi::schema::SchemaFormat`] enum variant schema format.
    ExplicitKnownFormat(ExplicitKnownFormat),
    /// Rust type schema format.
    RustTypeKnownFormat(RustTypeKnownFormat<'c>),
}

impl SchemaFormat<'_> {
    pub(crate) fn is_known_format(&self) -> bool {
        match self {
            Self::RustTypeKnownFormat(ty) => ty.is_known_format(),
            Self::ExplicitKnownFormat(_) => true,
        }
    }
}

impl<'a> From<&'a Path> for SchemaFormat<'a> {
    fn from(path: &'a Path) -> Self {
        Self::RustTypeKnownFormat(RustTypeKnownFormat(path))
    }
}

impl Parse for SchemaFormat<'_> {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        Ok(Self::ExplicitKnownFormat(input.parse()?))
    }
}

impl TryToTokens for SchemaFormat<'_> {
    fn try_to_tokens(&self, tokens: &mut TokenStream) -> DiagResult<()> {
        match self {
            Self::RustTypeKnownFormat(ty) => {
                ty.try_to_tokens(tokens)?;
            }
            Self::ExplicitKnownFormat(variant) => variant.to_tokens(tokens),
        }
        Ok(())
    }
}

/// Tokenizes OpenAPI data type format correctly by given Rust type.
#[derive(Clone, Debug)]
pub struct RustTypeKnownFormat<'a>(&'a syn::Path);

impl RustTypeKnownFormat<'_> {
    /// Check is the format know format. Known formats can be used within `quote!{...}` statements.
    pub(crate) fn is_known_format(&self) -> bool {
        let last_segment = match self.0.segments.last() {
            Some(segment) => segment,
            None => return false,
        };
        let name = &*last_segment.ident.to_string();

        #[cfg(not(any(
            feature = "chrono",
            feature = "decimal",
            feature = "decimal-float",
            feature = "url",
            feature = "ulid",
            feature = "uuid",
            feature = "time"
        )))]
        {
            is_known_format(name)
        }

        #[cfg(any(
            feature = "chrono",
            feature = "decimal",
            feature = "decimal-float",
            feature = "url",
            feature = "ulid",
            feature = "uuid",
            feature = "time"
        ))]
        {
            let mut known_format = is_known_format(name);

            #[cfg(feature = "chrono")]
            if !known_format {
                known_format = matches!(name, "DateTime" | "NaiveDate" | "NaiveDateTime");
            }
            #[cfg(feature = "decimal")]
            if !known_format {
                known_format = matches!(name, "Decimal");
            }
            #[cfg(feature = "decimal-float")]
            if !known_format {
                known_format = matches!(name, "Decimal");
            }
            #[cfg(feature = "url")]
            if !known_format {
                known_format = matches!(name, "Url");
            }
            #[cfg(feature = "ulid")]
            if !known_format {
                known_format = matches!(name, "Ulid");
            }
            #[cfg(feature = "uuid")]
            if !known_format {
                known_format = matches!(name, "Uuid");
            }

            #[cfg(feature = "time")]
            if !known_format {
                known_format = matches!(name, "Date" | "PrimitiveDateTime" | "OffsetDateTime");
            }

            known_format
        }
    }
}

#[inline]
fn is_known_format(name: &str) -> bool {
    matches!(
        name,
        "i8" | "i16" | "i32" | "u8" | "u16" | "u32" | "i64" | "u64" | "f32" | "f64"
    )
}

impl TryToTokens for RustTypeKnownFormat<'_> {
    fn try_to_tokens(&self, tokens: &mut TokenStream) -> DiagResult<()> {
        let oapi = crate::oapi_crate();
        let last_segment = self.0.segments.last().ok_or_else(|| {
            Diagnostic::spanned(
                self.0.span(),
                DiagLevel::Error,
                "type should have at least one segment in the path",
            )
        })?;
        let name = &*last_segment.ident.to_string();

        match name {
            "i8" | "i16" | "i32" | "u8" | "u16" | "u32" => {
                tokens.extend(quote! { #oapi::oapi::SchemaFormat::KnownFormat(#oapi::oapi::KnownFormat::Int32) })
            }
            "i64" | "u64" => {
                tokens.extend(quote! { #oapi::oapi::SchemaFormat::KnownFormat(#oapi::oapi::KnownFormat::Int64) })
            }
            "f32" => tokens.extend(quote! { #oapi::oapi::SchemaFormat::KnownFormat(#oapi::oapi::KnownFormat::Float) }),
            "f64" => tokens.extend(quote! { #oapi::oapi::SchemaFormat::KnownFormat(#oapi::oapi::KnownFormat::Double) }),
            #[cfg(any(feature = "decimal", feature = "decimal-float"))]
            "Decimal" => {
                tokens.extend(quote! { #oapi::oapi::SchemaFormat::KnownFormat(#oapi::oapi::KnownFormat::Decimal) })
            }
            #[cfg(feature = "chrono")]
            "NaiveDate" => {
                tokens.extend(quote! { #oapi::oapi::SchemaFormat::KnownFormat(#oapi::oapi::KnownFormat::Date) })
            }
            #[cfg(feature = "chrono")]
            "DateTime" => {
                tokens.extend(quote! { #oapi::oapi::SchemaFormat::KnownFormat(#oapi::oapi::KnownFormat::DateTime) })
            }
            #[cfg(feature = "chrono")]
            "NaiveDateTime" => {
                tokens.extend(quote! { #oapi::oapi::SchemaFormat::KnownFormat(#oapi::oapi::KnownFormat::DateTime) })
            }
            #[cfg(feature = "time")]
            "Date" => tokens.extend(quote! { #oapi::oapi::SchemaFormat::KnownFormat(#oapi::oapi::KnownFormat::Date) }),
            #[cfg(feature = "url")]
            "Url" => tokens.extend(quote! { #oapi::oapi::SchemaFormat::KnownFormat(#oapi::oapi::KnownFormat::Url) }),
            #[cfg(feature = "ulid")]
            "Ulid" => tokens.extend(quote! { #oapi::oapi::SchemaFormat::KnownFormat(#oapi::oapi::KnownFormat::Ulid) }),
            #[cfg(feature = "uuid")]
            "Uuid" => tokens.extend(quote! { #oapi::oapi::SchemaFormat::KnownFormat(#oapi::oapi::KnownFormat::Uuid) }),
            #[cfg(feature = "time")]
            "PrimitiveDateTime" | "OffsetDateTime" => {
                tokens.extend(quote! { #oapi::oapi::SchemaFormat::KnownFormat(#oapi::oapi::KnownFormat::DateTime) })
            }
            _ => (),
        };

        Ok(())
    }
}

/// [`Parse`] and [`ToTokens`] implementation for [`salvo_oapi::schema::SchemaFormat`].
#[derive(Clone, Debug)]
pub(crate) enum ExplicitKnownFormat {
    Int32,
    Int64,
    Float,
    Double,
    Byte,
    Binary,
    Date,
    DateTime,
    Password,
    #[cfg(feature = "url")]
    Url,
    #[cfg(feature = "ulid")]
    Ulid,
    #[cfg(feature = "uuid")]
    Uuid,
    Custom(String),
}

impl Parse for ExplicitKnownFormat {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        const FORMATS: [&str; 12] = [
            "Int32", "Int64", "Float", "Double", "Byte", "Binary", "Date", "DateTime", "Password", "Ulid", "Uuid",
            "Url",
        ];
        let excluded_format: &[&str] = &[
            #[cfg(not(feature = "url"))]
            "Uri",
            #[cfg(not(feature = "uuid"))]
            "Uuid",
            #[cfg(not(feature = "ulid"))]
            "Ulid",
        ];
        let known_formats = FORMATS
            .into_iter()
            .filter(|format| !excluded_format.contains(format))
            .collect::<Vec<_>>();

        let lookahead = input.lookahead1();
        if lookahead.peek(Ident) {
            let format = input.parse::<Ident>()?;
            let name = &*format.to_string();

            match name {
                "Int32" => Ok(Self::Int32),
                "Int64" => Ok(Self::Int64),
                "Float" => Ok(Self::Float),
                "Double" => Ok(Self::Double),
                "Byte" => Ok(Self::Byte),
                "Binary" => Ok(Self::Binary),
                "Date" => Ok(Self::Date),
                "DateTime" => Ok(Self::DateTime),
                "Password" => Ok(Self::Password),
                #[cfg(feature = "url")]
                "Url" => Ok(Self::Url),
                #[cfg(feature = "uuid")]
                "Uuid" => Ok(Self::Uuid),
                #[cfg(feature = "ulid")]
                "Ulid" => Ok(Self::Ulid),
                _ => Err(Error::new(
                    format.span(),
                    format!(
                        "unexpected format: {name}, expected one of: {}",
                        known_formats.join(", ")
                    ),
                )),
            }
        } else if lookahead.peek(LitStr) {
            let value = input.parse::<LitStr>()?.value();
            Ok(Self::Custom(value))
        } else {
            Err(lookahead.error())
        }
    }
}

impl ToTokens for ExplicitKnownFormat {
    fn to_tokens(&self, stream: &mut proc_macro2::TokenStream) {
        let oapi = crate::oapi_crate();
        match self {
            Self::Int32 => stream.extend(quote!(#oapi::oapi::SchemaFormat::KnownFormat(
                #oapi::oapi::KnownFormat::Int32
            ))),
            Self::Int64 => stream.extend(quote!(#oapi::oapi::SchemaFormat::KnownFormat(
                #oapi::oapi::KnownFormat::Int64
            ))),
            Self::Float => stream.extend(quote!(#oapi::oapi::SchemaFormat::KnownFormat(
                #oapi::oapi::KnownFormat::Float
            ))),
            Self::Double => stream.extend(quote!(#oapi::oapi::SchemaFormat::KnownFormat(
                #oapi::oapi::KnownFormat::Double
            ))),
            Self::Byte => stream.extend(quote!(#oapi::oapi::SchemaFormat::KnownFormat(
                #oapi::oapi::KnownFormat::Byte
            ))),
            Self::Binary => stream.extend(quote!(#oapi::oapi::SchemaFormat::KnownFormat(
                #oapi::oapi::KnownFormat::Binary
            ))),
            Self::Date => stream.extend(quote!(#oapi::oapi::SchemaFormat::KnownFormat(
                #oapi::oapi::KnownFormat::Date
            ))),
            Self::DateTime => stream.extend(quote!(#oapi::oapi::SchemaFormat::KnownFormat(
                #oapi::oapi::KnownFormat::DateTime
            ))),
            Self::Password => stream.extend(quote!(#oapi::oapi::SchemaFormat::KnownFormat(
                #oapi::oapi::KnownFormat::Password
            ))),
            #[cfg(feature = "uuid")]
            Self::Uuid => stream.extend(quote!(#oapi::oapi::SchemaFormat::KnownFormat(
                #oapi::oapi::KnownFormat::Uuid
            ))),
            #[cfg(feature = "ulid")]
            Self::Ulid => stream.extend(quote!(#oapi::oapi::SchemaFormat::KnownFormat(
                #oapi::oapi::KnownFormat::Ulid
            ))),
            #[cfg(feature = "url")]
            Self::Url => stream.extend(quote!(#oapi::oapi::SchemaFormat::KnownFormat(
                #oapi::oapi::KnownFormat::Url
            ))),
            Self::Custom(value) => stream.extend(quote!(#oapi::oapi::SchemaFormat::Custom(
                String::from(#value)
            ))),
        };
    }
}
