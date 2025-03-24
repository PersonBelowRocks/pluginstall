//! Logic for plugins downloaded from spiget.

use std::{
    borrow::Cow,
    cmp::min,
    collections::HashMap,
    num::ParseIntError,
    pin::Pin,
    str::FromStr,
    sync::Arc,
    task::{self, Poll},
};

use chrono::Utc;
use derive_new::new;
use futures::{task::FutureObj, FutureExt, Stream, StreamExt, TryStream};
use indexmap::IndexMap;
use miette::{Context, Error, IntoDiagnostic};
use reqwest_middleware::ClientWithMiddleware;
use rq::{Response, StatusCode, Url};
use tokio::sync::{RwLock, RwLockReadGuard};
use uuid::Uuid;

use crate::{
    error::{NotFoundError, ParseError, UnexpectedHttpStatus},
    session::IoSession,
};

use super::{PluginApiType, PluginDetails, PluginVersion, VersionSpec};

/// A Spiget plugin entry in the manifest.
#[derive(serde::Deserialize, Clone, Debug)]
pub struct ManifestSpigetPlugin {
    pub resource_id: ResourceId,
}

/// A resource ID for a Spigot resource.
#[derive(
    Copy,
    Clone,
    Debug,
    PartialEq,
    Eq,
    Hash,
    dm::Into,
    dm::From,
    serde::Deserialize,
    dm::Display,
    dm::Constructor,
)]
#[display("{}", _0)]
pub struct ResourceId(u64);

impl ResourceId {
    /// Get the URL to the page for this plugin on the Spigot website.
    #[inline]
    pub fn plugin_page(&self) -> Url {
        Url::parse(&format!("https://www.spigotmc.org/resources/{}", self.0)).unwrap()
    }
}

impl FromStr for ResourceId {
    type Err = ParseIntError;

    #[inline]
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        u64::from_str(s).map(Self)
    }
}

/// A version ID for a Spigot resource. Version IDs are tied to a specific resource (i.e., versions of that resource).
#[derive(
    Copy,
    Clone,
    Debug,
    PartialEq,
    Eq,
    Hash,
    dm::Into,
    dm::From,
    serde::Deserialize,
    dm::Display,
    dm::Constructor,
)]
#[display("{}", _0)]
pub struct VersionId(u64);

impl FromStr for VersionId {
    type Err = VersionIdParseError;

    #[inline]
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        u64::from_str(s).map(Self).map_err(|_| VersionIdParseError)
    }
}

/// The error returned by [`<VersionId as FromStr>::from_str`].
#[derive(thiserror::Error, miette::Diagnostic, Debug)]
#[error("Error parsing version ID")]
pub struct VersionIdParseError;

/// Model for the resource details as returned by the Spiget API.
#[derive(serde::Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
pub struct SpigetResourceJson {
    pub id: ResourceId,
    pub name: String,
    pub tag: String,
    pub contributors: String,
    pub likes: u64,
    pub file: SpigetResourceFileJson,
    pub tested_versions: Vec<String>,
    // TODO: links?
    pub rating: SpigetRatingJson,
    #[serde(deserialize_with = "chrono::serde::ts_seconds::deserialize")]
    pub release_date: chrono::DateTime<Utc>,
    #[serde(deserialize_with = "chrono::serde::ts_seconds::deserialize")]
    pub update_date: chrono::DateTime<Utc>,
    pub downloads: u64,
    pub external: bool,
    pub versions: Vec<TinyVersionJson>,
    // we don't have a resource icon field, since this is a CLI app
    // we don't include information regarding the premium status of a resource because im lazy
    pub source_code_link: Option<String>,
    pub donation_link: Option<String>,
}

/// A small version JSON object present in the resource details JSON object's `versions` field.
/// Only contains version IDs, and no other information about the version.
#[derive(serde::Deserialize, Debug, Clone)]
pub struct TinyVersionJson {
    pub id: VersionId,
    pub uuid: Uuid,
}

/// Model for a resource file as returned by the Spiget API.
#[derive(serde::Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
pub struct SpigetResourceFileJson {
    #[serde(rename = "type")]
    pub file_type: String,
    pub size: f64,
    pub size_unit: String,
    pub url: String,
    pub external_url: Option<String>,
}

/// Model for a resource version as returned by the Spiget API.
///
/// Fields marked with "may be excluded" will sometimes not be included in outputs from [`SpigetApiClient`] in order to save bandwidth.
/// Check the documentation on the method you're calling to see which fields are excluded. By default all fields are included.
#[derive(serde::Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
pub struct SpigetVersionJson {
    pub id: VersionId,
    /// May be excluded.
    pub uuid: Option<Uuid>,
    pub name: String,
    #[serde(deserialize_with = "chrono::serde::ts_seconds::deserialize")]
    pub release_date: chrono::DateTime<Utc>,
    /// May be excluded.
    pub downloads: Option<u64>,
    /// May be excluded.
    pub rating: Option<SpigetRatingJson>,
}

/// Model for the ratings of a Spigot resource.
#[derive(serde::Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct SpigetRatingJson {
    pub count: u64,
    pub average: f64,
}

/// A client for communicating with the Spiget API.
#[derive(Clone, Debug)]
pub struct SpigetApiClient {
    client: ClientWithMiddleware,
    spiget_base_url: Url,
}

/// Essentially a more verbose variant of [`SpigetVersionJson`]. Implements [`crate::adapter::PluginVersion`], so this type can be used in more general contexts.
/// Holds information about a specific version of a specific resource. But compared to [`SpigetVersionJson`] this type has more information about the resource itself, not just the version.
#[derive(Debug, Clone)]
pub struct SpigetResourceVersion {
    pub resource_id: ResourceId,
    pub version: SpigetVersionJson,
    pub download_url: Url,
}

impl PluginVersion for SpigetResourceVersion {
    fn version_identifier(&self) -> Cow<'_, str> {
        self.version.id.to_string().into()
    }

    fn version_name(&self) -> Cow<'_, str> {
        (&self.version.name).into()
    }

    fn download_url(&self) -> &Url {
        &self.download_url
    }

    fn publish_date(&self) -> Option<chrono::DateTime<Utc>> {
        Some(self.version.release_date)
    }
}

/// Details of a Spiget resource.
/// This type implements [`PluginDetails`] and is meant to be used to pass
/// resource/plugin information to consumers who operate on generalized plugins.
#[derive(Clone, Debug)]
pub struct SpigetResourceDetails {
    pub manifest_name: String,
    pub page_url: Url,
}

impl SpigetResourceDetails {
    /// Construct a new [`SpigetResourceDetails`] from a Spiget resource's ID, and the manifest
    /// name of that plugin. Will compute the page URL based on the resource ID.
    ///
    /// # Warning
    /// Nothing guarantees that the computed page URL will point to a valid resource.
    /// Make sure that there actually exists a resource with the given resource ID before calling this.
    #[inline]
    pub fn new(resource_id: ResourceId, manifest_name: impl Into<String>) -> Self {
        Self {
            manifest_name: manifest_name.into(),
            page_url: Url::parse(&format!("https://www.spigotmc.org/resources/{resource_id}"))
                .unwrap(),
        }
    }
}

impl PluginDetails for SpigetResourceDetails {
    fn manifest_name(&self) -> &str {
        &self.manifest_name
    }

    fn page_url(&self) -> &Url {
        &self.page_url
    }

    fn plugin_type(&self) -> PluginApiType {
        PluginApiType::Spiget
    }
}

/// The base URL for the Spiget API.
pub(crate) static BASE_URL: &str = "https://api.spiget.org/v2/";

/// A type alias to clean up function signatures a bit.
pub type SpigetApiResult<T> = miette::Result<T>;

// TODO: get all versions of the plugin immediately instead of getting them lazily as they are requested.
//  this will be more cache-friendly and much, much, much simpler
#[allow(dead_code)]
impl SpigetApiClient {
    /// Create a new API client, wrapping the given [`reqwest::Client`].
    #[inline]
    #[must_use]
    pub fn new(client: &ClientWithMiddleware) -> Self {
        Self {
            client: client.clone(),
            spiget_base_url: Url::parse(BASE_URL).unwrap(),
        }
    }

    /// Add the given path (a string) to the client's Spiget API base URL.
    #[inline]
    fn endpoint_url(&self, path: &str) -> Result<Url, url::ParseError> {
        self.spiget_base_url.join(path)
    }

    /// Build the request from the given builder, wrapping errors for better user feedback.
    #[inline]
    async fn send_request(
        &self,
        request: reqwest_middleware::RequestBuilder,
    ) -> SpigetApiResult<Response> {
        let request = request
            .build()
            .into_diagnostic()
            .wrap_err("Error building request for Spiget API")?;
        let url = request.url().clone();

        self.client
            .execute(request)
            .await
            .into_diagnostic()
            .wrap_err_with(|| format!("Spiget API error with URL '{url}'"))
    }

    /// Compute the download URL for a given version of a given resource.
    /// The URL is not guaranteed to even point to an existing resource or version, this is just a helper method to avoid code duplication.
    /// Validation of the provided URL must be done seperately.
    #[inline]
    pub fn compute_download_url(&self, resource_id: ResourceId, version_id: VersionId) -> Url {
        self.endpoint_url(&format!(
            "resources/{resource_id}/versions/{version_id}/download/proxy"
        ))
        .unwrap()
    }

    /// Parse an API JSON response to [`T`].
    #[inline]
    async fn parse_response<T: for<'a> serde::Deserialize<'a>>(
        response: Response,
    ) -> SpigetApiResult<T> {
        let url = response.url().clone();
        let response_text = response
            .text()
            .await
            .into_diagnostic()
            .wrap_err_with(|| format!("Error reading response data from '{url}'"))?;

        let deser = serde_json::from_str::<T>(&response_text)
            .map_err(|error| ParseError::json(error, &response_text))
            .wrap_err_with(|| format!("Error parsing response JSON from '{url}'"))?;

        Ok(deser)
    }

    /// Get resource details from the `/resources/{resource_id}` endpoint.
    /// Response JSON will be parsed into a [`SpigotResourceDetails`] type.
    ///
    /// Returns [`SpigetApiError::NotFound`] if a resource with the given ID could not be found.
    #[inline]
    pub async fn resource_details(
        &self,
        resource_id: ResourceId,
    ) -> SpigetApiResult<SpigetResourceJson> {
        let url = self
            .endpoint_url(&format!("resources/{resource_id}"))
            .unwrap();

        let req = self.client.get(url);
        let response = self.send_request(req).await?;

        match response.status() {
            StatusCode::OK => Self::parse_response(response).await,
            StatusCode::NOT_FOUND => Err(NotFoundError::PluginInApi.into()),
            status @ _ => Err(UnexpectedHttpStatus(status).into()),
        }
        .wrap_err_with(|| format!("Error getting details of resource '{resource_id}'"))
    }

    /// Get a list of versions for this resource, starting at the most recent.
    /// The parameter `size` determines the maximum length of the returned list.
    ///
    /// In order to save bandwidth, the versions in the returned vector will not include the following fields:
    /// - [`SpigetResourceVersion::downloads`]
    /// - [`SpigetResourceVersion::rating`]
    /// - [`SpigetResourceVersion::uuid`]
    ///
    /// The returned vector may be empty if no versions have been published for this resource.
    /// Returns [`SpigetApiError::NotFound`] if a resource with the given ID could not be found.
    #[inline]
    pub async fn resource_versions(
        &self,
        resource_id: ResourceId,
        size: u64,
    ) -> SpigetApiResult<Vec<SpigetVersionJson>> {
        let mut url = self
            .endpoint_url(&format!("resources/{resource_id}/versions"))
            .unwrap();
        url.set_query(Some(&format!(
            "size={size}&sort=-releaseDate&fields=id,name,releaseDate"
        )));

        let req = self.client.get(url);
        let response = self.send_request(req).await?;

        match response.status() {
            StatusCode::OK => Self::parse_response(response).await,
            StatusCode::NOT_FOUND => Err(NotFoundError::PluginInApi.into()),
            status @ _ => Err(UnexpectedHttpStatus(status).into()),
        }
        .wrap_err_with(|| format!("Error getting version list of resource '{resource_id}'"))
    }

    /// Get a specific version of the resource.
    /// Unlike [`SpigetApiClient::resource_versions`], the returned [`SpigetResourceVersion`] has all fields, none are excluded.
    ///
    /// Returns [`SpigetApiError::NotFound`] if a resource with the given ID, or a version with the given ID, could not be found.
    #[inline]
    pub async fn resource_version(
        &self,
        resource_id: ResourceId,
        version_id: VersionId,
    ) -> SpigetApiResult<SpigetVersionJson> {
        let url = self
            .endpoint_url(&format!("resources/{resource_id}/versions/{version_id}"))
            .unwrap();

        let req = self.client.get(url);
        let response = self.send_request(req).await?;

        match response.status() {
            StatusCode::OK => Self::parse_response(response).await,
            StatusCode::NOT_FOUND => Err(NotFoundError::Version.into()),
            status @ _ => Err(UnexpectedHttpStatus(status).into()),
        }
        .wrap_err_with(|| {
            format!("Error getting version with ID '{version_id}' of resource '{resource_id}'")
        })
    }

    /// Get a the version of the resource. Similar to [`SpigetApiClient::resource_version`] instead of getting a specific version, this gets the latest version.
    /// Unlike [`SpigetApiClient::resource_versions`], the returned [`SpigetResourceVersion`] has all fields, none are excluded.
    ///
    /// Returns [`SpigetApiError::NotFound`] if a resource with the given ID, or a latest version, could not be found.
    pub async fn resource_version_latest(
        &self,
        resource_id: ResourceId,
    ) -> SpigetApiResult<SpigetVersionJson> {
        let url = self
            .endpoint_url(&format!("resources/{resource_id}/versions/latest"))
            .unwrap();

        let req = self.client.get(url);
        let response = self.send_request(req).await?;

        match response.status() {
            StatusCode::OK => Self::parse_response(response).await,
            StatusCode::NOT_FOUND => Err(NotFoundError::Version.into()),
            status @ _ => Err(UnexpectedHttpStatus(status).into()),
        }
        .wrap_err_with(|| format!("Error getting latest version of resource '{resource_id}'"))
    }

    /// Get the URL to download the provided version of the provided resource.
    /// This method performs validation to ensure that the requested version is actually valid for this resource, and that the requested resource exists in the first place.
    ///
    /// # Warning
    /// This will return a `/resources/{resource_id}/versions/{version_id}/download/proxy` URL. This endpoint is heavily ratelimited!
    /// Be mindful when downloading from it, and cache downloads to avoid placing unnecessary load on the endpoint.
    #[inline]
    pub async fn resource_version_download_url(
        &self,
        resource_id: ResourceId,
        version_id: VersionId,
    ) -> SpigetApiResult<Url> {
        let response_version = self.resource_version(resource_id, version_id).await?;

        // rename here to make code clearer
        let expected_version_id = version_id;

        // ensure that the version IDs actually match up, which they probably do, but we'll play it safe.
        if response_version.id != expected_version_id {
            let response_version_id = response_version.id;

            log::error!("Version ID mismatch for requested version '{version_id}' of resource '{resource_id}'.");
            log::error!(
                "{}='{}', {}='{}'",
                stringify!(expected_version_id),
                expected_version_id,
                stringify!(response_version_id),
                response_version_id
            );

            return Err(miette::Error::from(NotFoundError::Version)).wrap_err_with(|| format!("Error getting download URL for version '{version_id}' of resource '{resource_id}'"));
        }

        // we've verified that the resource and version exist, so this is okay
        let download_url = self.compute_download_url(resource_id, version_id);
        Ok(download_url)
    }
}

/// Construct a map of the latest versions of a given resource.
/// The `limit` argument specifies the maximum number of versions to fetch, starting with the latest.
#[inline]
async fn versions_map(
    session: &IoSession,
    resource_id: ResourceId,
    limit: u64,
) -> SpigetApiResult<SpigetVersionMap> {
    let versions = session
        .spiget_api()
        .resource_versions(resource_id, limit)
        .await?;
    Ok(IndexMap::from_iter(versions.into_iter().map(|v| (v.id, v))))
}

/// Map of version IDs and the JSON for those versions.
pub type SpigetVersionMap = IndexMap<VersionId, SpigetVersionJson>;

/// A plugin on the Spiget API. Provides a friendly interface for getting information about the plugin.
#[derive(Clone)]
pub struct SpigetPlugin {
    io: IoSession,
    resource_details: SpigetResourceJson,
    /// Cached version details. Ordered by release date, with the latest version first.
    cached_versions: Arc<IndexMap<VersionId, SpigetVersionJson>>,
}

impl SpigetPlugin {
    /// Create a new [`SpigetPlugin`] in the given [`IoSession`].
    ///
    /// Returns [`SpigetApiError::NotFoundError`] if a resource with the given ID did not exist.
    #[inline]
    pub async fn new(
        session: &IoSession,
        resource_id: ResourceId,
    ) -> SpigetApiResult<SpigetPlugin> {
        let resource_details = session
            .spiget_api()
            .resource_details(resource_id)
            .await
            .wrap_err("Error with Spiget API")?;
        let num_of_versions = resource_details.versions.len() as u64;
        let versions = versions_map(session, resource_details.id, num_of_versions).await?;

        Ok(Self {
            io: session.clone(),
            cached_versions: Arc::new(versions),
            resource_details,
        })
    }

    #[inline]
    pub fn resource_id(&self) -> ResourceId {
        self.resource_details.id
    }

    /// Try getting a version from the version cache.
    #[inline]
    fn get_cached_version(&self, version_id: VersionId) -> Option<&SpigetVersionJson> {
        self.cached_versions.get(&version_id)
    }

    #[inline]
    pub fn iter_versions(&self) -> VersionsIter<'_> {
        VersionsIter {
            version_json_iter: self.cached_versions.values(),
            resource_id: self.resource_id(),
            spiget_api: self.io.spiget_api(),
        }
    }

    /// Get the latest version of this plugin.
    ///
    /// Returns [`None`] if there is no latest version (i.e., no version has been published).
    #[inline]
    pub fn latest_version(&self) -> Option<SpigetResourceVersion> {
        log::debug!("finding latest version");

        let latest_version = self.cached_versions.first().map(|e| e.1).cloned()?;

        let resource_id = self.resource_id();

        Some(SpigetResourceVersion {
            download_url: self
                .io
                .spiget_api()
                .compute_download_url(resource_id, latest_version.id),
            version: latest_version,
            resource_id,
        })
    }

    /// Get a specific version of this plugin.
    ///
    /// Returns [`None`] if the given version could not be found.
    #[inline]
    pub fn version(&self, version_id: VersionId) -> Option<SpigetResourceVersion> {
        let version = self.get_cached_version(version_id)?.clone();

        Some(SpigetResourceVersion {
            resource_id: self.resource_id(),
            download_url: self
                .io
                .spiget_api()
                .compute_download_url(self.resource_id(), version.id),
            version,
        })
    }

    /// Search for a version with the specified name.
    /// Will return the most recent version with this name.
    ///
    /// Returns [`None`] if a version with the given name could not be found.
    #[inline]
    pub fn search_version(&self, version_name: &str) -> Option<SpigetResourceVersion> {
        self.iter_versions()
            .find(|v| v.version.name == version_name)
    }

    /// Get a version from the given [`VersionSpec`].
    /// Returns [`None`] if no version could be found for the given spec.
    #[inline]
    pub fn version_from_spec(
        &self,
        version_spec: &VersionSpec,
    ) -> SpigetApiResult<Option<SpigetResourceVersion>> {
        Ok(match version_spec {
            VersionSpec::Identifier(ident) => {
                let id = VersionId::from_str(ident)
                    .wrap_err_with(|| format!("'{ident}' is not a valid Spiget version ID"))?;
                self.version(id)
            }
            VersionSpec::Name(name) => self.search_version(name),
            VersionSpec::Latest => self.latest_version(),
        })
    }
}

/// An iterator over the versions of a plugin.
pub struct VersionsIter<'a> {
    version_json_iter: indexmap::map::Values<'a, VersionId, SpigetVersionJson>,
    resource_id: ResourceId,
    spiget_api: &'a SpigetApiClient,
}

impl Iterator for VersionsIter<'_> {
    type Item = SpigetResourceVersion;

    fn next(&mut self) -> Option<Self::Item> {
        let next_version = self.version_json_iter.next()?;

        let download_url = self
            .spiget_api
            .compute_download_url(self.resource_id, next_version.id);

        Some(SpigetResourceVersion {
            resource_id: self.resource_id,
            version: next_version.clone(),
            download_url,
        })
    }
}
