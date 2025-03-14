//! CLI interface logic

use crate::manifest::{Manifest, ManifestResult, DEFAULT_MANIFEST_FILE_NAME};
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
    pub fn get_manifest_path(&self) -> ManifestResult<Cow<'_, Path>> {
        self.manifest.as_ref().map_or_else(
            || {
                let mut dir = current_dir()?;
                dir.push(DEFAULT_MANIFEST_FILE_NAME);
                Ok(Cow::<Path>::Owned(dir))
            },
            |manifest| Ok(Cow::Borrowed(manifest.as_ref())),
        )
    }
}
