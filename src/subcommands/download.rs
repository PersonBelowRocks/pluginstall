//! The 'download' subcommand for downloading a plugin in the manifest.

use std::path::{Path, PathBuf};

use clap::Args;
use owo_colors::OwoColorize;

use crate::{
    adapter::{
        spiget::{ResourceId, SpigetPlugin},
        PluginApiType, PluginDetails, PluginVersion, VersionSpec,
    },
    cli::Subcommand,
    manifest::{Manifest, PluginDownloadSpec},
    output::DataDisplay,
    session::IoSession,
};

use super::PluginNotFoundError;

/// The 'download' subcommand.
#[derive(Args, Debug, Clone)]
pub struct Download {
    #[arg(
        value_name = "PLUGIN_NAME",
        help = "The name of the plugin in the manifest file."
    )]
    pub plugin_name: String,

    #[arg(
        short = 'o',
        long,
        value_name = "PATH",
        help = "The directory to download the file into. By default the file will be downloaded into the working directory."
    )]
    pub out_dir: Option<PathBuf>,

    #[arg(
        short = 'V', // using a capital V since we might want to use lowercase 'v' for verbosity
        long,
        value_name = "VERSION_NAME",
        help = "The version name of the plugin to download. If multiple versions with this name exist, then the most recent version will be downloaded."
    )]
    pub version_name: Option<String>,

    #[arg(
        short = 'I',
        long = "version-ident",
        value_name = "VERSION_IDENTIFIER",
        help = "The version identifier of the plugin to download. A plugin can't have duplicate version identifiers."
    )]
    pub version_identifier: Option<String>,
}

/// The output of the 'download' subcommand.
#[derive(Debug, serde::Serialize)]
pub struct DownloadOutput {
    pub download_size: u64,
    pub download_path: PathBuf,
    pub details: PluginDetails,
    pub version: PluginVersion,
}

#[derive(thiserror::Error, Debug)]
#[error("You cannot specify both a version name and a version identifier.")]
pub struct VersionNameOrVersionIdentError;

#[derive(thiserror::Error, Debug)]
#[error("Could not find the version '{version_spec}' for the plugin '{manifest_name}'")]
pub struct VersionNotFound {
    pub manifest_name: String,
    pub version_spec: VersionSpec,
}

impl DataDisplay for DownloadOutput {
    fn write_json(&self, w: &mut impl std::io::Write) -> Result<(), std::io::Error> {
        let json_string = serde_json::to_string(self).unwrap();
        write!(w, "{json_string}")
    }

    fn write_hr(&self, w: &mut impl std::io::Write) -> Result<(), std::io::Error> {
        writeln!(
            w,
            "Downloaded version '{0}' of '{1}' ({2})",
            self.version.version_name.green(),
            self.details.manifest_name.green(),
            pretty_bytes::converter::convert(self.download_size as f64),
        )?;

        match self.version.publish_date {
            Some(datetime) => writeln!(w, "Version was released on {}", datetime.green())?,
            None => writeln!(w, "No release date for this version could be found.")?,
        }

        writeln!(
            w,
            "File was downloaded to '{0}'",
            &self.download_path.to_string_lossy()
        )?;

        Ok(())
    }
}

impl Download {
    /// Get the version spec provided to the command.
    ///
    /// Will return [`VersionSpec::Latest`] if neither were specified.
    /// Will panic if both the version name and version identifier are specified.
    #[inline]
    fn get_version_spec(&self) -> VersionSpec {
        match (self.version_identifier.as_ref(), self.version_name.as_ref()) {
            (Some(version_ident), None) => VersionSpec::Identifier(version_ident.clone()),
            (None, Some(version_name)) => VersionSpec::Name(version_name.clone()),
            (None, None) => VersionSpec::Latest,
            _ => panic!("You cannot specify both version identifier and version name."),
        }
    }

    /// Get the download information from the arguments/options issued to this command.
    /// The returned [`PluginDetails`] and [`PluginVersion`] may be used to carry out the actual download operation itself.
    #[inline]
    async fn get_download_information(
        &self,
        session: &IoSession,
        manifest: &Manifest,
    ) -> anyhow::Result<(PluginDetails, PluginVersion)> {
        let manifest_name = &self.plugin_name;

        // these two options are mutually exclusive
        if self.version_identifier.is_some() && self.version_name.is_some() {
            return Err(VersionNameOrVersionIdentError.into());
        }

        let version_spec = self.get_version_spec();

        let Some(plugin_manifest) = manifest.plugin.get(manifest_name) else {
            return Err(PluginNotFoundError(self.plugin_name.clone()).into());
        };

        Ok(match plugin_manifest {
            PluginDownloadSpec::Spiget(spiget_plugin_manifest) => {
                let spiget_plugin =
                    SpigetPlugin::new(&session, spiget_plugin_manifest.resource_id).await?;

                let Some(download_info) = spiget_plugin
                    .get_download_information(session, &version_spec)
                    .await?
                else {
                    // error if we couldn't find this version
                    return Err(VersionNotFound {
                        manifest_name: manifest_name.clone(),
                        version_spec,
                    }
                    .into());
                };

                let details = PluginDetails {
                    manifest_name: manifest_name.clone(),
                    page_url: spiget_plugin.plugin_page(),
                    plugin_type: PluginApiType::Spiget,
                };

                (details, download_info.into())
            }
            _ => todo!(),
        })
    }
}

impl Subcommand for Download {
    type Output = DownloadOutput;

    async fn run(&self, session: &IoSession, manifest: &Manifest) -> anyhow::Result<Self::Output> {
        let (plugin_details, plugin_version) =
            self.get_download_information(session, manifest).await?;

        let output_directory = self
            .out_dir
            .as_ref()
            .map(<PathBuf as AsRef<Path>>::as_ref)
            .unwrap_or(Path::new("."));

        let (output_path, size) = session
            .download_file(&plugin_version.download_url, output_directory)
            .await?;

        Ok(DownloadOutput {
            download_size: size,
            download_path: output_path,
            details: plugin_details,
            version: plugin_version,
        })
    }
}
