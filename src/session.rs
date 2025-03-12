//! Logic for the HTTP(S) session (and communication) with the various plugin APIs.

use std::{
    marker::PhantomData,
    path::{Path, PathBuf},
    str::FromStr,
};

use futures::TryStreamExt;
use hyperx::header::{ContentDisposition, DispositionParam, DispositionType, Header, RawLike};
use log::debug;
use rq::{
    header::{HeaderValue, CONTENT_DISPOSITION},
    Method, Response, StatusCode, Url,
};
use spiget_endpoints::{
    SPIGET_API_RESOURCE_DETAILS, SPIGET_API_RESOURCE_LATEST_VERSION, SPIGET_API_RESOURCE_VERSION,
    SPIGET_API_RESOURCE_VERSIONS, SPIGET_RESOURCE_ID_PATTERN, SPIGET_RESOURCE_VERSION_PATTERN,
};
use tokio::fs::File;
use tokio_util::io::StreamReader;

use crate::{
    adapter::spiget::ResourceId,
    util::{content_disposition_file_name, validate_file_name},
};

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Reqwest error: {0}")]
    ReqwestError(#[from] rq::Error),
    #[error("URL parse error: {0}")]
    UrlParseError(#[from] url::ParseError),
}

/// An error when downloading a file.
#[derive(thiserror::Error, Debug)]
pub enum DownloadError {
    #[error("Download URL returned status code {0}")]
    StatusError(StatusCode),
    #[error("Cannot download to path '{0}'")]
    InvalidPathError(PathBuf),
    /// The 'content-disposition' of the response is not valid, and thus the download cannot be performed.
    #[error("The content disposition is not valid for downloads: '{0}'")]
    ContentDispositionError(String),
    #[error("The response did not have a 'content-disposition' header.")]
    MissingContentDispositionError,
    #[error("Error parsing content disposition header: {0}")]
    ContentDispositionParseError(hyperx::Error),
    #[error(transparent)]
    SessionError(#[from] Error),
    #[error(transparent)]
    IoError(#[from] std::io::Error),
}

/// The user agent to be used by pluginstall when talking to APIs.
pub static USER_AGENT: &str = "pluginstall CLI (https://github.com/PersonBelowRocks/pluginstall)";

/// The base URL for the Spiget API.
pub static SPIGET_API_BASE_URL: &str = "https://api.spiget.org/v2/";
/// The base URL for the Hangar API.
pub static HANGAR_API_BASE_URL: &str = "https://hangar.papermc.io/api/v1/";

pub(crate) mod spiget_endpoints {
    /// The pattern that should be substituted with a resource ID in API URLs.
    pub static SPIGET_RESOURCE_ID_PATTERN: &str = "{resource_id}";
    /// The pattern that should be substituted with a version in API URLs.
    pub static SPIGET_RESOURCE_VERSION_PATTERN: &str = "{version}";

    /// Endpoint for getting the details of a resource from the Spiget API.
    pub static SPIGET_API_RESOURCE_DETAILS: &str = "resources/{resource_id}";
    /// Endpoint for getting versions of a resource from the Spiget API.
    pub static SPIGET_API_RESOURCE_VERSIONS: &str = "resources/{resource_id}/versions";
    /// Endpoint for getting information about a specific Spiget resource version.
    pub static SPIGET_API_RESOURCE_VERSION: &str = "resources/{resource_id}/versions/{version}";
    /// Endpoint for getting the latest version of a resource.
    pub static SPIGET_API_RESOURCE_LATEST_VERSION: &str = "resources/{resource_id}/versions/latest";

    /// Endpoint for downloading a version of a resource.
    ///
    /// This endpoint may not download the file directly but instead redirect to the true download URL.
    pub static SPIGET_API_RESOURCE_VERSION_DOWNLOAD: &str =
        "resources/{resource_id}/versions/{version}/download";
}

/// A session that can be used to talk to various plugin APIs.
pub struct Session {
    spiget_base_url: Url,
    hangar_base_url: Url,
    rq_client: rq::Client,
}

impl Session {
    /// Creates a new API session.
    pub fn new() -> Self {
        Self {
            spiget_base_url: Url::parse(SPIGET_API_BASE_URL).unwrap(),
            hangar_base_url: Url::parse(HANGAR_API_BASE_URL).unwrap(),
            rq_client: rq::Client::new(),
        }
    }

    /// Download a file. Files will be downloaded into the provided `output_directory` with the file name
    /// specified in the "Content-Disposition" header of the HTTP response for the URL.
    ///
    /// Returns the path to the downloaded file, and the download size in bytes.
    #[inline]
    pub async fn download_file(
        &self,
        url: &Url,
        output_directory: impl AsRef<Path>,
    ) -> Result<(PathBuf, u64), DownloadError> {
        let response = self
            .request(url, Method::GET)
            .send()
            .await
            .map_err(Error::from)?;

        // only accept status code 200 for downloading
        if response.status() != StatusCode::OK {
            return Err(DownloadError::StatusError(response.status()));
        }

        let raw_header: &HeaderValue = response
            .headers()
            .get(CONTENT_DISPOSITION)
            .ok_or(DownloadError::MissingContentDispositionError)?; // if the URL doesn't have this header, it's invalid

        let content_disposition = ContentDisposition::parse_header(&raw_header)
            .map_err(DownloadError::ContentDispositionParseError)?;

        // only attachment disposition types are allowed
        if matches!(content_disposition.disposition, DispositionType::Attachment) {
            let content_disposition = content_disposition.to_string();
            return Err(DownloadError::ContentDispositionError(content_disposition));
        }

        let file_name = content_disposition_file_name(&content_disposition).ok_or_else(|| {
            let cd = content_disposition.to_string();
            DownloadError::ContentDispositionError(cd)
        })?;

        if !validate_file_name(&file_name) {
            return Err(DownloadError::InvalidPathError(file_name));
        }

        let output_path = PathBuf::from(output_directory.as_ref()).join(file_name);

        let mut output_file = File::create(&output_path).await?;

        let mut byte_stream = StreamReader::new(
            response
                .bytes_stream()
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e)),
        );

        let downloaded_and_copied = match tokio::io::copy(&mut byte_stream, &mut output_file).await
        {
            Ok(copied) => copied,
            Err(error) => {
                // if we error during the copy, we want to delete the file that we created.
                tokio::fs::remove_file(&output_path).await?;

                return Err(error.into());
            }
        };

        // write all data to disk
        output_file.sync_all().await?;

        Ok((output_path, downloaded_and_copied))
    }

    /// Create a new request for the given URL, with various default options.
    fn request(&self, url: &Url, method: rq::Method) -> rq::RequestBuilder {
        debug!("session: {method:?} {url}");

        self.rq_client
            .request(method, url.clone())
            .header("User-Agent", USER_AGENT)
    }

    /// Get the details of a resource with the given resource ID from the Spiget API.
    #[inline]
    pub async fn spiget_resource_details(
        &self,
        resource_id: ResourceId,
    ) -> Result<Response, Error> {
        let subbed = SPIGET_API_RESOURCE_DETAILS
            .replace(SPIGET_RESOURCE_ID_PATTERN, &resource_id.to_string());
        let url = self.spiget_base_url.join(&subbed)?;

        let response = self.request(&url, Method::GET).send().await?;
        Ok(response)
    }

    /// Get the versions of a resource with the given resource ID from the Spiget API.
    /// The number of versions returned in the response can be controlled with the `limit` parameter.
    #[inline]
    pub async fn spiget_resource_versions(
        &self,
        resource_id: ResourceId,
        limit: Option<u64>,
    ) -> Result<Response, Error> {
        let subbed = SPIGET_API_RESOURCE_VERSIONS
            .replace(SPIGET_RESOURCE_ID_PATTERN, &resource_id.to_string());
        let mut url = self.spiget_base_url.join(&subbed)?;

        // we always want to sort by the release date so that we get the newest versions first
        let query_str = match limit {
            // set the size of the returned array if needed
            Some(limit) => format!("size={limit}&sort=-releaseDate"),
            None => format!("sort=-releaseDate"),
        };

        url.set_query(Some(&query_str));

        let response = self.request(&url, Method::GET).send().await?;
        Ok(response)
    }

    /// Get information about a version of a Spiget resource.
    #[inline]
    pub async fn spiget_resource_version(
        &self,
        resource_id: ResourceId,
        version_id: u64,
    ) -> Result<Response, Error> {
        let subbed = SPIGET_API_RESOURCE_VERSION
            .replace(SPIGET_RESOURCE_ID_PATTERN, &resource_id.to_string())
            .replace(SPIGET_RESOURCE_VERSION_PATTERN, &version_id.to_string());

        let url = self.spiget_base_url.join(&subbed)?;

        let response = self.request(&url, Method::GET).send().await?;
        Ok(response)
    }

    /// Get the latest versin of a resource from the Spiget API.
    #[inline]
    pub async fn spiget_latest_version(&self, resource_id: ResourceId) -> Result<Response, Error> {
        let subbed = SPIGET_API_RESOURCE_LATEST_VERSION
            .replace(SPIGET_RESOURCE_ID_PATTERN, &resource_id.to_string());

        let url = self.spiget_base_url.join(&subbed)?;

        let response = self.request(&url, Method::GET).send().await?;
        Ok(response)
    }
}
