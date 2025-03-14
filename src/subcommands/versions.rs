//! The 'versions' subcommand for listing versions of a plugin.
use std::ops::Deref;

use clap::Args;

use crate::{
    adapter::{
        spiget::{SpigetPlugin, SpigetResourceDetails},
        PluginApiType, PluginDetails, PluginVersion,
    },
    cli::Subcommand,
    manifest::{Manifest, PluginDownloadSpec},
    output::DataDisplay,
    session::IoSession,
    util::{CliTable, CliTableFormatting},
};

use super::PluginNotFoundError;

/// The 'versions' subcommand.
#[derive(Args, Debug, Clone)]
pub struct Versions {
    #[arg(
        value_name = "PLUGIN_NAME",
        help = "The name of the plugin in the manifest file."
    )]
    pub plugin_name: String,
    #[arg(
        short = 'L',
        long,
        value_name = "LIMIT",
        default_value = "10",
        help = "The number of versions to list."
    )]
    pub limit: u64,
    #[arg(
        short = 'd',
        long,
        action = clap::ArgAction::SetTrue,
        help = "Output the download URL for the versions in human-readable mode."
    )]
    pub download_url: bool,
    #[arg(
        short = 'F',
        long,
        value_name = "TIME_FORMAT",
        default_value = "%Y-%m-%d",
        help = "The strftime/strptime format string for the release date of the versions."
    )]
    pub time_format: String,
}

/// The output of the list command. Written to stdout with [`DataDisplay`].
#[derive(Debug, serde::Serialize)]
pub struct VersionsOutput<'a, P: PluginDetails, V: PluginVersion> {
    /// The format that datetimes should be written as when writing in human-readable mode.
    #[serde(skip)]
    pub cfg: VersionsOutputCfg,
    #[serde(serialize_with = "crate::adapter::PluginDetails::serialize")]
    pub details: P,
    #[serde(serialize_with = "crate::adapter::PluginVersion::serialize_slice")]
    pub versions: &'a [V],
}

/// Options for how data should be formatted to the terminal.
#[derive(Debug)]
pub struct VersionsOutputCfg {
    /// The datetime format
    pub strftime_format: String,
    /// Whether download URLs for versions should be written
    pub write_download_urls: bool,
}

impl<'a, P: PluginDetails, V: PluginVersion> DataDisplay for VersionsOutput<'a, P, V> {
    fn write_json(&self, w: &mut impl std::io::Write) -> Result<(), std::io::Error> {
        let json_string = serde_json::to_string(self).unwrap();
        write!(w, "{json_string}")
    }

    fn write_hr(&self, w: &mut impl std::io::Write) -> Result<(), std::io::Error> {
        if self.cfg.write_download_urls {
            // this table has 4 columns since the download URL is included
            let mut table = CliTable::new([
                "Version Name",
                "Version Date",
                "Version Identifier",
                "Download URL",
            ]);

            for version in self.versions {
                let datetime_str = version
                    .publish_date()
                    .map(|d| d.format(&self.cfg.strftime_format).to_string());

                table.push([
                    version.version_name().to_string(),
                    datetime_str.as_deref().unwrap_or("---").to_string(),
                    version.version_identifier().to_string(),
                    version.download_url().to_string(),
                ]);
            }

            table.write(
                w,
                &CliTableFormatting {
                    write_headers: true,
                    equal_field_width: true,
                    ..Default::default()
                },
            )?;
        } else {
            // this table only has 3 columns since the download URL is excluded
            let mut table = CliTable::new(["Version Name", "Version Date", "Version Identifier"]);

            for version in self.versions {
                let datetime_str = version
                    .publish_date()
                    .map(|d| d.format(&self.cfg.strftime_format).to_string());

                table.push([
                    version.version_name().to_string(),
                    datetime_str.as_deref().unwrap_or("---").to_string(),
                    version.version_identifier().to_string(),
                ]);
            }

            table.write(
                w,
                &CliTableFormatting {
                    write_headers: true,
                    equal_field_width: true,
                    ..Default::default()
                },
            )?;
        }

        Ok(())
    }
}

impl Subcommand for Versions {
    /// Run the versions command.
    #[inline]
    async fn run(&self, session: &IoSession, manifest: &Manifest) -> anyhow::Result<()> {
        let manifest_name = &self.plugin_name;

        let Some(plugin_manifest) = manifest.plugin.get(manifest_name) else {
            return Err(PluginNotFoundError(self.plugin_name.clone()).into());
        };

        match plugin_manifest {
            PluginDownloadSpec::Spiget(spiget_plugin_manifest) => {
                let spiget_plugin =
                    SpigetPlugin::new(&session, spiget_plugin_manifest.resource_id).await?;

                let versions = spiget_plugin.versions(self.limit).await?;

                let output = VersionsOutput {
                    cfg: VersionsOutputCfg {
                        strftime_format: self.time_format.clone(),
                        write_download_urls: self.download_url,
                    },
                    details: SpigetResourceDetails::new(spiget_plugin.resource_id(), manifest_name),
                    versions: &*versions,
                };

                session.cli_output().display(&output)?;
            }

            _ => todo!(),
        };

        Ok(())
    }
}
