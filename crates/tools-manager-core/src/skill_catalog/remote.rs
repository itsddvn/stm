use std::{io::Read, time::Duration};

use reqwest::{blocking::Client, redirect::Policy};
use url::Url;

use super::{
    CatalogRemote, SkillCatalogError, CATALOG_MANIFEST_URL, CATALOG_PAYLOAD_URL,
    CATALOG_SIGNATURE_URL,
};

#[derive(Clone)]
pub struct FixedHttpsCatalogRemote {
    client: Client,
}

impl FixedHttpsCatalogRemote {
    pub fn new() -> Result<Self, SkillCatalogError> {
        let origin = Url::parse(CATALOG_MANIFEST_URL)
            .map_err(|error| SkillCatalogError::Transport(error.to_string()))?;
        let policy = Policy::custom(move |attempt| {
            let target = attempt.url();
            let same_origin = target.scheme() == origin.scheme()
                && target.host_str() == origin.host_str()
                && target.port_or_known_default() == origin.port_or_known_default();
            if !same_origin {
                return attempt.error("catalog redirect crossed the fixed origin");
            }
            if attempt.previous().len() >= 3 {
                return attempt.error("catalog redirect limit exceeded");
            }
            attempt.follow()
        });
        let client = Client::builder()
            .https_only(true)
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(15))
            .redirect(policy)
            .build()
            .map_err(|error| SkillCatalogError::Transport(error.to_string()))?;
        Ok(Self { client })
    }
}

impl CatalogRemote for FixedHttpsCatalogRemote {
    fn fetch(&self, url: &'static str, maximum_bytes: usize) -> Result<Vec<u8>, SkillCatalogError> {
        let resource = match url {
            CATALOG_MANIFEST_URL => "manifest",
            CATALOG_SIGNATURE_URL => "signature",
            CATALOG_PAYLOAD_URL => "catalog",
            _ => {
                return Err(SkillCatalogError::Transport(
                    "catalog URL is outside the compiled allowlist".to_string(),
                ))
            }
        };
        let mut response = self
            .client
            .get(url)
            .send()
            .map_err(|error| SkillCatalogError::Transport(error.to_string()))?;
        if !response.status().is_success() {
            return Err(SkillCatalogError::Transport(format!(
                "{resource} returned HTTP {}",
                response.status().as_u16()
            )));
        }
        if response
            .content_length()
            .is_some_and(|length| length > maximum_bytes as u64)
        {
            return Err(SkillCatalogError::ResponseTooLarge {
                resource,
                limit: maximum_bytes,
            });
        }
        let mut bytes = Vec::with_capacity(
            response
                .content_length()
                .unwrap_or_default()
                .min(maximum_bytes as u64) as usize,
        );
        response
            .by_ref()
            .take(maximum_bytes as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|error| SkillCatalogError::Transport(error.to_string()))?;
        if bytes.len() > maximum_bytes {
            return Err(SkillCatalogError::ResponseTooLarge {
                resource,
                limit: maximum_bytes,
            });
        }
        Ok(bytes)
    }
}
