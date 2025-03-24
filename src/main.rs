extern crate derive_more as dm;
extern crate reqwest as rq;

use std::process::ExitCode;

use crate::cli::Cli;
use clap::Parser;
use miette::IntoDiagnostic;
use session::IoSession;

mod adapter;
mod caching;
mod cli;
mod error;
mod manifest;
mod output;
mod session;
mod util;

fn main() -> miette::Result<()> {
    util::setup_logger();

    // start the async runtime and block
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();

    runtime.block_on(async_main())
}

/// The async entrypoint of the app. The main function will block here when the app is ran.
async fn async_main() -> miette::Result<()> {
    let cli = Cli::parse();

    let manifest = cli.manifest().await.into_diagnostic()?;

    let cli_output = cli.cli_output();
    let download_cache = cli
        .download_cache(&manifest.meta.manifest_name)
        .await
        .into_diagnostic()?;
    let session = IoSession::new(cli_output, download_cache);

    cli.command.run(&session, &manifest).await
}
