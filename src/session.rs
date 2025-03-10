//! Logic for the HTTP(S) session (and communication) with the various plugin APIs.

use rq::{Request, Url};
use spiget_endpoints::{sub_resource_id, SPIGET_API_RESOURCE_DETAILS};

use crate::spiget_plugin::ResourceId;
use std::path::PathBuf;

/// A session that can be used to talk to various plugin APIs.
pub struct Session {
    rq_client: rq::Client
}

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Reqwest error: {0}")]
    ReqwestError(#[from] rq::Error),
    #[error("URL parse error: {0}")]
    UrlParseError(#[from] url::ParseError),
}

/// The user agent to be used by pluginstall when talking to APIs.
pub static USER_AGENT: &str = "pluginstall";

/// The base URL for the Spiget API.
pub static SPIGET_API_BASE_URL: &str = "https://api.spiget.org/v2";
/// The base URL for the Hangar API.
pub static HANGAR_API_BASE_URL: &str = "https://hangar.papermc.io/api/v1";

pub(crate) mod spiget_endpoints {
    use super::*;

    /// Substitute a `resource_id` string with the given resource ID.
    #[inline]
    pub fn sub_resource_id(base: &str, resource_id: ResourceId) -> String {
        let resource_id_str = u64::from(resource_id).to_string();
        let subbed = base.replace("resource_id", &resource_id_str);

        subbed
    }

    /// Endpoint for getting the details of a resource from the Spiget API.
    pub static SPIGET_API_RESOURCE_DETAILS: &str = "resources/{resource_id}";
}

impl Session {
    /// Creates a new API session.
    pub fn new() -> Self {
        Self {
            rq_client: rq::Client::new(),
        }
    }

    /// Create a new request for the given URL, with various default options.
    fn new_request(&self, url: Url, method: rq::Method) -> rq::RequestBuilder {
        let mut req = self.rq_client.request(method, url);
        
        todo!()

    }

    /// Get a plugin from the Spiget API with the given resource ID.
    #[inline]
    pub async fn get_spiget_plugin(&self, resource_id: ResourceId) -> Result<String, Error> {
        let subbed = sub_resource_id(SPIGET_API_RESOURCE_DETAILS, resource_id);
        let mut url = Url::parse(SPIGET_API_BASE_URL)?.join(&subbed)?;

        let req = self.new_request(url, rq::Method::GET);

        todo!()
    }
}