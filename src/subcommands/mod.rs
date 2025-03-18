mod versions;
pub use versions::*;

mod info;
pub use info::*;

mod download;
pub use download::*;

use crate::adapter::VersionSpec;

/// An error that indicates a specified plugin name could not be found in the manifest.
#[derive(thiserror::Error, Debug, Clone)]
#[error("Could not find a plugin with the name '{0}' in the manifest.")]
pub struct PluginNotFoundError(pub String);

/// Version specification arguments. If no argument is provided, then the latest version is specified.
#[derive(clap::Args, Debug, Clone)]
#[group(required = false, multiple = false)]
pub struct VersionSpecArgs {
    /// The name of a version to search for.
    /// If multiple versions have the same name, the latest version will be chosen.
    #[arg(long, short = 'V', value_name = "VERSION_NAME")]
    pub version_name: Option<String>,
    /// The unique version identifier of a version.
    #[arg(long, short = 'I', value_name = "VERSION_IDENTIFIER")]
    pub version_ident: Option<String>,
}

impl VersionSpecArgs {
    /// Get the version spec provided to the command.
    ///
    /// Will return [`VersionSpec::Latest`] if neither were specified.
    /// Will panic if both the version name and version identifier are specified.
    #[inline]
    pub fn get_version_spec(&self) -> VersionSpec {
        match (self.version_ident.as_ref(), self.version_name.as_ref()) {
            (Some(version_ident), None) => VersionSpec::Identifier(version_ident.clone()),
            (None, Some(version_name)) => VersionSpec::Name(version_name.clone()),
            (None, None) => VersionSpec::Latest,
            _ => panic!("You cannot specify both version identifier and version name."),
        }
    }
}
