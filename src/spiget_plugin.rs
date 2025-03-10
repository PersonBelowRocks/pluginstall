//! Logic for plugins downloaded from spiget.

#[derive(serde::Deserialize, Clone, Debug)]
pub struct SpigetPlugin {
    resource_id: ResourceId,
}

/// A resource ID for a Spigot resource (a plugin basically).
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, dm::Into, dm::From, serde::Deserialize)]
pub struct ResourceId(u64);

impl ResourceId {
    
}