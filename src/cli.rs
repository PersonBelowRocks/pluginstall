//! CLI interface logic

use crate::caching::{create_cache, default_cache_directory_path, CacheResult, DownloadCache};
use crate::manifest::{Manifest, ManifestResult, DEFAULT_MANIFEST_FILE_NAME};
use crate::output::CliOutput;
use crate::session::IoSession;
use crate::subcommands;
use std::borrow::Cow;
use std::env::current_dir;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

/// The CLI command with its parameters, parsed from the arguments provided to the process.
#[derive(clap::Parser, Clone, Debug)]
#[command(version, about, long_about = None)]
pub struct Cli {
    /// The path to the manifest file.
    #[arg(
        short,
        long,
        value_name = "MANIFEST_FILE",
        help = "The path to the plugin manifest file."
    )]
    pub manifest: Option<PathBuf>,

    /// Path to the download cache. Downloaded plugins will be cached in a subdirectory
    /// in the download cache with the name of the manifest used.
    ///
    /// By default the download cache that will be used is `$HOME/.pluginstall_cache`.
    /// If no cache directory is provided and the default cache directory doesn't exist,
    /// then the default cache directory will be created.
    #[arg(long, value_name = "CACHE_PATH")]
    pub cache: Option<PathBuf>,

    #[arg(long, action=clap::ArgAction::SetTrue, help = "Use JSON output instead of human readable output.")]
    pub json: bool,

    #[arg(long, action=clap::ArgAction::SetTrue, help = "Don't write a newline at the end of the command output.")]
    pub no_newline: bool,

    /// The subcommand
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(clap::Subcommand, Clone, Debug)]
pub enum Commands {
    #[command(about = "List all versions of a plugin.")]
    Versions(subcommands::Versions),
    #[command(about = "Show info about a plugin.")]
    Info(subcommands::Info),
    #[command(about = "Download a plugin.")]
    Download(subcommands::Download),
}

macro_rules! run_subcommand {
    ($commands:expr, $variant:ident, $session:expr, $manifest:expr) => {
        if let Commands::$variant(cmd) = $commands {
            match cmd.run($session, $manifest).await {
                Ok(()) => return ExitCode::SUCCESS,
                Err(error) => {
                    // let std_err = AsRef::<dyn std::error::Error>::as_ref(&error);
                    log::error!("{}", error);

                    return ExitCode::FAILURE;
                }
            }
        }
    };
}

impl Commands {
    /// Run the subcommand.
    #[inline]
    pub async fn run(&self, session: &IoSession, manifest: &Manifest) -> ExitCode {
        run_subcommand!(self, Versions, session, manifest);
        run_subcommand!(self, Info, session, manifest);
        run_subcommand!(self, Download, session, manifest);

        unreachable!();
    }
}

/// Trait implemented by subcommands.
pub trait Subcommand {
    async fn run(&self, session: &IoSession, manifest: &Manifest) -> anyhow::Result<()>;
}

impl Cli {
    /// Get the path to the manifest file. Returns an error if the current working directory is invalid.
    #[inline]
    fn manifest_file_path(&self) -> ManifestResult<Cow<'_, Path>> {
        self.manifest.as_ref().map_or_else(
            || {
                let mut dir = current_dir()?;
                dir.push(DEFAULT_MANIFEST_FILE_NAME);
                Ok(Cow::<Path>::Owned(dir))
            },
            |manifest| Ok(Cow::Borrowed(manifest.as_ref())),
        )
    }

    /// Parse the manifest file specified by the options passed to this CLI.
    /// If no manifest file is specified, this will parse the default manifest file.
    #[inline]
    pub async fn manifest(&self) -> ManifestResult<Manifest> {
        Manifest::parse_from_file(self.manifest_file_path()?.as_ref()).await
    }

    /// Create a [`CliOutput`] object using the output options provided to the CLI.
    #[must_use]
    #[inline]
    pub fn cli_output(&self) -> CliOutput {
        CliOutput::new(self.json, !self.no_newline)
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
                // create the default cache directory and its manifest subdirectory if they don't exist
                create_cache(&path).await?;
                Cow::Owned(path)
            }
            // if the user has specified a cache path, we just trust them that it exists and error later
            Some(cache) => Cow::Borrowed(cache),
        };

        DownloadCache::new(&path).await
    }
}
