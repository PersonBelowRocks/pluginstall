//! CLI interface logic

use crate::manifest::{ManifestResult, DEFAULT_MANIFEST_FILE_NAME};
use crate::subcommands;
use std::borrow::Cow;
use std::env::current_dir;
use std::path::{Path, PathBuf};

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

    /// The subcommand
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(clap::Subcommand, Clone, Debug)]
pub enum Commands {
    #[command(about = "List all versions of a plugin.")]
    List(subcommands::List),
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
