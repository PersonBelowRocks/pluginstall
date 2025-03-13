//! The 'info' subcommand for showing information about a plugin in the manifest.

// TODO: allow this command to display info about a specific version too

use clap::Args;
use owo_colors::OwoColorize;

use crate::{
    adapter::{spiget::SpigetPlugin, PluginApiType, PluginDetails},
    cli::Subcommand,
    manifest::{Manifest, PluginDownloadSpec},
    output::DataDisplay,
    session::IoSession,
};

use super::PluginNotFoundError;

/// The 'info' subcommand.
#[derive(Args, Debug, Clone)]
pub struct Info {
    #[arg(
        value_name = "PLUGIN_NAME",
        help = "The name of the plugin in the manifest file."
    )]
    pub plugin_name: String,
}

/// The output of the 'info' subcommand.
#[derive(Debug, serde::Serialize)]
pub struct InfoOutput<P: PluginDetails> {
    #[serde(serialize_with = "crate::adapter::PluginDetails::serialize")]
    pub details: P,
}

impl<P: PluginDetails> DataDisplay for InfoOutput<P> {
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
        )
    }
}

impl Subcommand for Info {
    async fn run(&self, session: &IoSession, manifest: &Manifest) -> anyhow::Result<()> {
        let manifest_name = &self.plugin_name;

        let Some(plugin_manifest) = manifest.plugin.get(manifest_name) else {
            return Err(PluginNotFoundError(self.plugin_name.clone()).into());
        };

        match plugin_manifest {
            PluginDownloadSpec::Spiget(spiget_plugin_manifest) => {
                let spiget_plugin_details = session
                    .spiget_api()
                    .resource_details(spiget_plugin_manifest.resource_id)
                    .await?;

                let spiget_plugin =
                    SpigetPlugin::new(&session, spiget_plugin_manifest.resource_id).await?;

                let details = PluginDetails {
                    manifest_name: manifest_name.clone(),
                    page_url: spiget_plugin.plugin_page(),
                    plugin_type: PluginApiType::Spiget,
                };

                Ok(InfoOutput { details })
            }
            _ => todo!(),
        }
    }
}
