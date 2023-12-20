use std::collections::BTreeSet;

use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::{to_json_binary, Addr, Binary, StdError, WasmMsg};
use covenant_macros::{clocked, covenant_clock_address};
use covenant_utils::ReceiverConfig;

#[cw_serde]
pub struct InstantiateMsg {
    /// address for the clock. this contract verifies
    /// that only the clock can execute ticks
    pub clock_address: String,
    /// config that determines whether router should
    /// route over ibc or natively
    pub receiver_config: ReceiverConfig,
    /// specified denoms to route
    pub denoms: BTreeSet<String>,
}

#[cw_serde]
pub struct PresetInterchainRouterFields {
    /// config that determines whether router should
    /// route over ibc or natively
    pub receiver_config: ReceiverConfig,
    /// specified denoms to route
    pub denoms: BTreeSet<String>,
    pub label: String,
    pub code_id: u64,
}

impl PresetInterchainRouterFields {
    pub fn to_instantiate_msg(&self, clock_address: String) -> InstantiateMsg {
        InstantiateMsg {
            clock_address,
            receiver_config: self.receiver_config.clone(),
            denoms: self.denoms.clone(),
        }
    }

    pub fn to_instantiate2_msg(
        &self,
        admin_addr: String,
        salt: Binary,
        clock_address: String,
    ) -> Result<WasmMsg, StdError> {
        let instantiate_msg = self.to_instantiate_msg(clock_address);
        Ok(WasmMsg::Instantiate2 {
            admin: Some(admin_addr),
            code_id: self.code_id,
            label: self.label.to_string(),
            msg: to_json_binary(&instantiate_msg)?,
            funds: vec![],
            salt,
        })
    }
}

#[clocked]
#[cw_serde]
pub enum ExecuteMsg {
    DistributeFallback { denoms: Vec<String> },
}

#[covenant_clock_address]
#[derive(QueryResponses)]
#[cw_serde]
pub enum QueryMsg {
    #[returns(ReceiverConfig)]
    ReceiverConfig {},
    #[returns(BTreeSet<String>)]
    TargetDenoms {},
}

#[cw_serde]
pub enum MigrateMsg {
    UpdateConfig {
        clock_addr: Option<String>,
        receiver_config: Option<ReceiverConfig>,
        target_denoms: Option<Vec<String>>,
    },
    UpdateCodeId {
        data: Option<Binary>,
    },
}
