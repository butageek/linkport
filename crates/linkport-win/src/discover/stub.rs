//! Non-Windows stub.

#[derive(Debug, Clone, serde::Serialize)]
pub struct DiscoveredBrowser {
    pub name: String,
    pub command: String,
}

pub fn discover_browsers() -> Vec<DiscoveredBrowser> {
    Vec::new()
}
