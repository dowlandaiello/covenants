use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::{Addr, Binary, Uint128, Uint64};
use covenant_clock_derive::clocked;
use neutron_sdk::bindings::msg::IbcFee;

#[cw_serde]
pub struct InstantiateMsg {
    /// Address for the clock. This contract verifies
    /// that only the clock can execute Ticks
    pub clock_address: String,
    /// IBC transfer channel on Stride for Neutron
    /// This is used to IBC transfer stuatom on Stride
    /// to the LP contract
    pub stride_neutron_ibc_transfer_channel_id: String,
    /// IBC connection ID on Neutron for Stride
    /// We make an Interchain Account over this connection
    pub neutron_stride_ibc_connection_id: String,
    /// Address for the covenant's LP contract.
    /// We send the liquid staked amount to this address
    pub lp_address: String,
    /// The liquid staked denom (e.g., stuatom). This is
    /// required because we only allow transfers of this denom
    /// out of the LSer
    pub ls_denom: String,
    /// Neutron requires fees to be set to refund relayers for
    /// submission of ack and timeout messages.
    /// recv_fee and ack_fee paid in untrn from this contract
    pub ibc_fee: IbcFee,
    /// Time in seconds for ICA SubmitTX messages from Neutron
    /// Note that ICA uses ordered channels, a timeout implies
    /// channel closed. We can reopen the channel by reregistering
    /// the ICA with the same port id and connection id
    pub ica_timeout: Uint64,
    /// Timeout in seconds. This is used to craft a timeout timestamp
    /// that will be attached to the IBC transfer message from the ICA
    /// on the host chain (Stride) to its destination. Typically
    /// this timeout should be greater than the ICA timeout, otherwise
    /// if the ICA times out, the destination chain receiving the funds
    /// will also receive the IBC packet with an expired timestamp.
    pub ibc_transfer_timeout: Uint64,
}

#[cw_serde]
pub struct PresetLsFields {
    pub ls_code: u64,
    pub label: String,
    pub ls_denom: String,
    pub stride_neutron_ibc_transfer_channel_id: String,
    pub neutron_stride_ibc_connection_id: String,
}

impl PresetLsFields {
    pub fn to_instantiate_msg(
        self,
        clock_address: String,
        lp_address: String,
        ibc_fee: IbcFee,
        ica_timeout: Uint64,
        ibc_transfer_timeout: Uint64,
    ) -> InstantiateMsg {
        InstantiateMsg {
            clock_address,
            stride_neutron_ibc_transfer_channel_id: self.stride_neutron_ibc_transfer_channel_id,
            neutron_stride_ibc_connection_id: self.neutron_stride_ibc_connection_id,
            lp_address,
            ls_denom: self.ls_denom,
            ibc_fee,
            ica_timeout,
            ibc_transfer_timeout,
        }
    }
}

#[clocked]
#[cw_serde]
pub enum ExecuteMsg {
    /// The transfer message allows anybody to permissionlessly
    /// transfer a specified amount of tokens of the preset ls_denom
    /// from the ICA of the host chain to the preset lp_address
    Transfer { amount: Uint128 },
}

#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    #[returns(Addr)]
    ClockAddress {},
    #[returns(Addr)]
    StrideICA {},
    #[returns(Addr)]
    LpAddress {},
    #[returns(ContractState)]
    ContractState {},
    #[returns(String)]
    StrideNeutronIbcTransferChannelId {},
    #[returns(String)]
    NeutronStrideIbcConnectionId {},
    #[returns(IbcFee)]
    IbcFee {},
    #[returns(Uint64)]
    IcaTimeout {},
    #[returns(Uint64)]
    IbcTransferTimeout {},
    #[returns(String)]
    LsDenom {},
}

#[cw_serde]
pub enum MigrateMsg {
    UpdateConfig {
        clock_addr: Option<String>,
        stride_neutron_ibc_transfer_channel_id: Option<String>,
        lp_address: Option<String>,
        neutron_stride_ibc_connection_id: Option<String>,
        ls_denom: Option<String>,
        ibc_fee: Option<IbcFee>,
        ibc_transfer_timeout: Option<Uint64>,
        ica_timeout: Option<Uint64>,
    },
    UpdateCodeId {
        data: Option<Binary>,
    },
}

#[cw_serde]
pub struct OpenAckVersion {
    pub version: String,
    pub controller_connection_id: String,
    pub host_connection_id: String,
    pub address: String,
    pub encoding: String,
    pub tx_type: String,
}

#[cw_serde]
pub enum ContractState {
    Instantiated,
    ICACreated,
}

/// SudoPayload is a type that stores information about a transaction that we try to execute
/// on the host chain. This is a type introduced for our convenience.
#[cw_serde]
pub struct SudoPayload {
    pub message: String,
    pub port_id: String,
}

/// Serves for storing acknowledgement calls for interchain transactions
#[cw_serde]
pub enum AcknowledgementResult {
    /// Success - Got success acknowledgement in sudo with array of message item types in it
    Success(Vec<String>),
    /// Error - Got error acknowledgement in sudo with payload message in it and error details
    Error((String, String)),
    /// Timeout - Got timeout acknowledgement in sudo with payload message in it
    Timeout(String),
}
