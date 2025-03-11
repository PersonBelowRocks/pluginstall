//! This module contains types that generalize plugins between different APIs.
//! See submodules for API-specific types and logic.

use chrono::{DateTime, Utc};
use rq::Url;

pub mod hangar;
pub mod jenkins;
pub mod spiget;

/// Represents a plugin version.
///
/// A plugin version is a file that is associated with a plugin from one of the supported APIs.
/// Two different versions may have the same version name, but they must have different a version identifier.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PluginVersion {
    /// A string that uniquely identifies this plugin version. No two versions must have the same version identifier.
    pub version_identifier: String,
    /// A human-readable name for this version. This can be the same as the version identifier,
    /// but in some cases versions have duplicate names but different actual files, in which case the
    /// version identifier must be used to uniquely identify a plugin version.
    pub version_name: String,
    /// The URL where the file for this version can be downloaded.
    ///
    /// Note: This URL may redirect to the true download URL. Make sure redirects are handled properly!
    pub download_url: Url,
    /// The datetime that this version was published on.
    /// May be [`None`] if no publishing datetime could be found.
    pub publish_date: Option<DateTime<Utc>>,
}

/// The details of a plugin.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PluginDetails {
    /// The name of this plugin in the manifest file. This is the name used to identify and specify the plugin in the CLI.
    pub manifest_name: String,
    /// The URL to the page of the plugin. A plugin's "page" depends on the plugin's API type.
    ///
    /// The page URL will be the following depending on the API type:
    /// - Hangar: The plugin's page on https://hangar.papermc.io/
    /// - Spiget: The plugin's page on https://www.spigotmc.org/resources/
    /// - Jenkins: It's complicated.
    pub page_url: Url,
    /// The type of API that this plugin comes from.
    pub plugin_type: PluginApiType,
}

/// The type of API that a plugin is sourced from.
#[derive(
    Copy, Clone, PartialEq, Eq, Debug, Hash, dm::Display, serde::Serialize, serde::Deserialize,
)]
pub enum PluginApiType {
    #[display("hangar")]
    Hangar,
    #[display("spiget")]
    Spiget,
    #[display("jenkins")]
    Jenkins,
}
