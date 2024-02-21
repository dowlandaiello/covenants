use std::collections::BTreeMap;

use cosmwasm_schema::cw_serde;
use cosmwasm_std::{
    Attribute, BankMsg, Coin, CosmosMsg, Decimal, Fraction, StdError, StdResult, Uint128,
};

#[cw_serde]
pub struct SplitConfig {
    /// map receiver address to its share of the split
    pub receivers: BTreeMap<String, Decimal>,
}

impl SplitConfig {
    pub fn remap_receivers_to_routers(
        &self,
        // vector of (old_addr, new_addr) tuples to remap
        remaps: Vec<(String, String)>,
    ) -> Result<SplitConfig, StdError> {
        let mut new_receivers = BTreeMap::new();

        for (old, new) in remaps {
            match self.receivers.get(&old) {
                Some(val) => new_receivers.insert(new, *val),
                None => return Err(StdError::not_found(format!("receiver {old:?} not found in split config: {:?}", self.receivers))),
            };
        }

        Ok(SplitConfig {
            receivers: new_receivers,
        })
    }

    pub fn validate(&self, party_a: &str, party_b: &str) -> Result<(), StdError> {
        let share_a = match self.receivers.get(party_a) {
            Some(val) => *val,
            None => return Err(StdError::not_found(party_a)),
        };
        let share_b = match self.receivers.get(party_b) {
            Some(val) => *val,
            None => return Err(StdError::not_found(party_b)),
        };

        if share_a + share_b != Decimal::one() {
            return Err(StdError::generic_err(
                "shares must add up to 1.0".to_string(),
            ));
        }

        Ok(())
    }

    /// Validate that all shares are added to one
    pub fn validate_shares(&self) -> Result<(), StdError> {
        let mut total_shares = Decimal::zero();

        for (_, share) in self.receivers.clone() {
            total_shares += share;
        }

        if total_shares != Decimal::one() {
            return Err(StdError::generic_err(
                "shares must add up to 1.0".to_string(),
            ));
        }

        Ok(())
    }

    pub fn get_transfer_messages(
        &self,
        amount: Uint128,
        denom: String,
        filter_addr: Option<String>,
    ) -> Result<Vec<CosmosMsg>, StdError> {
        let msgs: Result<Vec<CosmosMsg>, StdError> = self
            .receivers
            .iter()
            .map(|(addr, share)| {
                // if we are filtering for a single receiver,
                // then we wish to transfer only to that receiver.
                // we thus set receiver share to 1.0, as the
                // entitlement already takes that into account.
                match &filter_addr {
                    Some(filter) => {
                        if filter == addr {
                            (addr, Decimal::one())
                        } else {
                            (addr, Decimal::zero())
                        }
                    }
                    None => (addr, *share),
                }
            })
            .filter(|(_, share)| !share.is_zero())
            .map(|(addr, share)| {
                let entitlement = amount
                    .checked_multiply_ratio(share.numerator(), share.denominator())
                    .map_err(|_| StdError::generic_err("failed to checked_multiply".to_string()))?;

                let amount = Coin {
                    denom: denom.to_string(),
                    amount: entitlement,
                };

                Ok(CosmosMsg::Bank(BankMsg::Send {
                    to_address: addr.to_string(),
                    amount: vec![amount],
                }))
            })
            .collect();

        msgs
    }

    pub fn get_response_attribute(&self, denom: String) -> Attribute {
        let mut receivers = "[".to_string();
        self.receivers.iter().for_each(|(receiver, share)| {
            receivers.push('(');
            receivers.push_str(receiver);
            receivers.push(':');
            receivers.push_str(&share.to_string());
            receivers.push_str("),");
        });
        receivers.push(']');
        Attribute::new(denom, receivers)
    }
}

pub fn remap_splits(
    splits: BTreeMap<String, SplitConfig>,
    (party_a_receiver, party_a_router): (String, String),
    (party_b_receiver, party_b_router): (String, String),
) -> StdResult<BTreeMap<String, SplitConfig>> {
    let mut remapped_splits: BTreeMap<String, SplitConfig> = BTreeMap::new();

    let remap_vector = vec![
        (party_a_receiver.clone(), party_a_router.clone()),
        (party_b_receiver.clone(), party_b_router.clone()),
    ];

    for (denom, split) in splits.iter() {
        let remapped_split = split.remap_receivers_to_routers(remap_vector.clone())?;
        remapped_splits.insert(denom.to_string(), remapped_split);
    }

    Ok(remapped_splits)
}
