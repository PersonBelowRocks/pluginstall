use std::error::Error;

use derive_new::new;
use miette::SourceOffset;
use rq::StatusCode;

use crate::adapter::VersionSpec;

/// Error parsing data (like TOML or JSON).
#[derive(thiserror::Error, miette::Diagnostic, Debug)]
#[error("Error parsing provided data")]
pub struct ParseError {
    cause: ParseErrorCause,
    #[source_code]
    input: String,
    #[label("{cause}")]
    location: SourceOffset,
}

impl ParseError {
    /// Create a JSON parse error for the given JSON input.
    #[inline]
    pub fn json(error: serde_json::Error, input: impl Into<String>) -> Self {
        let input: String = input.into();

        Self {
            location: SourceOffset::from_location(&input, error.line(), error.column()),
            cause: ParseErrorCause::JsonError(error),
            input,
        }
    }

    /// Create a TOML parse error for the given TOML input.
    #[inline]
    pub fn toml(error: toml::de::Error, input: impl Into<String>) -> Self {
        let input: String = input.into();

        let span = error.span().map(|span| span.start).unwrap_or(0);

        Self {
            location: SourceOffset::from(span),
            cause: ParseErrorCause::TomlError(error),
            input,
        }
    }
}

/// The cause of the parse error.
#[derive(thiserror::Error, Debug)]
pub enum ParseErrorCause {
    /// Error parsing JSON
    #[error("JSON error: {0}")]
    JsonError(serde_json::Error),
    /// Error parsing TOML
    #[error("TOML error: {0}")]
    TomlError(toml::de::Error),
}

/// Error with finding a plugin or version specified in the CLI invocation.
#[derive(thiserror::Error, miette::Diagnostic, Debug)]
pub enum NotFoundError {
    #[error("Plugin was not found in the manifest.")]
    ManifestPlugin,
    #[error("Could not find plugin in API.")]
    ApiPlugin,
    #[error("Could not find this version of the plugin.")]
    Version,
}

/// An error for when a response has an unexpected status code.
#[derive(thiserror::Error, miette::Diagnostic, Debug)]
#[error("Unexpected response status: {0}")]
pub struct UnexpectedHttpStatus(pub StatusCode);

/// Helpers for easily creating diagnostics.
pub mod diagnostics {
    use std::path::Path;

    use miette::{diagnostic, MietteDiagnostic};
    use rq::header::{CACHE_CONTROL, CONTENT_DISPOSITION};

    use crate::adapter::VersionSpec;

    /// A "version not found" diagnostic.
    #[inline]
    pub fn version_not_found(
        manifest_name: impl Into<String>,
        version_spec: &VersionSpec,
    ) -> MietteDiagnostic {
        let manifest_name: String = manifest_name.into();
        diagnostic!("Could not find version '{version_spec}' for plugin '{manifest_name}'")
    }

    /// An "invalid download directory" diagnostic. Usually emitted when trying to download into a directory that doesn't exist.
    #[inline]
    pub fn invalid_download_dir(dir: &Path) -> MietteDiagnostic {
        diagnostic!(
            "Cannot download to the directory '{}'",
            dir.to_string_lossy()
        )
    }

    /// An error indicating a missing content disposition header in a download response.
    #[inline]
    pub fn missing_content_disposition() -> MietteDiagnostic {
        diagnostic!("Missing '{CONTENT_DISPOSITION}' header in response.")
    }

    /// An error with parsing the content disposition header, or the header did not specify a filename.
    #[inline]
    pub fn invalid_content_disposition() -> MietteDiagnostic {
        diagnostic!("Error parsing the '{CONTENT_DISPOSITION}' header in response.")
    }

    /// An error with parsing the cache control header of a response.
    #[inline]
    pub fn invalid_cache_control() -> MietteDiagnostic {
        diagnostic!("Error parsing the '{CACHE_CONTROL}' header in response.")
    }
}
