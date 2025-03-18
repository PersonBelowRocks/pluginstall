//! The 'info' subcommand for showing information about a plugin in the manifest.

// TODO: allow this command to display info about a specific version too

use std::str::FromStr;

use clap::Args;
use owo_colors::OwoColorize;

use crate::{
    adapter::{
        spiget::{SpigetPlugin, SpigetResourceDetails, VersionId},
        PluginDetails, PluginVersion, VersionSpec,
    },
    cli::Subcommand,
    manifest::{Manifest, PluginDownloadSpec},
    output::DataDisplay,
    session::IoSession,
};

use super::{PluginNotFoundError, VersionSpecArgs};

/// The 'info' subcommand.
#[derive(Args, Debug, Clone)]
pub struct Info {
    #[arg(
        value_name = "PLUGIN_NAME",
        help = "The name of the plugin in the manifest file."
    )]
    pub plugin_name: String,
    #[command(flatten)]
    pub version_spec: VersionSpecArgs,
}

/// The output of the 'info' subcommand.
#[derive(Debug, serde::Serialize)]
pub struct InfoOutput<P: PluginDetails, V: PluginVersion> {
    #[serde(serialize_with = "crate::adapter::PluginDetails::serialize")]
    pub details: P,
    #[serde(serialize_with = "crate::adapter::PluginVersion::serialize")]
    pub version: V,
    /// Whether the version in the output is the latest version or not.
    pub latest: bool,
}

impl<P: PluginDetails, V: PluginVersion> DataDisplay for InfoOutput<P, V> {
    fn write_json(&self, w: &mut impl std::io::Write) -> Result<(), std::io::Error> {
        let json_string = serde_json::to_string(self).unwrap();
        write!(w, "{json_string}")
    }

    fn write_hr(&self, w: &mut impl std::io::Write) -> Result<(), std::io::Error> {
        writeln!(
            w,
            "{0} plugin '{1}' ({2})",
            self.details.plugin_type(),
            self.details.manifest_name().bright_green(),
            self.details.page_url().bright_green(),
        )?;

        writeln!(
            w,
            "Version '{0}' (ID {1}) was published {2}",
            self.version.version_name().bright_green(),
            self.version.version_identifier().bright_green(),
            self.version
                .publish_date()
                .as_ref()
                .map(ToString::to_string)
                .unwrap_or("---".into())
                .bright_green(),
        )
    }
}

impl Subcommand for Info {
    async fn run(&self, session: &IoSession, manifest: &Manifest) -> anyhow::Result<()> {
        let manifest_name = &self.plugin_name;

        let Some(plugin_manifest) = manifest.plugin.get(manifest_name) else {
            return Err(PluginNotFoundError(self.plugin_name.clone()).into());
        };

        let version_spec = self.version_spec.get_version_spec();

        match plugin_manifest {
            PluginDownloadSpec::Spiget(spiget_plugin_manifest) => {
                let spiget_plugin =
                    SpigetPlugin::new(&session, spiget_plugin_manifest.resource_id).await?;

                let mut latest = false;
                let version = match version_spec {
                    VersionSpec::Name(name) => spiget_plugin.search_version(&name).await?,
                    VersionSpec::Identifier(ident) => {
                        let version_id = VersionId::from_str(&ident)?;
                        spiget_plugin.version(version_id).await?
                    }
                    VersionSpec::Latest => {
                        latest = true;
                        spiget_plugin.latest_version().await?
                    }
                };

                let out = InfoOutput {
                    details: SpigetResourceDetails::new(spiget_plugin.resource_id(), manifest_name),
                    version,
                    latest,
                };

                session.cli_output().display(&out)?;
            }
            _ => todo!(),
        }

        Ok(())
    }
}
