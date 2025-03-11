//! CLI interface logic

use crate::manifest::{Manifest, ManifestResult, DEFAULT_MANIFEST_FILE_NAME};
use crate::output::{DataDisplay, OutputManager};
use crate::session::Session;
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
    List(subcommands::List),
}

macro_rules! run_subcommand {
    ($commands:expr, $variant:ident, $session:expr, $manifest:expr, $output_mgr:expr) => {
        if let Commands::$variant(cmd) = $commands {
            match cmd.run($session, $manifest).await {
                Ok(output) => {
                    $output_mgr.display(output).unwrap();

                    return ExitCode::SUCCESS;
                }
                Err(error) => {
                    let std_err = AsRef::<dyn std::error::Error>::as_ref(&error);
                    $output_mgr.error(std_err).unwrap();

                    return ExitCode::FAILURE;
                }
            }
        }
    };
}

impl Commands {
    /// Run the subcommand.
    #[inline]
    pub async fn run(
        &self,
        session: &Session,
        manifest: &Manifest,
        output_manager: &OutputManager,
    ) -> ExitCode {
        run_subcommand!(self, List, session, manifest, output_manager);

        unreachable!();
    }
}

/// Trait implemented by subcommands.
pub trait Subcommand {
    /// The output type that should be displayed to the user.
    type Output: DataDisplay;

    async fn run(&self, session: &Session, manifest: &Manifest) -> anyhow::Result<Self::Output>;
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
