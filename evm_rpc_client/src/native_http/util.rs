use serde_json::Value;
use url::Host;

pub fn hex_to_bytes(hex: &str) -> Option<Vec<u8>> {
    if !hex.starts_with("0x") {
        return None;
    }
    hex::decode(&hex[2..]).ok()
}

pub fn canonicalize_json(text: &[u8]) -> Option<Vec<u8>> {
    let json = serde_json::from_slice::<Value>(text).ok()?;
    serde_json::to_vec(&json).ok()
}

pub fn hostname_from_url(url: &str) -> Option<String> {
    url::Url::parse(url).ok().and_then(|url| match url.host() {
        Some(Host::Domain(domain)) => {
            if !domain.contains(['{', '}']) {
                Some(domain.to_string())
            } else {
                None
            }
        }
        _ => None,
    })
}
