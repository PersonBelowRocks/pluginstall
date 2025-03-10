//! Logic for plugins downloaded from spiget.

use std::collections::HashMap;

use chrono::{TimeZone, Utc};
use rq::StatusCode;
use uuid::Uuid;

use crate::session::{self, Session};

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
