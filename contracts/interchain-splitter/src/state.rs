use cosmwasm_std::Addr;
use cw_storage_plus::{Item, Map};

use crate::msg::SplitConfig;

/// clock module address to verify the sender of incoming ticks
pub const CLOCK_ADDRESS: Item<Addr> = Item::new("clock_address");

// maps a denom string to a vec of SplitReceivers
pub const SPLIT_CONFIG_MAP: Map<String, SplitConfig> = Map::new("split_config");

