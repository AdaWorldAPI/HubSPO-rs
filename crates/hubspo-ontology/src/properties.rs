//! Property modeling (strongly typed + history aware)

#[derive(Debug, Clone)]
pub struct Property {
    pub name: String,
    pub value: String,
    pub source: Option<String>,
    pub timestamp_ms: Option<u64>,
}