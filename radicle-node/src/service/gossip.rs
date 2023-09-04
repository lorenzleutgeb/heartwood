pub mod store;

use super::*;

pub use store::Error;
pub use store::GossipStore as Store;

pub fn node(config: &Config, timestamp: Timestamp) -> NodeAnnouncement {
    let features = config.features();
    let alias = config.alias.clone();
    let addresses: BoundedVec<_, ADDRESS_LIMIT> = config
        .external_addresses
        .clone()
        .try_into()
        .expect("external addresses are within the limit");
    let relays = if addresses.is_empty() {
        BoundedVec::collect_from(
            config
                .connect
                .iter()
                .map(|connect_address| connect_address.node_id),
        )
    } else {
        BoundedVec::new()
    };

    NodeAnnouncement {
        features,
        timestamp,
        alias,
        addresses,
        relays,
        nonce: 0,
    }
}

pub fn inventory(timestamp: Timestamp, inventory: Vec<Id>) -> InventoryAnnouncement {
    type Inventory = BoundedVec<Id, INVENTORY_LIMIT>;

    if inventory.len() > Inventory::max() {
        error!(
            target: "service",
            "inventory announcement limit ({}) exceeded, other nodes will see only some of your projects",
            inventory.len()
        );
    }

    InventoryAnnouncement {
        inventory: BoundedVec::truncate(inventory),
        timestamp,
    }
}
