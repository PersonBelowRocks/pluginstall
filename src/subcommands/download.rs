//! The 'download' subcommand for downloading a plugin in the manifest.

use std::path::PathBuf;

use clap::Args;

use crate::{
    adapter::{spiget::SpigetPlugin, VersionSpec},
    cli::Subcommand,
    manifest::{Manifest, PluginDownloadSpec},
    output::DataDisplay,
    session::IoSession,
    subcommands::PluginNotFoundError,
};

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
        todo!()
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
}

impl Subcommand for Download {
    async fn run(&self, session: &IoSession, manifest: &Manifest) -> anyhow::Result<()> {
        let manifest_name = &manifest.meta.manifest_name;

        let Some(plugin_manifest) = manifest.plugin.get(manifest_name) else {
            return Err(PluginNotFoundError(self.plugin_name.clone()).into());
        };

        match plugin_manifest {
            PluginDownloadSpec::Hangar(_) => todo!(),
            PluginDownloadSpec::Jenkins => todo!(),
            PluginDownloadSpec::Spiget(spiget) => {
                let plugin = SpigetPlugin::new(session, spiget.resource_id).await?;
            }
        }

        todo!()
    }
}
