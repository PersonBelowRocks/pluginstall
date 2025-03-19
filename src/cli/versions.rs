//! The 'versions' subcommand for listing versions of a plugin.

use clap::Args;
use miette::IntoDiagnostic;
use owo_colors::AnsiColors;

use crate::{
    adapter::{
        spiget::{SpigetPlugin, SpigetResourceDetails},
        PluginDetails, PluginVersion,
    },
    cli::Subcommand,
    manifest::{Manifest, PluginDownloadSpec},
    output::DataDisplay,
    session::IoSession,
    util::{CliTable, CliTableRow},
};

use super::PluginSpecArgs;

/// The 'versions' subcommand.
#[derive(Args, Debug, Clone)]
pub struct Versions {
    #[command(flatten)]
    pub plugin: PluginSpecArgs,
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
        let mut headers = if self.cfg.write_download_urls {
            CliTableRow::new(&[
                "Version Name".into(),
                "Version Date".into(),
                "Version Identifier".into(),
                "Download URL".into(),
            ])
        } else {
            CliTableRow::new(&[
                "Version Name".into(),
                "Version Date".into(),
                "Version Identifier".into(),
            ])
        };

        headers.color_all(AnsiColors::Green);

        let mut table = CliTable::new(headers);

        for version in self.versions {
            let datetime_str = version
                .publish_date()
                .map(|d| d.format(&self.cfg.strftime_format).to_string());

            let mut row_cell_text = vec![
                version.version_name().to_string(),
                datetime_str.as_deref().unwrap_or("").to_string(),
                version.version_identifier().to_string(),
            ];

            // include download URL if requested
            if self.cfg.write_download_urls {
                row_cell_text.push(version.download_url().to_string());
            }

            let mut row = CliTableRow::new(&row_cell_text);
            row[0].color = AnsiColors::Green;

            table.add(row);
        }

        writeln!(w, "{table}")?;

        Ok(())
    }
}

impl Subcommand for Versions {
    /// Run the versions command.
    #[inline]
    async fn run(&self, session: &IoSession, manifest: &Manifest) -> miette::Result<()> {
        let plugin_manifest = manifest.plugin(&self.plugin.plugin_name)?;

        match plugin_manifest {
            PluginDownloadSpec::Spiget(spiget_plugin_manifest) => {
                let spiget_plugin =
                    SpigetPlugin::new(&session, spiget_plugin_manifest.resource_id).await?;

                let versions = spiget_plugin
                    .iter_versions()
                    .take(self.limit as _)
                    .collect::<Vec<_>>();

                let output = VersionsOutput {
                    cfg: VersionsOutputCfg {
                        strftime_format: self.time_format.clone(),
                        write_download_urls: self.download_url,
                    },
                    details: SpigetResourceDetails::new(
                        spiget_plugin.resource_id(),
                        &self.plugin.plugin_name,
                    ),
                    versions: &*versions,
                };

                session.cli_output().display(&output).into_diagnostic()?;
            }

            _ => todo!(),
        };

        Ok(())
    }
}
