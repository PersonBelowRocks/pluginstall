//! IO logic (networking, filesystem, stdout/stderr, etc.)

use std::{io, sync::Arc};

use crate::{
    adapter::spiget::{ResourceId, SpigetApiClient, SpigetApiError, SpigetPlugin},
    manifest::Manifest,
    output::CliOutput,
};

/// The user agent to be used by pluginstall when talking to APIs.
pub static USER_AGENT: &str = "pluginstall CLI app (github PersonBelowRocks/pluginstall)";

/// The base URL for the Spiget API.
pub static SPIGET_API_BASE_URL: &str = "https://api.spiget.org/v2/";
/// The base URL for the Hangar API.
pub static HANGAR_API_BASE_URL: &str = "https://hangar.papermc.io/api/v1/";

/// Error emitted by [`IoSession`] operations.
#[derive(thiserror::Error, Debug)]
pub enum IoSessionError {
    /// Error with the Spiget API.
    #[error("Spiget API error: {0}")]
    SpigetError(#[from] SpigetApiError),
    /// Error with CLI output.
    #[error("CLI output error: {0}")]
    CliOutputError(io::Error),
    /// Error when interfacing with the local filesystem.
    #[error("Filesystem error: {0}")]
    FilesystemError(io::Error),
}

/// The result of an [`IoSession`] operation.
pub type IoSessionResult<T> = Result<T, IoSessionError>;

/// A session for IO operations. Functions as a bridge between both HTTP APIs and the local filesystem (including local filesystem caches).
#[derive(Clone)]
pub struct IoSession {
    client: rq::Client,
    spiget: SpigetApiClient,
    cli_output: Arc<CliOutput>,
}

impl IoSession {
    /// Creates a new API session.
    pub fn new(cli_output: CliOutput) -> Self {
        let client = rq::Client::builder()
            .user_agent(USER_AGENT)
            .connection_verbose(true)
            .build()
            .unwrap();

        Self {
            spiget: SpigetApiClient::new(&client),
            cli_output: Arc::new(cli_output),
            client,
        }
    }

    /// Get the Spiget API client.
    #[inline]
    pub fn spiget_api(&self) -> &SpigetApiClient {
        &self.spiget
    }

    /// Get the CLI output controller.
    #[inline]
    pub fn cli_output(&self) -> &CliOutput {
        &self.cli_output
    }
}
