//! The 'download' subcommand for downloading a plugin in the manifest.

use std::path::{Path, PathBuf};

use clap::Args;
use miette::{bail, Context, IntoDiagnostic};
use owo_colors::{AnsiColors, OwoColorize};

use crate::{
    adapter::{spiget::SpigetPlugin, PluginApiType, VersionSpec},
    cli::Subcommand,
    error::diagnostics,
    manifest::{Manifest, PluginDownloadSpec},
    output::DataDisplay,
    session::{DownloadReport, DownloadSpec, IoSession},
};

use super::{PluginSpecArgs, VersionSpecArgs};

/// The 'download' subcommand.
#[derive(Args, Debug, Clone)]
pub struct Download {
    #[command(flatten)]
    pub plugin: PluginSpecArgs,
    #[command(flatten)]
    pub version: VersionSpecArgs,
    /// The directory to download the file into. By default the file will be downloaded into the working directory.
    #[arg(short = 'o', long, value_name = "PATH")]
    pub out_dir: Option<PathBuf>,
}

/// The output of the 'download' subcommand.
#[derive(Debug, serde::Serialize)]
pub struct DownloadOutput {
    pub report: DownloadReport,
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
        writeln!(
            w,
            "Downloaded plugin to '{}'",
            self.download_path.to_string_lossy().green()
        )?;

        let cached = if self.report.cached {
            "cached".color(AnsiColors::Green)
        } else {
            "not cached".color(AnsiColors::Yellow)
        };

        let download_size = pretty_bytes::converter::convert(self.report.download_size as _);

        write!(w, "Download size: {0} ({1})", download_size.green(), cached)?;

        Ok(())
    }
}

impl Subcommand for Download {
    async fn run(&self, session: &IoSession, manifest: &Manifest) -> miette::Result<()> {
        let plugin_manifest = manifest.plugin(&self.plugin.plugin_name)?;

        match plugin_manifest {
            PluginDownloadSpec::Hangar(_) => todo!(),
            PluginDownloadSpec::Jenkins => todo!(),
            PluginDownloadSpec::Spiget(spiget) => {
                let plugin = SpigetPlugin::new(session, spiget.resource_id).await?;
                let version_spec = self.version.get();

                let out_dir = match &self.out_dir {
                    None => Path::new(".").to_path_buf(), // by default download to working directory
                    Some(path) => path.clone(),
                };

                // ensure the path is an existing directory
                if !out_dir.exists() || !out_dir.is_dir() {
                    bail!(diagnostics::invalid_download_dir(&out_dir));
                }

                let Some(version) = plugin.version_from_spec(&version_spec)? else {
                    bail!(diagnostics::version_not_found(
                        &self.plugin.plugin_name,
                        &version_spec
                    ));
                };

                let report = session
                    .download_plugin(
                        DownloadSpec {
                            plugin_name: &self.plugin.plugin_name,
                            version: &version,
                            api_type: PluginApiType::Spiget,
                        },
                        &out_dir,
                    )
                    .await
                    .wrap_err("Error downloading Spiget plugin")?;

                let out = DownloadOutput {
                    report,
                    download_path: out_dir,
                };

                session.cli_output().display(&out).into_diagnostic()?;
            }
        }

        Ok(())
    }
}
