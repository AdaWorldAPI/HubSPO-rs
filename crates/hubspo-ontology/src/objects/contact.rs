//! Contact object (ontological definition)

use crate::properties::Property;

#[derive(Debug, Clone)]
pub struct Contact {
    pub id: String,
    pub properties: Vec<Property>,
}

impl Contact {
    pub fn new(id: &str) -> Self {
        Self {
            id: id.to_string(),
            properties: vec![],
        }
    }
}