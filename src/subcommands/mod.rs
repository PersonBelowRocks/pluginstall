mod versions;
pub use versions::*;

mod info;
pub use info::*;

mod download;
pub use download::*;

/// An error that indicates a specified plugin name could not be found in the manifest.
#[derive(thiserror::Error, Debug, Clone)]
#[error("Could not find a plugin with the name '{0}' in the manifest.")]
pub struct PluginNotFoundError(pub String);
