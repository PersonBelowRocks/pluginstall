//! This module contains types that generalize plugins between different APIs.
//! See submodules for API-specific types and logic.

use std::borrow::Cow;

use chrono::{DateTime, Utc};
use ref_cast::RefCast;
use rq::Url;
use serde::ser::{SerializeMap, SerializeSeq};

pub mod hangar;
pub mod jenkins;
pub mod spiget;

/// The number of fields in a serialized [`PluginVersion`].
const PLUGIN_VERSION_SERIALIZED_FIELDS: usize = 4;

/// Represents a plugin version.
///
/// A plugin version is a file that is associated with a plugin from one of the supported APIs.
/// Two different versions may have the same version name, but they must have different a version identifier.
pub trait PluginVersion {
    /// A string that uniquely identifies this plugin version. No two versions of the same plugin can have the same version identifier.
    fn version_identifier(&self) -> Cow<'_, str>;

    /// A human-readable name for this version (like a semver version). This can be the same as the version identifier,
    /// but in some cases versions have duplicate names but different actual files, in which case the
    /// version identifier must be used to uniquely identify a plugin version.
    fn version_name(&self) -> Cow<'_, str>;

    /// The URL where the file for this version can be downloaded.
    ///
    /// Note: This URL may redirect to the true download URL. Make sure redirects are handled properly!
    ///
    /// # For Implementors
    /// Implementors (and constructors of implemented types) must (try to) ensure that this URL is a "reasonable" download URL.
    /// It doesn't have to be guaranteed to work, but it should:
    /// - Try to download the version specified by all the other properties of a [`PluginVersion`] type.
    /// - Be downloadable through a headless API call (i.e., no URLs that only work in a browser)
    /// - Not error or return 404 or anything of the like.
    ///
    /// If it's unrealistic to aim for these goals in your implementation,
    /// either rethink how you plan on writing the implementation or make a more suitable type to implement this trait for.
    ///
    /// You almost always want the download URL to be contained in the type itself, not be computed when this method is called (hence why it returns a reference).
    fn download_url(&self) -> &Url;

    /// The datetime that this version was published on.
    /// May be [`None`] if no publishing datetime could be found.
    fn publish_date(&self) -> Option<DateTime<Utc>>;

    /// Generalized serialization for all [`PluginVersion`].
    ///
    /// Implementors of this trait should use the default implementation of this method,
    /// unless there's a really good reason to write a custom implementation.
    #[inline]
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let publish_date = self.publish_date();
        let num_fields = match publish_date {
            Some(_) => PLUGIN_VERSION_SERIALIZED_FIELDS,
            None => PLUGIN_VERSION_SERIALIZED_FIELDS - 1,
        };

        let mut map = serializer.serialize_map(Some(num_fields))?;

        map.serialize_entry("version_identifier", self.version_identifier().as_ref())?;
        map.serialize_entry("version_name", self.version_name().as_ref())?;
        map.serialize_entry("download_url", self.download_url())?;

        publish_date.map(|datetime| map.serialize_entry("publish_date", &datetime));

        map.end()
    }

    /// Serialize a slice of plugin versions.
    /// Meant to be used with the serde field tag `#[serde(serialize_with = ...)]`.
    ///
    /// Implementors of this trait should use the default implementation of this method,
    /// unless there's a really good reason to write a custom implementation.
    #[inline]
    fn serialize_slice<S>(versions: &impl AsRef<[Self]>, serializer: S) -> Result<S::Ok, S::Error>
    where
        Self: Sized,
        S: serde::Serializer,
    {
        let versions = versions.as_ref();
        let mut seq = serializer.serialize_seq(Some(versions.len()))?;

        for version in versions {
            seq.serialize_element(PluginVersionWrapper::ref_cast(version))?;
        }

        seq.end()
    }
}

/// Wrapper around a [`PluginVersion`] that implements [`serde::Serialize`].
#[derive(serde::Serialize, RefCast)]
#[serde(transparent)]
#[repr(transparent)]
pub struct PluginVersionWrapper<V: PluginVersion>(
    #[serde(serialize_with = "PluginVersion::serialize")] pub V,
);

/// Wrapper around a [`PluginDetails`] that implements [`serde::Serialize`].
#[derive(serde::Serialize, RefCast)]
#[serde(transparent)]
#[repr(transparent)]
pub struct PluginDetailsWrapper<P: PluginDetails>(
    #[serde(serialize_with = "PluginDetails::serialize")] pub P,
);

/// The number of fields in a serialized [`PluginDetails`].
const PLUGIN_DETAILS_SERIALIZED_FIELDS: usize = 3;

/// The details of a plugin.
pub trait PluginDetails {
    /// The name of this plugin in the manifest file. This is the name used to identify and specify the plugin in the CLI.
    fn manifest_name(&self) -> &str;

    /// The URL to the page of the plugin. A plugin's "page" depends on the plugin's API type.
    ///
    /// The page URL will be the following depending on the API type:
    /// - Hangar: The plugin's page on https://hangar.papermc.io/
    /// - Spiget: The plugin's page on https://www.spigotmc.org/resources/
    /// - Jenkins: It's complicated.
    fn page_url(&self) -> &Url;

    /// The type of API that this plugin comes from.
    fn plugin_type(&self) -> PluginApiType;

    /// Generalized serialization for all [`PluginDetails`].
    ///
    /// Implementors of this trait should use the default implementation of this method,
    /// unless there's a really good reason to write a custom implementation.
    #[inline]
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(Some(PLUGIN_DETAILS_SERIALIZED_FIELDS))?;

        map.serialize_entry("manifest_name", self.manifest_name())?;
        map.serialize_entry("page_url", self.page_url())?;
        map.serialize_entry("plugin_type", &self.plugin_type())?;

        map.end()
    }
}

/// The type of API that a plugin is sourced from.
#[derive(
    Copy, Clone, PartialEq, Eq, Debug, Hash, dm::Display, serde::Serialize, serde::Deserialize,
)]
pub enum PluginApiType {
    #[display("Hangar")]
    Hangar,
    #[display("Spiget")]
    Spiget,
    #[display("Jenkins")]
    Jenkins,
}

/// A plugin version specification. Either a version name, a version identifier, or "latest" can be used to specify a version.
/// This enum unifies all three ways into one type.
#[derive(Debug, Clone, PartialEq, Eq, Hash, dm::Display)]
pub enum VersionSpec {
    /// A version name. The exact format of the name depends on the plugin and the plugin's API.
    #[display("{}", _0)]
    Name(String),
    /// A version identifier. The exact format of the identifier depends on the plugin and the plugin's API.
    #[display("{}", _0)]
    Identifier(String),
    /// The most recent version. Only get the most recent version, do not consider anything else.
    #[display("latest")]
    Latest,
}

impl VersionSpec {
    /// Check if this version spec describes the latest version.
    /// (i.e., [`VersionSpec::Latest`])
    #[inline]
    pub fn is_latest(&self) -> bool {
        matches!(self, Self::Latest)
    }
}
