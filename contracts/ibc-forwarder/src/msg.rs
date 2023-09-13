use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::{Addr, Attribute, Uint128, Uint64};
use covenant_macros::{
    clocked, covenant_clock_address, covenant_deposit_address, covenant_ica_address,
    covenant_remote_chain,
};
use covenant_utils::neutron_ica::RemoteChainInfo;
use neutron_sdk::bindings::msg::IbcFee;

#[cw_serde]
pub struct InstantiateMsg {
    /// address for the clock. this contract verifies
    /// that only the clock can execute ticks
    pub clock_address: String,
    /// contract responsible for providing the address to forward the
    /// funds to
    pub next_contract: String,

    pub remote_chain_connection_id: String,
    pub remote_chain_channel_id: String,
    pub denom: String,
    pub amount: Uint128,

    /// neutron requires fees to be set to refund relayers for
    /// submission of ack and timeout messages.
    /// recv_fee and ack_fee paid in untrn from this contract
    pub ibc_fee: IbcFee,
    /// timeout in seconds. this is used to craft a timeout timestamp
    /// that will be attached to the IBC transfer message from the ICA
    /// on the host chain to its destination. typically this timeout
    /// should be greater than the ICA timeout, otherwise if the ICA
    /// times out, the destination chain receiving the funds will also
    /// receive the IBC packet with an expired timestamp.
    pub ibc_transfer_timeout: Uint64,
    /// time in seconds for ICA SubmitTX messages from neutron
    /// note that ICA uses ordered channels, a timeout implies
    /// channel closed. We can reopen the channel by reregistering
    /// the ICA with the same port id and connection id
    pub ica_timeout: Uint64,
}

#[cw_serde]
pub struct PresetIbcForwarderFields {
    pub remote_chain_connection_id: String,
    pub remote_chain_channel_id: String,
    pub denom: String,
    pub amount: Uint128,
}

impl PresetIbcForwarderFields {
    pub fn to_instantiate_msg(
        self,
        clock_address: String,
        next_contract: String,
        ibc_fee: IbcFee,
        ibc_transfer_timeout: Uint64,
        ica_timeout: Uint64,
    ) -> InstantiateMsg {
        InstantiateMsg {
            clock_address,
            next_contract,
            remote_chain_connection_id: self.remote_chain_connection_id,
            remote_chain_channel_id: self.remote_chain_channel_id,
            denom: self.denom,
            amount: self.amount,
            ibc_fee,
            ibc_transfer_timeout,
            ica_timeout,
        }
    }
}

impl InstantiateMsg {
    pub fn get_response_attributes(&self) -> Vec<Attribute> {
        vec![
            Attribute::new("clock_address", &self.clock_address),
            Attribute::new(
                "remote_chain_connection_id",
                &self.remote_chain_connection_id,
            ),
            Attribute::new("remote_chain_channel_id", &self.remote_chain_channel_id),
            Attribute::new("remote_chain_denom", &self.denom),
            Attribute::new("remote_chain_amount", &self.amount.to_string()),
            Attribute::new(
                "ibc_transfer_timeout",
                self.ibc_transfer_timeout.to_string(),
            ),
            Attribute::new("ica_timeout", self.ica_timeout.to_string()),
        ]
    }
}

#[clocked]
#[cw_serde]
pub enum ExecuteMsg {}

#[covenant_deposit_address]
#[covenant_remote_chain]
#[covenant_clock_address]
#[covenant_ica_address]
#[derive(QueryResponses)]
#[cw_serde]
pub enum QueryMsg {
    #[returns(ContractState)]
    ContractState {},
}

#[cw_serde]
pub enum ContractState {
    /// Contract was instantiated, ready create ica
    Instantiated,
    /// ICA was created, funds are ready to be forwarded
    IcaCreated,
    /// forwarder is complete
    Complete,
}

/// SudoPayload is a type that stores information about a transaction that we try to execute
/// on the host chain. This is a type introduced for our convenience.
#[cw_serde]
pub struct SudoPayload {
    pub message: String,
    pub port_id: String,
}
