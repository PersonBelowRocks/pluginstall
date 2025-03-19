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
