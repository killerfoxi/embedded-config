use std::{
    env::{self, VarError},
    fmt::Display,
    path::{Path, PathBuf},
    string::FromUtf8Error,
};

use proc_macro2::{Span, TokenStream};
use quote::quote;
use syn::{parse_macro_input, LitStr};

#[proc_macro]
pub fn embed_config_value(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let name = parse_macro_input!(input as LitStr);
    embed_config_value_impl(name)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

#[derive(Debug)]
enum ConfigError {
    NotExist(PathBuf),
    LoadError(PathBuf, std::io::Error),
    InvalidEncoding(String),
    MissingField(String),
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
    pub fn from_io_error<P: Into<PathBuf>>(path: P, err: std::io::Error) -> Self {
        use std::io::ErrorKind;

        if let ErrorKind::NotFound = err.kind() {
            return Self::NotExist(path.into());
        }
        Self::LoadError(path.into(), err)
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
            Self::InvalidConfigValue => write!(f, "package.metadata.embedded-config.path is not of type String"),
            Self::MissingConfig => write!(
                f,
                "Neither EMBEDDED_CONFIG_PATH nor package.metadata.embedded-config.path (in the cargo manifest) is set"
            ),
            Self::Config(e) => write!(f, "loading config: {e}"),
        }
    }
}

struct Config {
    root: toml::Value,
}

impl Config {
    pub fn from_file<P: Into<PathBuf> + AsRef<Path>>(path: P) -> Result<Self, ConfigError> {
        let content = String::from_utf8(
            std::fs::read(path.as_ref()).map_err(|e| ConfigError::from_io_error(path, e))?,
        )?;
        Ok(Self {
            root: toml::from_str(&content)?,
        })
    }

    pub fn resolve_field(&self, name: &str) -> Result<toml::Value, ConfigError> {
        name.split('.')
            .try_fold(&self.root, |cfg, f| {
                cfg.get(f).ok_or(ConfigError::MissingField(name.into()))
            })
            .cloned()
    }
}

fn load_embed_config() -> Result<Config, Error> {
    env::var("EMBEDDED_CONFIG_PATH")
        .map(PathBuf::from)
        .or_else(|_| {
            let mut manifest_dir = env::var("CARGO_MANIFEST_DIR").map(PathBuf::from)?;
            let config = {
                let mut path = manifest_dir.clone();
                path.push("Cargo.toml");
                Config::from_file(path)
            }?;
            let toml::Value::String(s) =
                config.resolve_field("package.metadata.embedded-config.path")?
            else {
                return Err(Error::InvalidConfigValue);
            };
            manifest_dir.push(s);
            Ok(manifest_dir)
        })
        .and_then(|config_file| Ok(Config::from_file(config_file)?))
}

fn embed_config_value_impl(name: LitStr) -> Result<TokenStream, syn::Error> {
    use toml::Value;

    let cfg = load_embed_config().map_err(|e| syn::Error::new(Span::call_site(), e.to_string()))?;
    let val = cfg
        .resolve_field(&name.value())
        .map_err(|e| syn::Error::new(name.span(), e.to_string()))?;
    match val {
        Value::Boolean(v) => Ok(quote! { #v }),
        Value::String(v) => Ok(quote! { #v }),
        Value::Float(v) => Ok(quote! { #v }),
        Value::Integer(v) => Ok(quote! { #v }),
        _ => Err(syn::Error::new(
            name.span(),
            "resulted in unsupported return type",
        )),
    }
}
