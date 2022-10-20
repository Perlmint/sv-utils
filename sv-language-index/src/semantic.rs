use crate::{ItemId, Range};

#[derive(Debug)]
pub enum Item {
    ModuleIdentifier {
        module_name: String,
        location: Range,
    },
    ModuleInstance {
        module_name: ItemId,
        instance_name: ItemId,
        parameters: Vec<ItemId>,
        ports: Vec<ItemId>,
        location: Range,
    },
    UnknownIdentifier {
        name: String,
        location: Range,
    },
}

impl Item {
    pub fn location(&self) -> &Range {
        match self {
            Item::ModuleIdentifier { location, .. } => location,
            Item::ModuleInstance { location, .. } => location,
            Item::UnknownIdentifier { location, .. } => location,
        }
    }
}
