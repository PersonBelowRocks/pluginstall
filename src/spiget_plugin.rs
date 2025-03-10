//! Logic for plugins downloaded from spiget.

use rq::StatusCode;
use uuid::Uuid;

#[derive(serde::Deserialize, Clone, Debug)]
pub struct SpigetPlugin {
    resource_id: ResourceId,
}

/// A resource ID for a Spigot resource (a plugin basically).
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, dm::Into, dm::From, serde::Deserialize)]
pub struct ResourceId(u64);

/// Represents an error returned by the Spiget API.
#[derive(thiserror::Error, Debug)]
pub enum SpigetError {
    #[error("Resource with ID {0:?} could not be found")]
    ResourceNotFound(ResourceId),
    #[error("API an unknown error. Status code {0}")]
    UnknownApiError(StatusCode),
}

/// A response from the Spiget API with resource details.
#[derive(serde::Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
pub struct ApiResourceDetails {
    id: ResourceId,
    name: String,
    tag: String,
    file: ApiResourceFile,
    tested_versions: Vec<String>,
    external: bool,
    version: ApiResourceVersion,
    versions: Vec<ApiResourceVersion>,
}

/// Model for a resource file as returned by the Spiget API.
#[derive(serde::Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
pub struct ApiResourceFile {
    #[serde(rename = "type")]
    file_type: String,
    size: u64,
    size_unit: String,
    url: String,
    external_url: String,
}

/// Model for a resource version as returned by the Spiget API.
#[derive(serde::Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
pub struct ApiResourceVersion {
    uuid: Uuid,
    name: String,
    /// Timestamp of the version's release date
    release_date: u64,
    downloads: u64,
}
