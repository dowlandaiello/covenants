use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::{Attribute, Binary, Coin, StdError, StdResult, Uint128, Uint64};
use neutron_sdk::{
    bindings::{msg::IbcFee, types::ProtobufAny}, query::min_ibc_fee::MinIbcFeeResponse, NeutronResult
};
use prost::Message;

#[cw_serde]
pub struct OpenAckVersion {
    pub version: String,
    pub controller_connection_id: String,
    pub host_connection_id: String,
    pub address: String,
    pub encoding: String,
    pub tx_type: String,
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

#[cw_serde]
pub struct RemoteChainInfo {
    /// connection id from neutron to the remote chain on which
    /// we wish to open an ICA
    pub connection_id: String,
    pub channel_id: String,
    pub denom: String,
    pub ibc_transfer_timeout: Uint64,
    pub ica_timeout: Uint64,
    pub ibc_fee: IbcFee,
}

impl RemoteChainInfo {
    pub fn get_response_attributes(&self) -> Vec<Attribute> {
        let recv_fee = coin_vec_to_string(&self.ibc_fee.recv_fee);
        let ack_fee = coin_vec_to_string(&self.ibc_fee.ack_fee);
        let timeout_fee = coin_vec_to_string(&self.ibc_fee.timeout_fee);

        vec![
            Attribute::new("connection_id", &self.connection_id),
            Attribute::new("channel_id", &self.channel_id),
            Attribute::new("denom", &self.denom),
            Attribute::new(
                "ibc_transfer_timeout",
                self.ibc_transfer_timeout.to_string(),
            ),
            Attribute::new("ica_timeout", self.ica_timeout.to_string()),
            Attribute::new("ibc_recv_fee", recv_fee),
            Attribute::new("ibc_ack_fee", ack_fee),
            Attribute::new("ibc_timeout_fee", timeout_fee),
        ]
    }

    pub fn validate(self) -> Result<RemoteChainInfo, StdError> {
        if self.ibc_fee.ack_fee.is_empty()
            || self.ibc_fee.timeout_fee.is_empty()
            || !self.ibc_fee.recv_fee.is_empty()
        {
            return Err(StdError::generic_err("invalid IbcFee".to_string()));
        }

        Ok(self)
    }
}

fn coin_vec_to_string(coins: &Vec<Coin>) -> String {
    let mut str = "".to_string();
    if coins.is_empty() {
        str.push_str("[]");
    } else {
        for coin in coins {
            str.push_str(&coin.to_string());
        }
    }
    str.to_string()
}

pub fn get_proto_coin(
    denom: String,
    amount: Uint128,
) -> cosmos_sdk_proto::cosmos::base::v1beta1::Coin {
    cosmos_sdk_proto::cosmos::base::v1beta1::Coin {
        denom,
        amount: amount.to_string(),
    }
}

#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    /// Returns the associated remote chain information
    #[returns(Option<String>)]
    DepositAddress {},
}

#[cw_serde]
#[derive(QueryResponses)]
pub enum CovenantQueryMsg {
    /// Returns the associated remote chain information
    #[returns(Option<String>)]
    DepositAddress {},
}

/// helper that serializes a MsgTransfer to protobuf
pub fn to_proto_msg_transfer(msg: impl Message) -> NeutronResult<ProtobufAny> {
    // Serialize the Transfer message
    let mut buf = Vec::with_capacity(msg.encoded_len());
    if let Err(e) = msg.encode(&mut buf) {
        return Err(StdError::generic_err(format!("Encode error: {e}")).into());
    }

    Ok(ProtobufAny {
        type_url: "/ibc.applications.transfer.v1.MsgTransfer".to_string(),
        value: Binary::from(buf),
    })
}

pub fn to_proto_msg_send(msg: impl Message) -> NeutronResult<ProtobufAny> {
    // Serialize the Send message
    let mut buf = Vec::with_capacity(msg.encoded_len());
    if let Err(e) = msg.encode(&mut buf) {
        return Err(StdError::generic_err(format!("Encode error: {e}")).into());
    }

    Ok(ProtobufAny {
        type_url: "/cosmos.bank.v1beta1.MsgSend".to_string(),
        value: Binary::from(buf),
    })
}

pub fn to_proto_msg_multi_send(msg: impl Message) -> NeutronResult<ProtobufAny> {
    // Serialize the Send message
    let mut buf = Vec::with_capacity(msg.encoded_len());
    if let Err(e) = msg.encode(&mut buf) {
        return Err(StdError::generic_err(format!("Encode error: {e}")).into());
    }

    Ok(ProtobufAny {
        type_url: "/cosmos.bank.v1beta1.MsgMultiSend".to_string(),
        value: Binary::from(buf),
    })
}

pub fn default_ibc_ack_fee_amount() -> Uint128 {
    Uint128::new(100000)
}

pub fn default_ibc_timeout_fee_amount() -> Uint128 {
    Uint128::new(100000)
}

pub fn default_ibc_fee() -> IbcFee {
    IbcFee {
        // must be empty
        recv_fee: vec![],
        ack_fee: vec![cosmwasm_std::Coin {
            denom: "untrn".to_string(),
            amount: default_ibc_ack_fee_amount(),
        }],
        timeout_fee: vec![cosmwasm_std::Coin {
            denom: "untrn".to_string(),
            amount: default_ibc_timeout_fee_amount(),
        }],
    }
}

pub fn get_default_ibc_fee_requirement() -> Uint128 {
    default_ibc_ack_fee_amount() + default_ibc_timeout_fee_amount()
}

pub fn get_default_ica_fee() -> Coin {
    Coin {
        denom: "untrn".to_string(),
        amount: Uint128::new(1000000),
    }
}

pub fn get_ibc_fee_total_amount(min_fee_query_response: MinIbcFeeResponse) -> Uint128 {
    let ack_fee_total: Uint128 = min_fee_query_response.min_fee.ack_fee.iter().map(|c| c.amount).sum();
    let recv_fee_total: Uint128 = min_fee_query_response.min_fee.recv_fee.iter().map(|c| c.amount).sum();
    let timeout_fee_total: Uint128 = min_fee_query_response.min_fee.timeout_fee.iter().map(|c| c.amount).sum();
    ack_fee_total + recv_fee_total + timeout_fee_total
}