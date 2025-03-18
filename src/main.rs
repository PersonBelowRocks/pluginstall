extern crate derive_more as dm;
extern crate reqwest as rq;

use std::process::ExitCode;

use crate::cli::Cli;
use clap::Parser;
use session::IoSession;

mod adapter;
mod caching;
mod cli;
mod manifest;
mod output;
mod session;
mod subcommands;
mod util;

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

    log::debug!("CLI args = {cli:#?}");

    let manifest = cli.manifest().await?;

    log::debug!("manifest = {manifest:#?}");

    let cli_output = cli.cli_output();
    let download_cache = cli.download_cache(&manifest.meta.manifest_name).await?;
    let session = IoSession::new(cli_output, download_cache);

    let exit_code = cli.command.run(&session, &manifest).await;

    Ok(exit_code)
}
