//! Logic for plugins downloaded from spiget.

use std::str::FromStr;

use chrono::Utc;
use rq::{StatusCode, Url};
use uuid::Uuid;

use crate::session::{
    self,
    spiget_endpoints::{
        SPIGET_API_RESOURCE_VERSION_DOWNLOAD, SPIGET_RESOURCE_ID_PATTERN,
        SPIGET_RESOURCE_VERSION_PATTERN,
    },
    Session, SPIGET_API_BASE_URL,
};

use super::{PluginDetails, PluginVersion, VersionSpec};

pub static SPIGOT_WEBSITE_RESOURCE_PAGE: &str = "https://www.spigotmc.org/resources/{resource_id}";

/// A Spiget plugin entry in the manifest.
#[derive(serde::Deserialize, Clone, Debug)]
pub struct ManifestSpigetPlugin {
    pub resource_id: ResourceId,
}

/// Represents a plugin from the Spiget API. This type supports various operations, most of which require a [`Session`]
/// to be passed as an argument so that the Spiget API may be contacted to obtain information.
#[derive(Clone, Debug)]
pub struct SpigetPlugin {
    /// Details of this resource. Will be populated by default when using [`SpigetPlugin::new`]
    pub details: SpigetResourceDetails,
    /// The versions of this resource. Must be manually set by using [`SpigetPlugin::get_versions()`]. Will be empty by default.
    pub versions: Vec<SpigetResourceVersion>,
}

impl SpigetPlugin {
    /// Get a plugin with the given resource ID from the Spiget API. This method will call the API and populate the type
    /// with information provided by the API. By default this function will not collect version information from the API.
    #[inline]
    pub async fn new(session: &Session, resource_id: ResourceId) -> Result<Self, SpigetError> {
        let response = session.spiget_resource_details(resource_id).await?;

        let resource_details = match response.status() {
            StatusCode::OK => {
                let raw_json = response.bytes().await.map_err(session::Error::from)?;
                serde_json::de::from_slice::<SpigetResourceDetails>(&raw_json)?
            }
            StatusCode::NOT_FOUND => return Err(SpigetError::ResourceNotFound(resource_id).into()),
            _ => return Err(SpigetError::UnknownApiError(response.status()).into()),
        };

        Ok(Self {
            details: resource_details,
            versions: Vec::new(),
        })
    }

    /// Get the URL to the page for this plugin on the Spigot website.
    #[inline]
    pub fn plugin_page(&self) -> Url {
        let subbed = SPIGOT_WEBSITE_RESOURCE_PAGE
            .replace(SPIGET_RESOURCE_ID_PATTERN, &self.details.id.to_string());
        Url::parse(&subbed).unwrap()
    }

    /// Get the versions of this resource from the API. This updates the [`SpigetPlugin`] and returns a slice to the newly obtained versions.
    #[inline]
    pub async fn get_versions(
        &mut self,
        session: &Session,
        limit: Option<u64>,
    ) -> Result<&[SpigetResourceVersion], SpigetError> {
        let response = session
            .spiget_resource_versions(self.details.id, limit)
            .await?;

        let resource_versions = match response.status() {
            StatusCode::OK => {
                let raw_json = response.bytes().await.map_err(session::Error::from)?;
                serde_json::de::from_slice::<Vec<SpigetResourceVersion>>(&raw_json)?
            }
            _ => return Err(SpigetError::UnknownApiError(response.status()).into()),
        };

        self.versions = resource_versions;
        Ok(&self.versions[..])
    }

    /// Iterate over the generalized versions of this plugin.
    ///
    /// The iterator will try to iterate in order of version publishing date; the latest versions will come first.
    pub fn general_versions(&self) -> impl Iterator<Item = PluginVersion> + use<'_> {
        let base_url = Url::parse(SPIGET_API_BASE_URL).unwrap();

        self.versions.iter().filter_map(move |version| {
            Some(PluginVersion {
                version_identifier: version.id.to_string(),
                version_name: version.name.clone(),
                download_url: {
                    let subbed = SPIGET_API_RESOURCE_VERSION_DOWNLOAD
                        .replace(SPIGET_RESOURCE_ID_PATTERN, &self.details.id.to_string())
                        .replace(SPIGET_RESOURCE_VERSION_PATTERN, &version.id.to_string());

                    match base_url.join(&subbed) {
                        Ok(url) => url,
                        Err(error) => {
                            log::error!(
                                "Could not join '{subbed}' with base URL '{base_url}': {error}"
                            );

                            return None;
                        }
                    }
                },
                publish_date: Some(version.release_date),
            })
        })
    }

    /// Get a resource version from the provided version spec. Will return [`None`] if no resource could be found for the given spec.
    #[inline]
    pub async fn get_version(
        &self,
        session: &Session,
        version_spec: &VersionSpec,
    ) -> Result<Option<SpigetResourceVersion>, SpigetError> {
        let version = match version_spec {
            // in these cases the resource version can be retrieved immediately without searching
            spec @ (VersionSpec::Latest | VersionSpec::Identifier(_)) => {
                let response = match spec {
                    VersionSpec::Latest => session.spiget_latest_version(self.details.id).await?,
                    VersionSpec::Identifier(identifier) => {
                        let version_id =
                            u64::from_str(identifier).map_err(SpigetError::VersionIdParseError)?;

                        session
                            .spiget_resource_version(self.details.id, version_id)
                            .await?
                    }
                    // we handle the VersionSpec::Name case in the outer match block
                    _ => unreachable!(),
                };

                // a status code of 404 means that the version (or resource?) was not found.
                // either no latest version was published (i.e., no version was ever published),
                // or the given version identifier didn't exist for this resource
                if response.status() == StatusCode::NOT_FOUND {
                    return Ok(None);
                }

                let raw_json = response.bytes().await.map_err(session::Error::from)?;
                serde_json::de::from_slice::<SpigetResourceVersion>(&raw_json)?
            }
            VersionSpec::Name(name) => {
                todo!()
            }
        };

        Ok(Some(version))
    }

    /// Get the download information (including the download URL) for the provided version of the plugin.
    /// The returned download URL may redirect to the "true" download.
    /// Furthermore, the returned download URL is not guaranteed to work, but is likely to.
    ///
    /// Will return [`None`] if the specified version could not be found for this resource.
    #[inline]
    pub async fn get_download_information(
        &self,
        session: &Session,
        version: &VersionSpec,
    ) -> Result<Option<SpigetPluginDownload>, SpigetError> {
        // if this is None, then no version could be found, which means we shouldn't provide a download URL
        let version = self.get_version(session, version).await?;

        Ok(version.map(|resource_version| {
            let version_id = resource_version.id;

            let subbed = SPIGET_API_RESOURCE_VERSION_DOWNLOAD
                .replace(SPIGET_RESOURCE_ID_PATTERN, &self.details.id.to_string())
                .replace(SPIGET_RESOURCE_VERSION_PATTERN, &version_id.to_string());

            // this is the URL we download the version from
            let download_url = Url::parse(SPIGET_API_BASE_URL)
                .unwrap()
                .join(&subbed)
                .unwrap();

            SpigetPluginDownload {
                download_url,
                version_details: resource_version,
            }
        }))
    }
}

/// Information regarding the downloading of a version of a Spiget plugin.
#[derive(Debug, Clone)]
pub struct SpigetPluginDownload {
    /// The URL to download the file from.
    pub download_url: Url,
    /// Details about this version.
    pub version_details: SpigetResourceVersion,
}

impl From<SpigetPluginDownload> for PluginVersion {
    fn from(value: SpigetPluginDownload) -> Self {
        PluginVersion {
            version_identifier: value.version_details.id.to_string(),
            version_name: value.version_details.name,
            download_url: value.download_url,
            publish_date: Some(value.version_details.release_date),
        }
    }
}

/// A resource ID for a Spigot resource (a plugin basically).
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, dm::Into, dm::From, serde::Deserialize)]
pub struct ResourceId(u64);

impl ToString for ResourceId {
    fn to_string(&self) -> String {
        self.0.to_string()
    }
}

/// Represents an error returned by the Spiget API.
#[derive(thiserror::Error, Debug)]
pub enum SpigetError {
    #[error("Resource with ID {0:?} could not be found")]
    ResourceNotFound(ResourceId),
    #[error("API an unknown error. Status code {0}")]
    UnknownApiError(StatusCode),
    #[error("Error deserializing JSON: {0}")]
    DeserializationError(#[from] serde_json::Error),
    #[error("HTTP error: {0}")]
    SessionError(#[from] session::Error),
    #[error("Error parsing version ID: {0}")]
    VersionIdParseError(<u64 as FromStr>::Err),
}

/// A response from the Spiget API with resource details.
#[derive(serde::Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
pub struct SpigetResourceDetails {
    id: ResourceId,
    file: SpigetResourceFile,
    tested_versions: Vec<String>,
}

/// Model for a resource file as returned by the Spiget API.
#[derive(serde::Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
pub struct SpigetResourceFile {
    #[serde(rename = "type")]
    file_type: String,
    size: f64,
    size_unit: String,
    url: String,
    external_url: Option<String>,
}

/// Model for a resource version as returned by the Spiget API.
#[derive(serde::Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
pub struct SpigetResourceVersion {
    id: u64,
    uuid: Uuid,
    name: String,
    /// Timestamp of the version's release date
    #[serde(deserialize_with = "chrono::serde::ts_seconds::deserialize")]
    release_date: chrono::DateTime<Utc>,
    downloads: u64,
}
