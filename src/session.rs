//! Logic for the HTTP(S) session (and communication) with the various plugin APIs.

use log::debug;
use rq::{Method, Response, Url};
use spiget_endpoints::{
    SPIGET_API_RESOURCE_DETAILS, SPIGET_API_RESOURCE_VERSIONS, SPIGET_RESOURCE_ID_PATTERN,
};

use crate::adapter::spiget::ResourceId;

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Reqwest error: {0}")]
    ReqwestError(#[from] rq::Error),
    #[error("URL parse error: {0}")]
    UrlParseError(#[from] url::ParseError),
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

    /// Create a new request for the given URL, with various default options.
    fn request(&self, url: Url, method: rq::Method) -> rq::RequestBuilder {
        debug!("session: {method:?} {url}");

        self.rq_client
            .request(method, url)
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

        let response = self.request(url, Method::GET).send().await?;
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

        let response = self.request(url, Method::GET).send().await?;
        Ok(response)
    }
}
