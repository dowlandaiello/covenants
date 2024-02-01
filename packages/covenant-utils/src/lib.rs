use std::collections::BTreeMap;

use cosmwasm_schema::cw_serde;
use cosmwasm_std::{
    to_json_string, Addr, Attribute, BankMsg, BlockInfo, Coin, CosmosMsg, Decimal, IbcMsg,
    IbcTimeout, StdError, StdResult, Timestamp, Uint128, Uint64,
};
use neutron::{default_ibc_ack_fee_amount, default_ibc_fee, default_ibc_timeout_fee_amount};
use neutron_sdk::{bindings::msg::NeutronMsg, sudo::msg::RequestPacketTimeoutHeight};

pub mod astroport;
pub mod deadline;
pub mod instantiate2_helper;
pub mod liquid_pooler_withdraw;
pub mod neutron;
pub mod polytone;
pub mod split;
pub mod withdraw_lp_helper;
pub mod osmo_outpost;

#[cw_serde]
pub enum ReceiverConfig {
    /// party expects to receive funds on the same chain
    Native(Addr),
    /// party expects to receive funds on a remote chain
    Ibc(DestinationConfig),
}

impl ReceiverConfig {
    pub fn get_response_attributes(self, party: String) -> Vec<Attribute> {
        match self {
            ReceiverConfig::Native(addr) => {
                vec![Attribute::new("receiver_config_native_addr", addr)]
            }
            ReceiverConfig::Ibc(destination_config) => destination_config
                .get_response_attributes()
                .into_iter()
                .map(|mut a| {
                    a.key = party.to_string() + &a.key;
                    a
                })
                .collect(),
        }
    }
}

#[cw_serde]
pub struct CovenantParty {
    /// authorized address of the party
    pub addr: String,
    /// denom provided by the party
    pub native_denom: String,
    /// information about receiver address
    pub receiver_config: ReceiverConfig,
}

impl CovenantParty {
    pub fn get_refund_msg(self, amount: Uint128, block: &BlockInfo) -> CosmosMsg {
        match self.receiver_config {
            ReceiverConfig::Native(addr) => CosmosMsg::Bank(BankMsg::Send {
                to_address: addr.to_string(),
                amount: vec![cosmwasm_std::Coin {
                    denom: self.native_denom,
                    amount,
                }],
            }),
            ReceiverConfig::Ibc(destination_config) => CosmosMsg::Ibc(IbcMsg::Transfer {
                channel_id: destination_config.local_to_destination_chain_channel_id,
                to_address: self.addr.to_string(),
                amount: cosmwasm_std::Coin {
                    denom: self.native_denom,
                    amount,
                },
                timeout: IbcTimeout::with_timestamp(
                    block
                        .time
                        .plus_seconds(destination_config.ibc_transfer_timeout.u64()),
                ),
            }),
        }
    }
}

#[cw_serde]
pub struct CovenantPartiesConfig {
    pub party_a: CovenantParty,
    pub party_b: CovenantParty,
}

impl CovenantPartiesConfig {
    pub fn get_response_attributes(self) -> Vec<Attribute> {
        let mut attrs = vec![
            Attribute::new("party_a_address", self.party_a.addr),
            Attribute::new("party_a_ibc_denom", self.party_a.native_denom),
            Attribute::new("party_b_address", self.party_b.addr),
            Attribute::new("party_b_ibc_denom", self.party_b.native_denom),
        ];
        attrs.extend(
            self.party_a
                .receiver_config
                .get_response_attributes("party_a_".to_string()),
        );
        attrs.extend(
            self.party_b
                .receiver_config
                .get_response_attributes("party_b_".to_string()),
        );
        attrs
    }

    pub fn match_caller_party(&self, caller: String) -> Result<CovenantParty, StdError> {
        let a = self.clone().party_a;
        let b = self.clone().party_b;
        if a.addr == caller {
            Ok(a)
        } else if b.addr == caller {
            Ok(b)
        } else {
            Err(StdError::generic_err("unauthorized"))
        }
    }
}

#[cw_serde]
pub enum CovenantTerms {
    TokenSwap(SwapCovenantTerms),
}

#[cw_serde]
pub struct SwapCovenantTerms {
    pub party_a_amount: Uint128,
    pub party_b_amount: Uint128,
}

#[cw_serde]
pub struct PolCovenantTerms {
    pub party_a_amount: Uint128,
    pub party_b_amount: Uint128,
}

impl CovenantTerms {
    pub fn get_response_attributes(self) -> Vec<Attribute> {
        match self {
            CovenantTerms::TokenSwap(terms) => {
                let attrs = vec![
                    Attribute::new("covenant_terms", "token_swap"),
                    Attribute::new("party_a_amount", terms.party_a_amount),
                    Attribute::new("party_b_amount", terms.party_b_amount),
                ];
                attrs
            }
        }
    }
}

#[cw_serde]
pub struct DestinationConfig {
    /// channel id of the destination chain
    pub local_to_destination_chain_channel_id: String,
    /// address of the receiver on destination chain
    pub destination_receiver_addr: String,
    /// timeout in seconds
    pub ibc_transfer_timeout: Uint64,
    /// pfm configurations for denoms
    pub denom_to_pfm_map: BTreeMap<String, PacketForwardMiddlewareConfig>,
}

#[cw_serde]
pub struct PacketForwardMiddlewareConfig {
    pub local_to_hop_chain_channel_id: String,
    pub hop_to_destination_chain_channel_id: String,
    pub hop_chain_receiver_address: String,
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

// https://github.com/strangelove-ventures/packet-forward-middleware/blob/main/router/types/forward.go
#[cw_serde]
pub struct PacketMetadata {
    pub forward: Option<ForwardMetadata>,
}

#[cw_serde]
pub struct ForwardMetadata {
    pub receiver: String,
    pub port: String,
    pub channel: String,
}

impl DestinationConfig {
    pub fn get_ibc_transfer_messages_for_coins(
        &self,
        coins: Vec<Coin>,
        current_timestamp: Timestamp,
        sender_address: String,
    ) -> StdResult<Vec<CosmosMsg<NeutronMsg>>> {
        let mut messages: Vec<CosmosMsg<NeutronMsg>> = vec![];
        // we get the number of target denoms we have to reserve
        // neutron fees for
        let count = Uint128::from(1 + coins.len() as u128);

        for coin in coins {
            let send_coin = if coin.denom != "untrn" {
                Some(coin)
            } else {
                // if its neutron we're distributing we need to keep a
                // reserve for ibc gas costs.
                // this is safe because we pass target denoms.
                let reserve_amount = count * get_default_ibc_fee_requirement();
                if coin.amount > reserve_amount {
                    Some(Coin {
                        denom: coin.denom,
                        amount: coin.amount - reserve_amount,
                    })
                } else {
                    None
                }
            };

            if let Some(c) = send_coin {
                match self.denom_to_pfm_map.get(&c.denom) {
                    Some(pfm_config) => {
                        messages.push(CosmosMsg::Custom(NeutronMsg::IbcTransfer {
                            source_port: "transfer".to_string(),
                            // local chain to hop chain channel
                            source_channel: pfm_config.local_to_hop_chain_channel_id.to_string(),
                            token: c.clone(),
                            sender: sender_address.to_string(),
                            receiver: pfm_config.hop_chain_receiver_address.to_string(),
                            timeout_height: RequestPacketTimeoutHeight {
                                revision_number: None,
                                revision_height: None,
                            },
                            timeout_timestamp: current_timestamp
                                .plus_seconds(self.ibc_transfer_timeout.u64())
                                .nanos(),
                            memo: to_json_string(&PacketMetadata {
                                forward: Some(ForwardMetadata {
                                    receiver: self.destination_receiver_addr.to_string(),
                                    port: "transfer".to_string(),
                                    // hop chain to final receiver chain channel
                                    channel: pfm_config
                                        .hop_to_destination_chain_channel_id
                                        .to_string(),
                                }),
                            })?,
                            fee: default_ibc_fee(),
                        }))
                    }
                    None => {
                        messages.push(CosmosMsg::Custom(NeutronMsg::IbcTransfer {
                            source_port: "transfer".to_string(),
                            source_channel: self.local_to_destination_chain_channel_id.to_string(),
                            token: c.clone(),
                            sender: sender_address.to_string(),
                            receiver: self.destination_receiver_addr.to_string(),
                            timeout_height: RequestPacketTimeoutHeight {
                                revision_number: None,
                                revision_height: None,
                            },
                            timeout_timestamp: current_timestamp
                                .plus_seconds(self.ibc_transfer_timeout.u64())
                                .nanos(),
                            memo: format!("ibc_distribution: {:?}:{:?}", c.denom, c.amount,)
                                .to_string(),
                            fee: default_ibc_fee(),
                        }));
                    }
                }
            }
        }

        Ok(messages)
    }

    pub fn get_response_attributes(&self) -> Vec<Attribute> {
        vec![
            Attribute::new(
                "local_to_destination_chain_channel_id",
                self.local_to_destination_chain_channel_id.to_string(),
            ),
            Attribute::new(
                "destination_receiver_addr",
                self.destination_receiver_addr.to_string(),
            ),
            Attribute::new("ibc_transfer_timeout", self.ibc_transfer_timeout),
        ]
    }
}

#[cw_serde]
pub struct PfmUnwindingConfig {
    // keys: relevant denoms IBC'd to neutron
    // values: channel ids to facilitate ibc unwinding to party chain
    pub party_1_pfm_map: BTreeMap<String, PacketForwardMiddlewareConfig>,
    pub party_2_pfm_map: BTreeMap<String, PacketForwardMiddlewareConfig>,
}

/// single side lp limits define the highest amount (in `Uint128`) that
/// we consider acceptable to provide single-sided.
/// if asset balance exceeds these limits, double-sided liquidity should be provided.
#[cw_serde]
pub struct SingleSideLpLimits {
    pub asset_a_limit: Uint128,
    pub asset_b_limit: Uint128,
}

/// config for the pool price expectations upon covenant instantiation
#[cw_serde]
pub struct PoolPriceConfig {
    pub expected_spot_price: Decimal,
    pub acceptable_price_spread: Decimal,
}
