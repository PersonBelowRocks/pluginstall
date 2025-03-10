extern crate derive_more as dm;
extern crate reqwest as rq;

use crate::cli::Cli;
use crate::manifest::{Manifest, DEFAULT_MANIFEST_FILE_NAME};
use clap::Parser;
use log::*;
use std::env::current_dir;
use std::path::PathBuf;

mod cli;
mod manifest;
mod session;
mod util;
// These modules contain the "adapter" logic for downloading from various different sources.
mod hangar_plugin;
mod jenkins_plugin;
mod spiget_plugin;

fn main() -> anyhow::Result<()> {
    util::setup_logger();

    // start the async runtime and block
    smol::block_on(async_main())
}

/// The async entrypoint of the app. The main function will block here when the app is ran.
async fn async_main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    debug!("CLI args = {cli:#?}");

    let manifest_path = cli.get_manifest_path()?;
    let manifest = Manifest::parse_from_file(manifest_path.as_ref()).await?;

    debug!("manifest = {manifest:#?}");

    Ok(())
}
