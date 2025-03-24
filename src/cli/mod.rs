//! CLI interface logic

mod versions;
pub use versions::*;

mod info;
pub use info::*;

mod download;
pub use download::*;

use crate::adapter::VersionSpec;

/// An error that indicates a specified plugin name could not be found in the manifest.
#[derive(thiserror::Error, Debug, Clone)]
#[error("Could not find a plugin with the name '{0}' in the manifest.")]
pub struct PluginNotFoundError(pub String);

use crate::caching::{default_cache_directory_path, CacheResult, DownloadCache};
use crate::cli;
use crate::manifest::{Manifest, ManifestResult, DEFAULT_MANIFEST_FILE_NAME};
use crate::output::CliOutput;
use crate::session::IoSession;
use std::borrow::Cow;
use std::path::PathBuf;

/// The CLI command with its parameters, parsed from the arguments provided to the process.
#[derive(clap::Parser, Clone, Debug)]
#[command(version, about, long_about = None)]
pub struct Cli {
    /// The path to the manifest file.
    #[arg(
        short,
        long,
        value_name = "MANIFEST_FILE",
        default_value = DEFAULT_MANIFEST_FILE_NAME
    )]
    pub manifest: PathBuf,

    /// Path to the download cache. Downloaded plugins will be cached in a subdirectory
    /// in the download cache with the name of the manifest used.
    ///
    /// By default the download cache that will be used is `$HOME/.pluginstall_cache`.
    /// If no cache directory is provided and the default cache directory doesn't exist,
    /// then the default cache directory will be created.
    #[arg(long, value_name = "CACHE_PATH")]
    pub cache: Option<PathBuf>,

    /// Output control arguments
    #[clap(flatten)]
    pub output_ctrl: OutputCtrlArgs,

    /// The subcommand
    #[command(subcommand)]
    pub command: Commands,
}

/// Arguments for controlling CLI output.
#[derive(clap::Args, Debug, Clone)]
pub struct OutputCtrlArgs {
    /// Use JSON output instead of human readable output.
    #[arg(long, action=clap::ArgAction::SetTrue)]
    pub json: bool,

    /// Don't write a newline at the end of the command output.
    #[arg(long, action=clap::ArgAction::SetTrue)]
    pub no_newline: bool,
}

/// Version specification arguments. If no argument is provided, then the latest version is specified.
#[derive(clap::Args, Debug, Clone)]
#[group(required = false, multiple = false)]
pub struct VersionSpecArgs {
    /// The name of a version to search for.
    /// If multiple versions have the same name, the latest version with that name will be chosen.
    ///
    /// If neither the version name, or version identifier are specified, then the latest version will be used.
    #[arg(long, short = 'V', value_name = "VERSION_NAME")]
    pub version_name: Option<String>,
    /// The unique version identifier of a version.
    #[arg(long, short = 'I', value_name = "VERSION_IDENTIFIER")]
    pub version_ident: Option<String>,
}

/// Arguments for specifying a specific plugin.
#[derive(clap::Args, Debug, Clone)]
#[group(required = true)]
pub struct PluginSpecArgs {
    /// The name of the plugin in the manifest file.
    /// Download strategy for the is specified in the manifest file under this key.
    #[arg(value_name = "PLUGIN_NAME")]
    pub plugin_name: String,
}

#[derive(clap::Subcommand, Clone, Debug)]
pub enum Commands {
    /// List all versions of a plugin.
    Versions(cli::Versions),
    /// Show info about a plugin.
    Info(cli::Info),
    /// Download a plugin.
    Download(cli::Download),
}

macro_rules! run_subcommand {
    ($commands:expr, $variant:ident, $session:expr, $manifest:expr) => {
        if let Commands::$variant(cmd) = $commands {
            cmd.run($session, $manifest).await?;
        }
    };
}

impl Commands {
    /// Run the subcommand.
    #[inline]
    pub async fn run(&self, session: &IoSession, manifest: &Manifest) -> miette::Result<()> {
        run_subcommand!(self, Versions, session, manifest);
        run_subcommand!(self, Info, session, manifest);
        run_subcommand!(self, Download, session, manifest);

        Ok(())
    }
}

/// Trait implemented by subcommands.
pub trait Subcommand {
    async fn run(&self, session: &IoSession, manifest: &Manifest) -> miette::Result<()>;
}

impl Cli {
    /// Parse the manifest file specified by the options passed to this CLI.
    /// If no manifest file is specified, this will parse the default manifest file.
    #[inline]
    pub async fn manifest(&self) -> ManifestResult<Manifest> {
        Manifest::parse_from_file(&self.manifest).await
    }

    /// Create a [`CliOutput`] object using the output options provided to the CLI.
    #[must_use]
    #[inline]
    pub fn cli_output(&self) -> CliOutput {
        CliOutput::new(self.output_ctrl.json, !self.output_ctrl.no_newline)
    }

    /// Create a [`DownloadCache`] object with the options provided to the CLI and the name of the manifest used.
    ///
    /// The provided `manifest_name` should come from the deserialized manifest file.
    ///
    /// If no special cache path is provided then the default cache in the user's home directory will be used.
    #[must_use]
    #[inline]
    pub async fn download_cache(&self, manifest_name: &str) -> CacheResult<DownloadCache> {
        let path = match &self.cache {
            None => {
                let path = default_cache_directory_path()?.join(manifest_name);
                Cow::Owned(path)
            }
            // if the user has specified a cache path, we just trust them that it exists and error later
            Some(cache) => Cow::Borrowed(cache),
        };

        DownloadCache::new(&path).await
    }
}

impl VersionSpecArgs {
    /// Get the version spec provided to the command.
    ///
    /// Will return [`VersionSpec::Latest`] if neither were specified.
    /// Will panic if both the version name and version identifier are specified.
    #[inline]
    pub fn get(&self) -> VersionSpec {
        match (self.version_ident.as_ref(), self.version_name.as_ref()) {
            (Some(version_ident), None) => VersionSpec::Identifier(version_ident.clone()),
            (None, Some(version_name)) => VersionSpec::Name(version_name.clone()),
            (None, None) => VersionSpec::Latest,
            _ => panic!("You cannot specify both version identifier and version name."),
        }
    }
}
