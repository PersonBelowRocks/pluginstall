//! Logic for plugins downloaded from Paper's hangar using the Hangar API.

#[derive(serde::Deserialize, Clone, Debug)]
pub struct HangarPlugin {
    slug: HangarSlug
}

/// Describes a project on Hangar.
#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Deserialize, dm::Into, dm::From)]
pub struct HangarSlug(String);