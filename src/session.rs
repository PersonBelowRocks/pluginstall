//! IO logic (networking, filesystem, stdout/stderr, etc.)

use std::{
    io::{self, ErrorKind},
    path::{Path, PathBuf},
    sync::Arc,
};

use chrono::TimeDelta;
use http_cache_reqwest::{CACacheManager, Cache, CacheMode, HttpCache, HttpCacheOptions};
use hyperx::header::{CacheControl, CacheDirective, ContentDisposition, Header};
use miette::{Context, IntoDiagnostic};
use reqwest_middleware::ClientWithMiddleware;
use rq::header::{CACHE_CONTROL, CONTENT_DISPOSITION};
use tokio::{fs::File, io::AsyncWriteExt};

use crate::{
    adapter::{spiget::SpigetApiClient, PluginApiType, PluginVersion, VersionSpec},
    caching::DownloadCache,
    error::diagnostics::{
        invalid_cache_control, invalid_content_disposition, missing_content_disposition,
    },
    ok_none,
    output::CliOutput,
    util::{content_disposition_file_name, validate_file_name},
};

/// The user agent to be used by pluginstall when talking to APIs.
pub static USER_AGENT: &str = "pluginstall (github PersonBelowRocks/pluginstall)";

/// Error emitted by [`IoSession`] operations.
#[derive(thiserror::Error, Debug)]
pub enum IoSessionError {
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
    client: ClientWithMiddleware,
    spiget: SpigetApiClient,
    cli_output: Arc<CliOutput>,
    cache: Arc<DownloadCache>,
}

impl IoSession {
    /// Creates a new API session.
    pub fn new(cli_output: CliOutput, download_cache: DownloadCache) -> Self {
        let client = rq::Client::builder()
            .user_agent(USER_AGENT)
            .connection_verbose(true)
            .build()
            .unwrap();

        let client = reqwest_middleware::ClientBuilder::new(client)
            .with(Cache(HttpCache {
                mode: CacheMode::Default,
                manager: download_cache.cacache_manager(),
                options: HttpCacheOptions::default(),
            }))
            .build();

        Self {
            spiget: SpigetApiClient::new(&client),
            cli_output: Arc::new(cli_output),
            cache: Arc::new(download_cache),
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

    /// Get the download cache.
    #[inline]
    pub fn download_cache(&self) -> &DownloadCache {
        &self.cache
    }

    /// Download the given version to the given path. Returns a [`DownloadReport`] upon success, describing details of this download.
    #[inline]
    pub async fn download_plugin<'a, V: PluginVersion>(
        &self,
        spec: DownloadSpec<'a, V>,
        download_dir: &Path,
    ) -> miette::Result<DownloadReport> {
        let version_ident = spec.version.version_identifier();

        log::debug!("getting cached file");

        let cached_file = self
            .download_cache()
            .get_cached_file(spec.plugin_name, &version_ident)
            .await?;

        if !download_dir.is_dir() {
            miette::bail!(
                "'{}' is not a valid directory path",
                download_dir.to_string_lossy()
            );
        }

        let report = match cached_file {
            Some(mut cached_file) if !cached_file.meta.is_outdated() => {
                log::debug!("retrieving file from cache");

                let copied = cached_file
                    .copy_to_directory(download_dir)
                    .await
                    .wrap_err("Error copying file from cache")?;
                DownloadReport {
                    download_size: copied,
                    cached: true,
                }
            }
            _ => {
                log::debug!("downloading file");

                let url = spec.version.download_url().clone();
                let response = self
                    .client
                    .get(url)
                    .send()
                    .await
                    .into_diagnostic()
                    .wrap_err("Error downloading plugin")?;

                let file_name = response_content_disposition_file_name(&response)?;
                let ttl = response_downloaded_file_ttl(&response)?;

                let file_path = download_dir.join(&file_name);

                let response_data = response
                    .bytes()
                    .await
                    .into_diagnostic()
                    .wrap_err("Error reading response data")?;

                log::debug!("read response data");

                self.cache
                    .cache_file(
                        spec.plugin_name,
                        &spec.version.version_identifier(),
                        &file_name,
                        spec.api_type,
                        ttl,
                        &response_data,
                    )
                    .await?;

                log::debug!("cached downloaded file");

                let download_size = response_data.len();

                let mut file = File::create(file_path)
                    .await
                    .into_diagnostic()
                    .wrap_err("Could not create file")?;

                file.write_all(&response_data)
                    .await
                    .into_diagnostic()
                    .wrap_err("Could not write download data to file")?;

                file.flush()
                    .await
                    .into_diagnostic()
                    .wrap_err("Error flushing data to disk")?;

                log::debug!("wrote to download path");

                DownloadReport {
                    download_size: download_size as _,
                    cached: false,
                }
            }
        };

        Ok(report)
    }
}

/// Get the file name specified in the content disposition header of a response, returning a diagnostic
/// error if it failed.
///
/// This function also does validation of the file name in the header, and errors if it's an invalid/unsafe
/// file name to use on the local filesystem.
#[inline]
fn response_content_disposition_file_name(response: &rq::Response) -> miette::Result<String> {
    let content_disposition = ContentDisposition::parse_header(
        &response
            .headers()
            .get(CONTENT_DISPOSITION)
            .wrap_err_with(missing_content_disposition)?,
    )
    .into_diagnostic()
    .wrap_err_with(invalid_content_disposition)?;

    let file_name = content_disposition_file_name(&content_disposition)
        .wrap_err_with(invalid_content_disposition)?;

    if !validate_file_name(&file_name) {
        miette::bail!("Invalid file name specified by '{CONTENT_DISPOSITION}' header.");
    }

    Ok(file_name.to_string_lossy().into_owned())
}

/// Get the TTL from the cache control header.
/// Will return [`None`] if this response did not have a cache control header,
/// or if the header didn't have max age directive.
///
/// Will error if the cache control header was found but could not be parsed.
#[inline]
fn response_downloaded_file_ttl(response: &rq::Response) -> miette::Result<Option<TimeDelta>> {
    let cache_control = ok_none!(response
        .headers()
        .get(CACHE_CONTROL)
        .as_ref()
        .map(CacheControl::parse_header)
        .transpose()
        .into_diagnostic()
        .wrap_err_with(invalid_cache_control)?);

    // ensure that we're allowed to cache this response
    if cache_control.contains(&CacheDirective::NoStore)
        || cache_control.contains(&CacheDirective::NoCache)
    {
        return Ok(None);
    }

    let max_age = ok_none!(cache_control.iter().find_map(|directive| match directive {
        &CacheDirective::MaxAge(age) if age > 0 => Some(age),
        _ => None,
    }));

    let ttl = TimeDelta::seconds(max_age as _);
    Ok(Some(ttl))
}

/// Details of a successful download.
#[derive(Debug, Clone, serde::Serialize)]
pub struct DownloadReport {
    /// The size of the downloaded file in bytes.
    pub download_size: u64,
    /// Whether the file was retrieved from the cache instead of downloaded from the API.
    pub cached: bool,
}

/// Specifies the download of a specific version of a plugin.
#[derive(Debug, Clone)]
pub struct DownloadSpec<'a, V: PluginVersion> {
    /// The name of the plugin in the manifest. Used for cache operations.
    pub plugin_name: &'a str,
    /// The version of the plugin to download.
    pub version: &'a V,
    /// The API that this plugin is associated with.
    pub api_type: PluginApiType,
}
