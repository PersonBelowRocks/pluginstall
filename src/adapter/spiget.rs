//! Logic for plugins downloaded from spiget.

use std::{borrow::Cow, collections::HashMap, sync::Arc};

use chrono::Utc;
use indexmap::{map::Values, IndexMap};
use rq::{Response, StatusCode, Url};
use tokio::sync::{OnceCell, RwLock, RwLockReadGuard, RwLockWriteGuard};
use uuid::Uuid;

use crate::session::{IoSession, IoSessionResult};

use super::{PluginApiType, PluginDetails, PluginVersion};

pub static SPIGOT_WEBSITE_RESOURCE_PAGE: &str = "https://www.spigotmc.org/resources/{resource_id}";

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

/// Represents an error from an operation on a [`SpigetPlugin`] type.
#[derive(thiserror::Error, Debug)]
pub enum SpigetError {
    #[error("Resource with ID '{0}' could not be found.")]
    ResourceNotFound(ResourceId),
    #[error("Version with ID '{1}' of resource with ID '{0}' could not be found.")]
    ResourceVersionNotFound(ResourceId, VersionId),
    #[error("Internal error: {0}")]
    InternalError(#[from] SpigetApiError),
}

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
    // we don't have a resource icon field, since this is a CLI app
    // we don't include information regarding the premium status of a resource because im lazy
    pub source_code_link: Option<String>,
    pub donation_link: Option<String>,
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
    client: rq::Client,
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
    pub fn new(resource_id: ResourceId, manifest_name: &str) -> Self {
        Self {
            manifest_name: manifest_name.to_string(),
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

/// An error with the Spiget API.
#[derive(thiserror::Error, Debug)]
pub enum SpigetApiError {
    /// An underlying error from Reqwest.
    #[error("Reqwest error: {0}")]
    ReqwestError(#[from] rq::Error),
    /// An error with parsing a JSON response.
    #[error("JSON error: {0}")]
    JsonError(#[from] serde_json::Error),
    /// A 404 error. Usually emitted when a requested resource or resource version does not exist.
    #[error("The requested resource or resource version does not exist.")]
    NotFoundError,
    /// Emitted when the Spiget API returns an unexpected status code.
    #[error("Spiget API returned bad status code: '{0}'")]
    UnknownApiError(StatusCode),
}

/// A type alias to clean up function signatures a bit.
pub type SpigetApiResult<T> = Result<T, SpigetApiError>;

#[allow(dead_code)]
impl SpigetApiClient {
    /// Create a new API client, wrapping the given [`reqwest::Client`].
    #[inline]
    #[must_use]
    pub fn new(client: &rq::Client) -> Self {
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
        let response_bytes = response.bytes().await?;
        let out = serde_json::from_slice::<T>(&response_bytes)?;
        Ok(out)
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

        let response = self.client.get(url).send().await?;

        match response.status() {
            StatusCode::OK => Self::parse_response(response).await,
            StatusCode::NOT_FOUND => Err(SpigetApiError::NotFoundError),
            status @ _ => Err(SpigetApiError::UnknownApiError(status)),
        }
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
        url.set_query(Some(&format!("size={size}&sort=-releaseDate")));

        let response = self.client.get(url).send().await?;

        match response.status() {
            StatusCode::OK => Self::parse_response(response).await,
            StatusCode::NOT_FOUND => Err(SpigetApiError::NotFoundError),
            status @ _ => Err(SpigetApiError::UnknownApiError(status)),
        }
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

        let response = self.client.get(url).send().await?;

        match response.status() {
            StatusCode::OK => Self::parse_response(response).await,
            StatusCode::NOT_FOUND => Err(SpigetApiError::NotFoundError),
            status @ _ => Err(SpigetApiError::UnknownApiError(status)),
        }
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

        let response = self.client.get(url).send().await?;

        match response.status() {
            StatusCode::OK => Self::parse_response(response).await,
            StatusCode::NOT_FOUND => Err(SpigetApiError::NotFoundError),
            status @ _ => Err(SpigetApiError::UnknownApiError(status)),
        }
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

        // rename here to make code cleared
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

            return Err(SpigetApiError::NotFoundError);
        }

        // we've verified that the resource and version exist, so this is okay
        let download_url = self.compute_download_url(resource_id, version_id);
        Ok(download_url)
    }
}

/// A plugin on the Spiget API. Provides a friendly interface for getting information about the plugin.
#[derive(Clone)]
pub struct SpigetPlugin {
    io: IoSession,
    resource_details: SpigetResourceJson,
    /// Cached list of versions sorted from latest to oldest.
    cached_latest_versions: Arc<RwLock<Vec<SpigetResourceVersion>>>,
    /// Cached version details.
    cached_versions: Arc<RwLock<HashMap<VersionId, SpigetVersionJson>>>,
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
        let resource_details = session.spiget_api().resource_details(resource_id).await?;

        Ok(Self {
            io: session.clone(),
            resource_details,
            cached_latest_versions: Default::default(),
            cached_versions: Default::default(),
        })
    }

    #[inline]
    pub fn resource_id(&self) -> ResourceId {
        self.resource_details.id
    }

    /// Update the internal version cache until it's either the size of the provided `limit`, or there are no more versions.
    #[inline]
    async fn update_latest_version_cache(&self, limit: u64) -> SpigetApiResult<()> {
        // cache is already bigger than the limit.
        // we don't handle edge cases where versions are deleted between the previous cache update and this one.
        if limit < self.cached_latest_versions.read().await.len() as u64 {
            return Ok(());
        }

        let versions = self
            .io
            .spiget_api()
            .resource_versions(self.resource_id(), limit)
            .await?;

        let mut cache = self.cached_latest_versions.write().await;

        cache.clear();
        cache.extend(versions.into_iter().map(|version| {
            SpigetResourceVersion {
                resource_id: self.resource_id(),
                download_url: self
                    .io
                    .spiget_api()
                    .compute_download_url(self.resource_id(), version.id),
                version,
            }
        }));

        Ok(())
    }

    /// Try getting a version from the version cache.
    #[inline]
    async fn get_cached_version(&self, version_id: VersionId) -> Option<SpigetVersionJson> {
        self.cached_versions.read().await.get(&version_id).cloned()
    }

    /// Cache the given version.
    #[inline]
    async fn cache_version(&self, version: SpigetVersionJson) {
        self.cached_versions
            .write()
            .await
            .insert(version.id, version);
    }

    /// Get the `limit` latest versions of this plugin. The slice is sorted from latest version to oldest version.
    ///
    /// This caches the versions internally, so subsequent calls with the same or smaller `limit` will get the cached data.
    #[inline]
    pub async fn versions(
        &self,
        limit: u64,
    ) -> SpigetApiResult<RwLockReadGuard<'_, [SpigetResourceVersion]>> {
        self.update_latest_version_cache(limit).await?;

        let limit = limit as usize;
        let guarded_slice =
            RwLockReadGuard::map(self.cached_latest_versions.read().await, |g| &g[..limit]);

        Ok(guarded_slice)
    }

    /// Get the latest version of this plugin.
    ///
    /// Returns [`SpigetApiError::NotFoundError`] if there is no latest version (i.e., no version has been published).
    #[inline]
    pub async fn latest_version(&self) -> SpigetApiResult<SpigetResourceVersion> {
        let latest_version = self
            .versions(1)
            .await?
            .first()
            .cloned()
            .ok_or(SpigetApiError::NotFoundError)?;
        let resource_id = self.resource_id();

        Ok(SpigetResourceVersion {
            download_url: self
                .io
                .spiget_api()
                .compute_download_url(resource_id, latest_version.version.id),
            version: latest_version.version,
            resource_id,
        })
    }

    /// Get a specific version of this plugin.
    ///
    /// Returns [`SpigetApiError::NotFoundError`] if the given version could not be found.
    #[inline]
    pub async fn version(&self, version_id: VersionId) -> SpigetApiResult<SpigetResourceVersion> {
        let version = match self.get_cached_version(version_id).await {
            Some(cached_version) => cached_version,
            None => {
                let version = self
                    .io
                    .spiget_api()
                    .resource_version(self.resource_id(), version_id)
                    .await?;
                self.cache_version(version.clone()).await;
                version
            }
        };

        Ok(SpigetResourceVersion {
            resource_id: self.resource_id(),
            download_url: self
                .io
                .spiget_api()
                .compute_download_url(self.resource_id(), version.id),
            version,
        })
    }
}
