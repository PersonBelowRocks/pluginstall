//! The 'info' subcommand for showing information about a plugin in the manifest.

// TODO: allow this command to display info about a specific version too

use clap::Args;
use miette::{bail, Context, IntoDiagnostic};
use owo_colors::OwoColorize;

use crate::{
    adapter::{
        spiget::{SpigetPlugin, SpigetResourceDetails},
        PluginDetails, PluginVersion,
    },
    cli::Subcommand,
    error::{diagnostics, NotFoundError},
    manifest::{Manifest, PluginDownloadSpec},
    output::DataDisplay,
    session::IoSession,
};

use super::{PluginSpecArgs, VersionSpecArgs};

/// The 'info' subcommand.
#[derive(Args, Debug, Clone)]
pub struct Info {
    #[command(flatten)]
    pub plugin: PluginSpecArgs,
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
    async fn run(&self, session: &IoSession, manifest: &Manifest) -> miette::Result<()> {
        let plugin_manifest = manifest.plugin(&self.plugin.plugin_name)?;
        let version_spec = self.version_spec.get();

        match plugin_manifest {
            PluginDownloadSpec::Spiget(spiget) => {
                let plugin = SpigetPlugin::new(&session, spiget.resource_id).await?;

                let latest = version_spec.is_latest();
                let Some(version) = plugin.version_from_spec(&version_spec)? else {
                    bail!(diagnostics::version_not_found(
                        &self.plugin.plugin_name,
                        &version_spec
                    ));
                };

                let out = InfoOutput {
                    details: SpigetResourceDetails::new(
                        plugin.resource_id(),
                        &self.plugin.plugin_name,
                    ),
                    version,
                    latest,
                };

                session.cli_output().display(&out).into_diagnostic()?;
            }
            _ => todo!(),
        }

        Ok(())
    }
}
