//! CLI interface logic

use crate::manifest::{ManifestResult, DEFAULT_MANIFEST_FILE_NAME};
use std::borrow::Cow;
use std::env::current_dir;
use std::path::{Path, PathBuf};

/// The CLI command with its parameters, parsed from the arguments provided to the process.
#[derive(clap::Parser, Clone, Debug)]
#[command(version, about, long_about = None)]
pub struct Cli {
    #[arg(short, long, value_name = "MANIFEST_FILE", help = include_str!("doc/manifest.arg.md"))]
    #[doc = include_str!("doc/manifest.arg.md")]
    pub manifest: Option<PathBuf>,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(clap::Subcommand, Clone, Debug)]
pub enum Commands {
    #[command(about = include_str!("doc/manifest.arg.md"))]
    #[doc = include_str!("doc/manifest.arg.md")]
    List {
        #[arg(short = 'U', long, action=clap::ArgAction::SetTrue, help = include_str!("doc/cmd.list.upgradable.arg.md"))]
        #[doc = include_str!("doc/cmd.list.upgradable.arg.md")]
        upgradable: bool,
    },
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
