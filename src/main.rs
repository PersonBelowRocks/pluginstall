extern crate derive_more as dm;
extern crate reqwest as rq;

use std::process::ExitCode;

use crate::cli::Cli;
use crate::manifest::Manifest;
use clap::Parser;
use log::*;
use output::OutputManager;
use session::Session;

mod cli;
mod manifest;
mod output;
mod session;
mod subcommands;
mod util;
// These modules contain the "adapter" logic for downloading from various different sources.
mod adapter;

fn main() -> ExitCode {
    util::setup_logger();

    // start the async runtime and block
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();

    runtime.block_on(async_main()).unwrap()
}

/// The async entrypoint of the app. The main function will block here when the app is ran.
async fn async_main() -> anyhow::Result<ExitCode> {
    let cli = Cli::parse();

    debug!("CLI args = {cli:#?}");

    let manifest_path = cli.get_manifest_path()?;
    let manifest = Manifest::parse_from_file(manifest_path.as_ref()).await?;

    debug!("manifest = {manifest:#?}");

    let session = Session::new();
    let output_manager = OutputManager::new(cli.json, !cli.no_newline);

    let exit_code = cli.command.run(&session, &manifest, &output_manager).await;

    Ok(exit_code)
}
