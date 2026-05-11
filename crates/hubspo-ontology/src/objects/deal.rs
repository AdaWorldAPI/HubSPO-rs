//! Deal object (ontological definition)

use crate::properties::Property;

#[derive(Debug, Clone)]
pub struct Deal {
    pub id: String,
    pub properties: Vec<Property>,
}

impl Deal {
    pub fn new(id: &str) -> Self {
        Self {
            id: id.to_string(),
            properties: vec![],
        }
    }
}