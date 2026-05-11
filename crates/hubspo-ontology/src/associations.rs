//! Association modeling between objects

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AssociationType {
    CompanyToContact,
    CompanyToDeal,
    ContactToDeal,
    DealToLineItem,
    // ... more as discovered
}

#[derive(Debug, Clone)]
pub struct Association {
    pub from_type: String,
    pub from_id: String,
    pub to_type: String,
    pub to_id: String,
    pub association_type: AssociationType,
}