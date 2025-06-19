// This file is part of the uutils coreutils package.
//
// For the full copyright and license information, please view the LICENSE
// file that was distributed with this source code.
// spell-checker:disable

use crate::error::UError;
use std::collections::HashMap;
use thiserror::Error;

#[cfg(all(feature = "i18n", not(feature = "no_i18n")))]
use std::sync::OnceLock;

#[cfg(all(feature = "i18n", not(feature = "no_i18n")))]
use fluent::{FluentArgs, FluentBundle, FluentResource};
#[cfg(all(feature = "i18n", not(feature = "no_i18n")))]
use fluent_syntax::parser::ParserError;
#[cfg(all(feature = "i18n", not(feature = "no_i18n")))]
use std::fs;
#[cfg(all(feature = "i18n", not(feature = "no_i18n")))]
use std::path::{Path, PathBuf};
#[cfg(all(feature = "i18n", not(feature = "no_i18n")))]
use std::str::FromStr;
#[cfg(all(feature = "i18n", not(feature = "no_i18n")))]
use unic_langid::LanguageIdentifier;

#[derive(Error, Debug)]
pub enum LocalizationError {
    #[cfg(all(feature = "i18n", not(feature = "no_i18n")))]
    #[error("I/O error loading '{path}': {source}")]
    Io {
        source: std::io::Error,
        path: PathBuf,
    },
    #[cfg(all(feature = "i18n", not(feature = "no_i18n")))]
    #[error("Parse-locale error: {0}")]
    ParseLocale(String),
    #[cfg(all(feature = "i18n", not(feature = "no_i18n")))]
    #[error("Resource parse error at '{snippet}': {error:?}")]
    ParseResource {
        #[source]
        error: ParserError,
        snippet: String,
    },
    #[cfg(all(feature = "i18n", not(feature = "no_i18n")))]
    #[error("Bundle error: {0}")]
    Bundle(String),
    #[cfg(all(feature = "i18n", not(feature = "no_i18n")))]
    #[error("Locales directory not found: {0}")]
    LocalesDirNotFound(String),
    #[cfg(all(feature = "i18n", not(feature = "no_i18n")))]
    #[error("Path resolution error: {0}")]
    PathResolution(String),
    #[error("Localization disabled")]
    Disabled,
}

#[cfg(all(feature = "i18n", not(feature = "no_i18n")))]
impl From<std::io::Error> for LocalizationError {
    fn from(error: std::io::Error) -> Self {
        LocalizationError::Io {
            source: error,
            path: PathBuf::from("<unknown>"),
        }
    }
}

impl UError for LocalizationError {
    fn code(&self) -> i32 {
        1
    }
}

pub const DEFAULT_LOCALE: &str = "en-US";

//==============================================================================
// Configuration with i18n enabled (default)
//==============================================================================

#[cfg(all(feature = "i18n", not(feature = "no_i18n")))]
mod i18n_enabled {
    use super::*;

    // A struct to handle localization with optional English fallback
    pub struct Localizer {
        primary_bundle: FluentBundle<FluentResource>,
        fallback_bundle: Option<FluentBundle<FluentResource>>,
    }

    impl Localizer {
        pub fn new(primary_bundle: FluentBundle<FluentResource>) -> Self {
            Self {
                primary_bundle,
                fallback_bundle: None,
            }
        }

        pub fn with_fallback(mut self, fallback_bundle: FluentBundle<FluentResource>) -> Self {
            self.fallback_bundle = Some(fallback_bundle);
            self
        }

        pub fn format(&self, id: &str, args: Option<&FluentArgs>) -> String {
            // Try primary bundle first
            if let Some(message) = self.primary_bundle.get_message(id).and_then(|m| m.value()) {
                let mut errs = Vec::new();
                return self
                    .primary_bundle
                    .format_pattern(message, args, &mut errs)
                    .to_string();
            }

            // Fall back to English bundle if available
            if let Some(ref fallback) = self.fallback_bundle {
                if let Some(message) = fallback.get_message(id).and_then(|m| m.value()) {
                    let mut errs = Vec::new();
                    return fallback
                        .format_pattern(message, args, &mut errs)
                        .to_string();
                }
            }

            // Return the key ID if not found anywhere
            id.to_string()
        }
    }

    // Global localizer stored in thread-local OnceLock
    thread_local! {
        static LOCALIZER: OnceLock<Localizer> = const { OnceLock::new() };
    }

    // Initialize localization with a specific locale and config
    fn init_localization(
        locale: &LanguageIdentifier,
        locales_dir: &Path,
    ) -> Result<(), LocalizationError> {
        let en_locale = LanguageIdentifier::from_str(DEFAULT_LOCALE)
            .expect("Default locale should always be valid");

        let english_bundle = create_bundle(&en_locale, locales_dir)?;
        let loc = if locale == &en_locale {
            // If requesting English, just use English as primary (no fallback needed)
            Localizer::new(english_bundle)
        } else {
            // Try to load the requested locale
            if let Ok(primary_bundle) = create_bundle(locale, locales_dir) {
                // Successfully loaded requested locale, load English as fallback
                Localizer::new(primary_bundle).with_fallback(english_bundle)
            } else {
                // Failed to load requested locale, just use English as primary
                Localizer::new(english_bundle)
            }
        };

        LOCALIZER.with(|lock| {
            lock.set(loc)
                .map_err(|_| LocalizationError::Bundle("Localizer already initialized".into()))
        })?;
        Ok(())
    }

    // Create a bundle for a specific locale
    fn create_bundle(
        locale: &LanguageIdentifier,
        locales_dir: &Path,
    ) -> Result<FluentBundle<FluentResource>, LocalizationError> {
        let locale_path = locales_dir.join(format!("{locale}.ftl"));

        let ftl_file = fs::read_to_string(&locale_path).map_err(|e| LocalizationError::Io {
            source: e,
            path: locale_path.clone(),
        })?;

        let resource = FluentResource::try_new(ftl_file.clone()).map_err(
            |(_partial_resource, mut errs): (FluentResource, Vec<ParserError>)| {
                let first_err = errs.remove(0);
                // Attempt to extract the snippet from the original ftl_file
                let snippet = if let Some(range) = first_err.slice.clone() {
                    ftl_file.get(range).unwrap_or("").to_string()
                } else {
                    String::new()
                };
                LocalizationError::ParseResource {
                    error: first_err,
                    snippet,
                }
            },
        )?;

        let mut bundle = FluentBundle::new(vec![locale.clone()]);

        // Disable Unicode directional isolate characters (U+2068, U+2069)
        // By default, Fluent wraps variables for security
        // and proper text rendering in mixed-script environments (Arabic + English).
        // Disabling gives cleaner output: "Welcome, Alice!" but reduces protection
        // against bidirectional text attacks. Safe for English-only applications.
        bundle.set_use_isolating(false);

        bundle.add_resource(resource).map_err(|errs| {
            LocalizationError::Bundle(format!(
                "Failed to add resource to bundle for {locale}: {errs:?}",
            ))
        })?;

        Ok(bundle)
    }

    pub fn get_message_internal(id: &str, args: Option<FluentArgs>) -> String {
        LOCALIZER.with(|lock| {
            lock.get()
                .map(|loc| loc.format(id, args.as_ref()))
                .unwrap_or_else(|| id.to_string()) // Return the key ID if localizer not initialized
        })
    }

    // Function to detect system locale from environment variables
    fn detect_system_locale() -> Result<LanguageIdentifier, LocalizationError> {
        let locale_str = std::env::var("LANG")
            .unwrap_or_else(|_| DEFAULT_LOCALE.to_string())
            .split('.')
            .next()
            .unwrap_or(DEFAULT_LOCALE)
            .to_string();
        LanguageIdentifier::from_str(&locale_str).map_err(|_| {
            LocalizationError::ParseLocale(format!("Failed to parse locale: {locale_str}"))
        })
    }

    /// Helper function to get the locales directory based on the build configuration
    fn get_locales_dir(p: &str) -> Result<PathBuf, LocalizationError> {
        #[cfg(debug_assertions)]
        {
            // During development, use the project's locales directory
            let manifest_dir = env!("CARGO_MANIFEST_DIR");
            // from uucore path, load the locales directory from the program directory
            let dev_path = PathBuf::from(manifest_dir)
                .join("../uu")
                .join(p)
                .join("locales");

            if dev_path.exists() {
                return Ok(dev_path);
            }

            // Fallback for development if the expected path doesn't exist
            let fallback_dev_path = PathBuf::from(manifest_dir).join(p);
            if fallback_dev_path.exists() {
                return Ok(fallback_dev_path);
            }

            Err(LocalizationError::LocalesDirNotFound(format!(
                "Development locales directory not found at {} or {}",
                dev_path.display(),
                fallback_dev_path.display()
            )))
        }

        #[cfg(not(debug_assertions))]
        {
            use std::env;
            // In release builds, look relative to executable
            let exe_path = env::current_exe().map_err(|e| {
                LocalizationError::PathResolution(format!("Failed to get executable path: {}", e))
            })?;

            let exe_dir = exe_path.parent().ok_or_else(|| {
                LocalizationError::PathResolution("Failed to get executable directory".to_string())
            })?;

            // Try the coreutils-style path first
            let coreutils_path = exe_dir.join("locales").join(p);
            if coreutils_path.exists() {
                return Ok(coreutils_path);
            }

            // Fallback to just the parameter as a relative path
            let fallback_path = exe_dir.join(p);
            if fallback_path.exists() {
                return Ok(fallback_path);
            }

            return Err(LocalizationError::LocalesDirNotFound(format!(
                "Release locales directory not found at {} or {}",
                coreutils_path.display(),
                fallback_path.display()
            )));
        }
    }

    pub fn setup_localization(p: &str) -> Result<(), LocalizationError> {
        let locale = detect_system_locale().unwrap_or_else(|_| {
            LanguageIdentifier::from_str(DEFAULT_LOCALE)
                .expect("Default locale should always be valid")
        });

        let locales_dir = get_locales_dir(p)?;
        init_localization(&locale, &locales_dir)
    }
}

//==============================================================================
// Configuration with i18n disabled (no_i18n feature)
//==============================================================================

#[cfg(feature = "no_i18n")]
mod i18n_disabled {
    use super::*;

    // Include the embedded strings generated at build time
    include!(concat!(env!("OUT_DIR"), "/embedded_locale.rs"));

    pub fn get_message_internal(id: &str, args: Option<HashMap<String, String>>) -> String {
        if let Some(value) = get_embedded_string(id) {
            if let Some(arg_map) = args {
                // Simple variable substitution for embedded strings
                let mut result = value.to_string();
                for (key, val) in &arg_map {
                    let pattern = format!("{{ ${} }}", key);
                    result = result.replace(&pattern, val);
                }
                result
            } else {
                value.to_string()
            }
        } else {
            // Return the key ID if not found - this serves as English fallback
            id.to_string()
        }
    }

    pub fn setup_localization(_p: &str) -> Result<(), LocalizationError> {
        // No-op for embedded strings - they're always available
        Ok(())
    }
}

//==============================================================================
// Public API (same regardless of feature)
//==============================================================================

/// Retrieves a localized message by its identifier.
///
/// Looks up a message with the given ID in the current locale bundle and returns
/// the localized text. If the message ID is not found in the current locale,
/// it will fall back to English. If the message is not found in English either,
/// returns the message ID itself.
///
/// # Arguments
///
/// * `id` - The message identifier in the Fluent resources
///
/// # Returns
///
/// A `String` containing the localized message, or the message ID if not found
///
/// # Examples
///
/// ```
/// use uucore::locale::get_message;
///
/// // Get a localized greeting (from .ftl files)
/// let greeting = get_message("greeting");
/// println!("{}", greeting);
/// ```
pub fn get_message(id: &str) -> String {
    #[cfg(all(feature = "i18n", not(feature = "no_i18n")))]
    {
        i18n_enabled::get_message_internal(id, None)
    }
    #[cfg(feature = "no_i18n")]
    {
        i18n_disabled::get_message_internal(id, None)
    }
}

/// Retrieves a localized message with variable substitution.
///
/// Looks up a message with the given ID in the current locale bundle,
/// substitutes variables from the provided arguments map, and returns the
/// localized text. If the message ID is not found in the current locale,
/// it will fall back to English. If the message is not found in English either,
/// returns the message ID itself.
///
/// # Arguments
///
/// * `id` - The message identifier in the Fluent resources
/// * `ftl_args` - Key-value pairs for variable substitution in the message
///
/// # Returns
///
/// A `String` containing the localized message with variable substitution, or the message ID if not found
///
/// # Examples
///
/// ```
/// use uucore::locale::get_message_with_args;
/// use std::collections::HashMap;
///
/// // For a Fluent message like: "Hello, { $name }! You have { $count } notifications."
/// let mut args = HashMap::new();
/// args.insert("name".to_string(), "Alice".to_string());
/// args.insert("count".to_string(), "3".to_string());
///
/// let message = get_message_with_args("notification", args);
/// println!("{}", message);
/// ```
pub fn get_message_with_args(id: &str, ftl_args: HashMap<String, String>) -> String {
    #[cfg(all(feature = "i18n", not(feature = "no_i18n")))]
    {
        let mut args = FluentArgs::new();

        for (key, value) in ftl_args {
            // Try to parse as number first for proper pluralization support
            if let Ok(num_val) = value.parse::<i64>() {
                args.set(key, num_val);
            } else if let Ok(float_val) = value.parse::<f64>() {
                args.set(key, float_val);
            } else {
                // Keep as string if not a number
                args.set(key, value);
            }
        }

        i18n_enabled::get_message_internal(id, Some(args))
    }
    #[cfg(feature = "no_i18n")]
    {
        i18n_disabled::get_message_internal(id, Some(ftl_args))
    }
}

/// Sets up localization using the system locale with English fallback.
///
/// This function initializes the localization system based on the system's locale
/// preferences (via the LANG environment variable) or falls back to English
/// if the system locale cannot be determined or the locale file doesn't exist.
/// English is always loaded as a fallback.
///
/// # Arguments
///
/// * `p` - Path to the directory containing localization (.ftl) files
///
/// # Returns
///
/// * `Ok(())` if initialization succeeds
/// * `Err(LocalizationError)` if initialization fails
///
/// # Errors
///
/// Returns a `LocalizationError` if:
/// * The en-US.ftl file cannot be read (English is required)
/// * The files contain invalid Fluent syntax
/// * The bundle cannot be initialized properly
///
/// # Examples
///
/// ```
/// use uucore::locale::setup_localization;
///
/// // Initialize localization using files in the "locales" directory
/// // Make sure you have at least an "en-US.ftl" file in this directory
/// // Other locale files like "fr-FR.ftl" are optional
/// match setup_localization("./locales") {
///     Ok(_) => println!("Localization initialized successfully"),
///     Err(e) => eprintln!("Failed to initialize localization: {}", e),
/// }
/// ```
pub fn setup_localization(p: &str) -> Result<(), LocalizationError> {
    #[cfg(all(feature = "i18n", not(feature = "no_i18n")))]
    {
        i18n_enabled::setup_localization(p)
    }
    #[cfg(feature = "no_i18n")]
    {
        i18n_disabled::setup_localization(p)
    }
}

// Tests remain the same, but only enabled when i18n is available
#[cfg(all(test, feature = "i18n", not(feature = "no_i18n")))]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::env;
    use std::fs;
    use std::path::PathBuf;
    use tempfile::TempDir;

    // Helper function to create a temporary directory with test locale files
    fn create_test_locales_dir() -> TempDir {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");

        // Create en-US.ftl
        let en_content = r#"
greeting = Hello, world!
welcome = Welcome, { $name }!
count-items = You have { $count ->
    [one] { $count } item
   *[other] { $count } items
}
missing-in-other = This message only exists in English
"#;

        // Create fr-FR.ftl
        let fr_content = r#"
greeting = Bonjour, le monde!
welcome = Bienvenue, { $name }!
count-items = Vous avez { $count ->
    [one] { $count } élément
   *[other] { $count } éléments
}
"#;

        fs::write(temp_dir.path().join("en-US.ftl"), en_content)
            .expect("Failed to write en-US.ftl");
        fs::write(temp_dir.path().join("fr-FR.ftl"), fr_content)
            .expect("Failed to write fr-FR.ftl");

        temp_dir
    }

    #[test]
    fn test_get_message_basic() {
        std::thread::spawn(|| {
            let temp_dir = create_test_locales_dir();
            let result = setup_localization(temp_dir.path().to_str().unwrap());
            assert!(result.is_ok());

            let message = get_message("greeting");
            // Should get English since LANG is not set to French
            assert!(message.contains("Hello") || message.contains("Bonjour"));
        })
        .join()
        .unwrap();
    }

    #[test]
    fn test_get_message_with_args_basic() {
        std::thread::spawn(|| {
            let temp_dir = create_test_locales_dir();
            let result = setup_localization(temp_dir.path().to_str().unwrap());
            assert!(result.is_ok());

            let mut args = HashMap::new();
            args.insert("name".to_string(), "Alice".to_string());

            let message = get_message_with_args("welcome", args);
            assert!(message.contains("Alice"));
        })
        .join()
        .unwrap();
    }
}
