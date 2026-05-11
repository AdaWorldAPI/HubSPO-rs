//! Company object (ontological definition)

use crate::properties::Property;

/// Core Company object in HubSpot
#[derive(Debug, Clone)]
pub struct Company {
    pub id: String,
    pub properties: Vec<Property>,
    // Associations are modeled separately for flexibility
}

impl Company {
    pub fn new(id: &str) -> Self {
        Self {
            id: id.to_string(),
            properties: vec![],
        }
    }
}