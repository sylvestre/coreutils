// This file is part of the uutils coreutils package.
//
// For the full copyright and license information, please view the LICENSE
// file that was distributed with this source code.
// spell-checker:ignore unic_langid

use fluent::{FluentBundle, FluentResource};
use std::cell::RefCell;
use std::fs;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::thread_local;
use thiserror::Error;
use unic_langid::LanguageIdentifier;
use crate::error::UError;

#[derive(Error, Debug)]
pub enum LocalizationError {
    #[error("I/O error loading '{path}': {source}")]
    IoError {
        source: std::io::Error,
        path: PathBuf,
    },
    #[error("Parse error: {0}")]
    ParseError(String),
    #[error("Bundle error: {0}")]
    BundleError(String),
}

impl From<std::io::Error> for LocalizationError {
    fn from(error: std::io::Error) -> Self {
        LocalizationError::IoError {
            source: error,
            path: PathBuf::from("<unknown>")
        }
    }
}

// Add a generic way to convert LocalizationError to UError
impl UError for LocalizationError {
    fn code(&self) -> i32 {
        1
    }
}

pub const DEFAULT_LOCALE: &str = "en-US";

// A struct to handle localization
pub struct Localizer {
    bundle: FluentBundle<FluentResource>,
}

impl Localizer {
    // Create a new localizer from a bundle
    fn new(bundle: FluentBundle<FluentResource>) -> Self {
        Self { bundle }
    }

    // Get a message by ID with a default fallback
    pub fn get_message(&self, id: &str, default: &str) -> String {
        if let Some(msg) = self.bundle.get_message(id) {
            if let Some(value) = msg.value() {
                let mut errors = Vec::new();
                let formatted = self.bundle.format_pattern(value, None, &mut errors);
                return formatted.to_string();
            }
        }
        default.to_string()
    }

    // Get a message with args
    pub fn get_message_with_args(
        &self,
        id: &str,
        args: fluent::FluentArgs,
        default: &str,
    ) -> String {
        if let Some(msg) = self.bundle.get_message(id) {
            if let Some(value) = msg.value() {
                let mut errors = Vec::new();
                let formatted = self.bundle.format_pattern(value, Some(&args), &mut errors);
                return formatted.to_string();
            }
        }
        default.to_string()
    }
}

// Configuration for localization
#[derive(Clone)]
pub struct LocalizationConfig {
    locales_dir: PathBuf,
    fallback_locales: Vec<LanguageIdentifier>,
}

impl LocalizationConfig {
    // Create a new config with a specific locales directory
    pub fn new<P: AsRef<Path>>(locales_dir: P) -> Self {
        Self {
            locales_dir: locales_dir.as_ref().to_path_buf(),
            fallback_locales: vec![],
        }
    }

    // Set fallback locales
    pub fn with_fallbacks(mut self, fallbacks: Vec<LanguageIdentifier>) -> Self {
        self.fallback_locales = fallbacks;
        self
    }

    // Get path for a specific locale
    fn get_locale_path(&self, locale: &LanguageIdentifier) -> PathBuf {
        self.locales_dir.join(format!("{}.ftl", locale))
    }
}

// Global localizer
thread_local! {
    static LOCALIZER: RefCell<Option<Localizer>> = RefCell::new(None);
}

// Initialize localization with a specific locale and config
pub fn init_localization(
    locale: &LanguageIdentifier,
    config: &LocalizationConfig,
) -> Result<(), LocalizationError> {
    let bundle = create_bundle(locale, config)?;
    LOCALIZER.with(|cell| {
        *cell.borrow_mut() = Some(Localizer::new(bundle));
    });
    Ok(())
}

// Create a bundle for a locale with fallback chain
pub fn create_bundle(
    locale: &LanguageIdentifier,
    config: &LocalizationConfig,
) -> Result<FluentBundle<FluentResource>, LocalizationError> {
    // Create a new bundle with requested locale
    let mut bundle = FluentBundle::new(vec![locale.clone()]);

    // Try to load the requested locale
    let mut locales_to_try = vec![locale.clone()];
    locales_to_try.extend_from_slice(&config.fallback_locales);

    // Always ensure DEFAULT_LOCALE is in the fallback chain
    let default_locale: LanguageIdentifier = DEFAULT_LOCALE
        .parse()
        .map_err(|_| LocalizationError::ParseError("Failed to parse default locale".into()))?;

    if !locales_to_try.contains(&default_locale) {
        locales_to_try.push(default_locale);
    }

    // Try each locale in the chain
    let mut loaded = false;
    let mut tried_paths = Vec::new();

    for try_locale in locales_to_try {

        let locale_path = config.get_locale_path(&try_locale);
        tried_paths.push(locale_path.clone());

        match fs::read_to_string(&locale_path) {
            Ok(ftl_string) => {
                // Parse the FTL resource
                let resource = FluentResource::try_new(ftl_string).map_err(|_| {
                    LocalizationError::ParseError(format!(
                        "Failed to parse localization resource for {}",
                        try_locale
                    ))
                })?;

                // Add the resource to the bundle
                bundle.add_resource(resource).map_err(|_| {
                    LocalizationError::BundleError(format!(
                        "Failed to add resource to bundle for {}",
                        try_locale
                    ))
                })?;

                loaded = true;
                break;
            }
            Err(e) => {
                // Just continue to the next locale
            }
        }
    }

    if !loaded {
        // Create a descriptive error message with all paths we tried
        let paths_str = tried_paths.iter()
            .map(|p| p.to_string_lossy().to_string())
            .collect::<Vec<_>>()
            .join(", ");

        return Err(LocalizationError::IoError {
            source: std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "No localization files found"
            ),
            path: PathBuf::from(paths_str),
        });
    }

    Ok(bundle)
}

// Helper function to get a message
pub fn get_message(id: &str, default: &str) -> String {
    LOCALIZER.with(|cell| match &*cell.borrow() {
        Some(localizer) => localizer.get_message(id, default),
        None => default.to_string(),
    })
}

// Helper function for messages with args
pub fn get_message_with_args(id: &str, args: fluent::FluentArgs, default: &str) -> String {
    LOCALIZER.with(|cell| match &*cell.borrow() {
        Some(localizer) => localizer.get_message_with_args(id, args, default),
        None => default.to_string(),
    })
}

// Function to detect system locale from environment variables
pub fn detect_system_locale() -> Result<LanguageIdentifier, LocalizationError> {
    // Get locale from environment or use default
    let locale_str = std::env::var("LANG")
        .unwrap_or_else(|_| DEFAULT_LOCALE.to_string())
        .split('.')
        .next()
        .unwrap_or(DEFAULT_LOCALE)
        .to_string();

    // Try to parse the locale, fallback to default if invalid
    LanguageIdentifier::from_str(&locale_str).map_err(|_| {
        LocalizationError::ParseError(format!("Failed to parse locale: {}", locale_str))
    })
}

/// Sets up localization using the system locale (or default) and project paths.
/// This is a convenience function to reduce boilerplate in each binary.
pub fn setup_localization(p: &str) -> Result<(), LocalizationError> {
    // Get system locale or use default
    let locale = match detect_system_locale() {
        Ok(locale) => locale,
        Err(_) => LanguageIdentifier::from_str(DEFAULT_LOCALE)
            .expect("Default locale should always be valid"),
    };

    let locales_dir =  PathBuf::from(p);

    let fallback_locales = vec![
        LanguageIdentifier::from_str(DEFAULT_LOCALE)
            .expect("Default locale should always be valid"),
    ];

    let config = LocalizationConfig::new(locales_dir).with_fallbacks(fallback_locales);

    init_localization(&locale, &config)?;

    Ok(())
}
/// Create a FluentArgs with a single key-value pair
pub fn create_args<'a, T: ToString>(key: &'a str, value: T) -> fluent::FluentArgs<'a> {
    let mut args = fluent::FluentArgs::new();
    args.set(key, value.to_string());
    args
}

/// Helper function to get a message with a single argument
pub fn get_message_with_arg<T: ToString>(id: &str, arg_key: &str, arg_value: T, default: &str) -> String {
    let mut args = fluent::FluentArgs::new();
    args.set(arg_key, arg_value.to_string());
    get_message_with_args(id, args, default)
}

/// Helper function to create an error message with an operand
pub fn format_error_with_operand<T: ToString>(id: &str, operand: T, default: &str) -> String {
    get_message_with_arg(id, "operand", operand, default)
}