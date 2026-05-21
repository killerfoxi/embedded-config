use std::{
    env::{self, VarError},
    fmt::Display,
    path::{Path, PathBuf},
    string::FromUtf8Error,
};

use proc_macro2::{Span, TokenStream};
use quote::quote;
use syn::{LitStr, Token, Type, parse::Parse, parse_macro_input};
use toml::Value;

#[proc_macro]
pub fn embed_config_value(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let MacroInput { path, target } = parse_macro_input!(input as MacroInput);
    embed_config_value_impl(path, target)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

#[proc_macro]
pub fn embed_config_value_opt(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let MacroInput { path, target } = parse_macro_input!(input as MacroInput);
    embed_config_value_impl_opt(path, target)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

struct MacroInput {
    path: LitStr,
    target: Option<Type>,
}

impl Parse for MacroInput {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let path = input.parse()?;
        let target = if input.peek(Token![as]) {
            input.parse::<Token![as]>()?;
            Some(input.parse()?)
        } else {
            None
        };
        Ok(Self { path, target })
    }
}

#[derive(Debug)]
enum ConfigError {
    NotExist(PathBuf),
    LoadError(PathBuf, std::io::Error),
    InvalidEncoding(String),
    MissingField(String),
}

impl ConfigError {
    const fn is_missing(&self) -> bool {
        matches!(self, Self::MissingField(_))
    }

    fn to_syn_error(self, span: Span) -> syn::Error {
        syn::Error::new(span, self)
    }
}

impl Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotExist(p) => write!(f, "config does not exist: {}", p.to_string_lossy()),
            Self::LoadError(p, e) => write!(
                f,
                "loading the config from {} failed: {e}",
                p.to_string_lossy()
            ),
            Self::InvalidEncoding(e) => write!(f, "loading {e} lead to a decode error"),
            Self::MissingField(mf) => write!(f, "config does not contain a field matching {mf}"),
        }
    }
}

impl ConfigError {
    fn from_io_error<P: Into<PathBuf>>(path: P, err: std::io::Error) -> Self {
        if err.kind() == std::io::ErrorKind::NotFound {
            Self::NotExist(path.into())
        } else {
            Self::LoadError(path.into(), err)
        }
    }
}

impl From<FromUtf8Error> for ConfigError {
    fn from(err: FromUtf8Error) -> Self {
        Self::InvalidEncoding(format!(
            "invalid utf-8 character at byte {}",
            err.utf8_error().valid_up_to()
        ))
    }
}

impl From<toml::de::Error> for ConfigError {
    fn from(err: toml::de::Error) -> Self {
        Self::InvalidEncoding(format!("not valid toml: {err}"))
    }
}

#[derive(Debug)]
enum Error {
    InvalidConfigValue,
    MissingConfig,
    Config(ConfigError),
}

impl From<ConfigError> for Error {
    fn from(err: ConfigError) -> Self {
        Self::Config(err)
    }
}

impl From<VarError> for Error {
    fn from(_: VarError) -> Self {
        Self::MissingConfig
    }
}

impl Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidConfigValue => write!(
                f,
                "package.metadata.embedded-config.path is not of type String"
            ),
            Self::MissingConfig => write!(
                f,
                "Neither EMBEDDED_CONFIG_PATH nor package.metadata.embedded-config.path \
                 (in the cargo manifest) is set"
            ),
            Self::Config(e) => write!(f, "loading config: {e}"),
        }
    }
}

enum TargetType {
    U8,
    U16,
    U32,
    U64,
    Usize,
    I8,
    I16,
    I32,
    I64,
    Isize,
    F32,
    F64,
    Array(Box<TargetType>, usize),
}

impl TargetType {
    fn as_type_tokens(&self) -> TokenStream {
        match self {
            Self::U8 => quote! { u8 },
            Self::U16 => quote! { u16 },
            Self::U32 => quote! { u32 },
            Self::U64 => quote! { u64 },
            Self::Usize => quote! { usize },
            Self::I8 => quote! { i8 },
            Self::I16 => quote! { i16 },
            Self::I32 => quote! { i32 },
            Self::I64 => quote! { i64 },
            Self::Isize => quote! { isize },
            Self::F32 => quote! { f32 },
            Self::F64 => quote! { f64 },
            Self::Array(inner, len) => {
                let inner_tokens = inner.as_type_tokens();
                quote! { [#inner_tokens; #len] }
            }
        }
    }
}

fn parse_target_type(ty: &Type) -> Result<TargetType, syn::Error> {
    match ty {
        Type::Path(type_path) if type_path.qself.is_none() => {
            let ident = type_path.path.get_ident().ok_or_else(|| {
                syn::Error::new_spanned(ty, "unsupported target type: expected a primitive type")
            })?;
            match ident.to_string().as_str() {
                "u8" => Ok(TargetType::U8),
                "u16" => Ok(TargetType::U16),
                "u32" => Ok(TargetType::U32),
                "u64" => Ok(TargetType::U64),
                "usize" => Ok(TargetType::Usize),
                "i8" => Ok(TargetType::I8),
                "i16" => Ok(TargetType::I16),
                "i32" => Ok(TargetType::I32),
                "i64" => Ok(TargetType::I64),
                "isize" => Ok(TargetType::Isize),
                "f32" => Ok(TargetType::F32),
                "f64" => Ok(TargetType::F64),
                _ => Err(syn::Error::new_spanned(
                    ty,
                    format!("unsupported target type: {}", ident),
                )),
            }
        }
        Type::Array(type_array) => {
            let inner = parse_target_type(&type_array.elem)?;
            let len = eval_usize_expr(&type_array.len)?;
            Ok(TargetType::Array(Box::new(inner), len))
        }
        _ => Err(syn::Error::new_spanned(
            ty,
            "unsupported target type: expected a primitive or array type",
        )),
    }
}

fn eval_usize_expr(expr: &syn::Expr) -> Result<usize, syn::Error> {
    match expr {
        syn::Expr::Lit(syn::ExprLit {
            lit: syn::Lit::Int(n),
            ..
        }) => n.base10_parse(),
        _ => Err(syn::Error::new_spanned(
            expr,
            "expected integer literal for array length",
        )),
    }
}

struct Config {
    root: toml::Value,
}

impl Config {
    fn from_file<P: Into<PathBuf> + AsRef<Path>>(path: P) -> Result<Self, ConfigError> {
        let content = String::from_utf8(
            std::fs::read(path.as_ref()).map_err(|e| ConfigError::from_io_error(path, e))?,
        )?;
        Ok(Self {
            root: toml::from_str(&content)?,
        })
    }

    fn resolve_field(&self, name: &str) -> Result<toml::Value, ConfigError> {
        name.split('.')
            .try_fold(&self.root, |cfg, field| {
                cfg.get(field)
                    .ok_or_else(|| ConfigError::MissingField(name.into()))
            })
            .cloned()
    }
}

fn load_embed_config() -> Result<Config, Error> {
    let path = env::var("EMBEDDED_CONFIG_PATH")
        .map(PathBuf::from)
        .or_else(|_| {
            let manifest_dir = env::var("CARGO_MANIFEST_DIR").map(PathBuf::from)?;
            let config = Config::from_file(manifest_dir.join("Cargo.toml"))?;
            let toml::Value::String(s) =
                config.resolve_field("package.metadata.embedded-config.path")?
            else {
                return Err(Error::InvalidConfigValue);
            };
            Ok(manifest_dir.join(s))
        })?;
    Config::from_file(path).map_err(Into::into)
}

fn value_to_tokens(
    val: &Value,
    span: Span,
    target: Option<&TargetType>,
) -> Result<TokenStream, syn::Error> {
    match val {
        Value::Array(arr) => {
            let (inner_target, expected_len) = match target {
                Some(TargetType::Array(inner, len)) => (Some(inner.as_ref()), Some(*len)),
                None => (None, None),
                Some(ty) => (Some(ty), None),
            };

            if let Some(len) = expected_len {
                if arr.len() != len {
                    return Err(syn::Error::new(
                        span,
                        format!("expected array of length {}, got {}", len, arr.len()),
                    ));
                }
            }

            let elements: Vec<_> = arr
                .iter()
                .map(|item| value_to_tokens(item, span, inner_target))
                .collect::<Result<_, _>>()?;
            Ok(quote! { [#(#elements),*] })
        }
        _ => {
            let base = match val {
                Value::Boolean(v) => quote! { #v },
                Value::String(v) => quote! { #v },
                Value::Float(v) => quote! { #v },
                Value::Integer(v) => quote! { #v },
                Value::Datetime(v) => {
                    let s = v.to_string();
                    quote! { #s }
                }
                _ => return Err(syn::Error::new(span, "unsupported TOML value type")),
            };

            match target {
                Some(TargetType::Array(_, _)) => Err(syn::Error::new(
                    span,
                    "expected array value for array target type",
                )),
                Some(ty) => {
                    let ty_tokens = ty.as_type_tokens();
                    Ok(quote! { (#base as #ty_tokens) })
                }
                None => Ok(base),
            }
        }
    }
}

fn embed_config_value_impl(
    name: LitStr,
    target_ty: Option<Type>,
) -> Result<TokenStream, syn::Error> {
    let target = target_ty.as_ref().map(parse_target_type).transpose()?;
    let cfg = load_embed_config().map_err(|e| syn::Error::new(Span::call_site(), e.to_string()))?;
    let val = cfg
        .resolve_field(&name.value())
        .map_err(|e| e.to_syn_error(name.span()))?;
    value_to_tokens(&val, name.span(), target.as_ref())
}

fn embed_config_value_impl_opt(
    name: LitStr,
    target_ty: Option<Type>,
) -> Result<TokenStream, syn::Error> {
    let target = target_ty.as_ref().map(parse_target_type).transpose()?;
    let cfg = load_embed_config().map_err(|e| syn::Error::new(Span::call_site(), e.to_string()))?;
    match cfg.resolve_field(&name.value()) {
        Ok(val) => value_to_tokens(&val, name.span(), target.as_ref())
            .map(|tokens| quote! { Some(#tokens) }),
        Err(e) if e.is_missing() => Ok(quote! { None }),
        Err(e) => Err(e.to_syn_error(name.span())),
    }
}
