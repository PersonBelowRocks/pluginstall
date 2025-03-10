//! The 'list' subcommand for listing versions of a plugin.

use clap::Args;

use crate::{manifest::Manifest, output::DataDisplay, session::Session};

/// The 'list' subcommand.
#[derive(Args, Debug, Clone)]
pub struct List {
    #[arg(
        value_name = "PLUGIN_NAME",
        help = "The name of the plugin in the manifest file."
    )]
    pub plugin_name: String,
    #[arg(
        short = 'L',
        long,
        value_name = "LIMIT",
        default_value = "10",
        help = "The number of versions to list."
    )]
    pub limit: u64,
}

/// The output of the list command. Written to stdout with [`DataDisplay`].
#[derive(Debug)]
pub struct ListOutput {
    versions: Vec<()>,
}

impl DataDisplay for ListOutput {
    fn write_json(&self, w: &mut impl std::io::Write) -> Result<(), std::io::Error> {
        todo!()
    }

    fn write_hr(&self, w: &mut impl std::io::Write) -> Result<(), std::io::Error> {
        todo!()
    }
}

impl List {
    /// Run the list command.
    #[inline]
    pub fn run(&self, session: &Session, manifest: &Manifest) -> anyhow::Result<ListOutput> {
        todo!()
    }
}
