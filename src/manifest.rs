//! Manifest file processing logic

use std::collections::HashMap;
use std::path::Path;

use tokio::fs::File;
use tokio::io::AsyncReadExt;

use crate::hangar_plugin::HangarPlugin;
use crate::spiget_plugin::ManifestSpigetPlugin;

pub static DEFAULT_MANIFEST_FILE_NAME: &str = "pluginstall.manifest.toml";

/// A plugin manifest specifying versions of plugins and where to download them from.
/// Usually obtained from deserializing a manifest file.
#[derive(serde::Deserialize, Clone, Debug)]
pub struct Manifest {
    pub meta: ManifestMeta,
    // maps plugin names to their details
    pub plugin: HashMap<String, PluginDownloadSpec>,
}

/// Metadata for a plugin manifest. Is currently just a human-friendly the name of the manifest.
#[derive(serde::Deserialize, Clone, Debug)]
pub struct ManifestMeta {
    /// A human-friendly name for this manifest.
    #[serde(rename = "name")]
    manifest_name: String,
}

/// An enum of various different supported download methods for the plugin.
#[derive(serde::Deserialize, Clone, Debug)]
#[serde(tag = "type")]
#[serde(rename_all = "kebab-case")]
pub enum PluginDownloadSpec {
    /// Gets a plugin from Hangar using the Hangar API.
    Hangar(HangarPlugin),
    /// Uses the Spiget API to download the plugin.
    Spiget(ManifestSpigetPlugin),
    /// Gets a plugin from Jenkins using the Jenkins API.
    Jenkins,
}

/// Error returned when trying to process a manifest file.
#[derive(thiserror::Error, Debug)]
pub enum ManifestError {
    /// IO error, usually because a file could not be found at the specified path.
    #[error(transparent)]
    Io(#[from] std::io::Error),

    /// Parse error, usually because the manifest file was not in valid TOML,
    /// lacked required keys, or contained unrecognized keys.
    #[error(transparent)]
    Parse(#[from] toml::de::Error),
}

/// Type alias for the generic result type returned by manifest operations.
pub type ManifestResult<T> = Result<T, ManifestError>;

impl Manifest {
    /// Parse a manifest object from a file path. Will return errors if the file could not be
    /// found/opened, or if the file contents were not valid manifest TOML.
    #[inline]
    pub async fn parse_from_file(path: impl AsRef<Path>) -> ManifestResult<Self> {
        let path = path.as_ref();
        let mut manifest_file = File::open(path).await?;

        let mut manifest_file_contents = String::with_capacity(1024);
        manifest_file
            .read_to_string(&mut manifest_file_contents)
            .await?;

        Self::parse(manifest_file_contents)
    }

    #[inline]
    pub fn parse(toml: impl AsRef<str>) -> ManifestResult<Self> {
        let toml = toml.as_ref();

        Ok(toml::de::from_str(toml)?)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_parse_manifest() {
        todo!()
    }
}
