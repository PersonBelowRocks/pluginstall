extern crate derive_more as dm;
extern crate reqwest as rq;

use crate::cli::Cli;
use crate::manifest::{Manifest, DEFAULT_MANIFEST_FILE_NAME};
use clap::Parser;
use cli::Commands;
use log::*;
use manifest::PluginDownloadSpec;
use output::OutputManager;
use session::Session;
use spiget_plugin::SpigetPlugin;

mod cli;
mod manifest;
mod output;
mod session;
mod subcommands;
mod util;
// These modules contain the "adapter" logic for downloading from various different sources.
mod hangar_plugin;
mod jenkins_plugin;
mod spiget_plugin;

fn main() -> anyhow::Result<()> {
    util::setup_logger();

    // start the async runtime and block
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();

    runtime.block_on(async_main())
}

/// The async entrypoint of the app. The main function will block here when the app is ran.
async fn async_main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    debug!("CLI args = {cli:#?}");

    let manifest_path = cli.get_manifest_path()?;
    let manifest = Manifest::parse_from_file(manifest_path.as_ref()).await?;

    debug!("manifest = {manifest:#?}");

    let session = Session::new();
    let output_manager = OutputManager::new(cli.json);

    match cli.command {
        Commands::List(cmd) => {
            let output = cmd.run(&session, &manifest)?;
            output_manager.display(output)?;
        }
    }

    for (plugin_name, plugin) in manifest.plugin {
        debug!("plugin name = '{plugin_name}'");

        if let PluginDownloadSpec::Spiget(manifest_plugin_entry) = plugin {
            debug!(
                "getting a plugin with resource ID {:?} from Spiget",
                manifest_plugin_entry.resource_id
            );

            let plugin = SpigetPlugin::new(&session, manifest_plugin_entry.resource_id).await?;

            debug!("spiget plugin = {plugin:#?}");
        }
    }

    Ok(())
}
