use std::collections::BTreeSet;

#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    to_json_binary, Binary, CanonicalAddr, Deps, DepsMut, Env, MessageInfo, Response, StdError,
    StdResult, WasmMsg,
};
use covenant_two_party_pol_holder::msg::{RagequitConfig, TwoPartyPolCovenantConfig};
use covenant_utils::{instantiate2_helper::get_instantiate2_salt_and_address, split::remap_splits};
use cw2::set_contract_version;

use crate::{
    error::ContractError,
    msg::{
        InstantiateMsg, LiquidPoolerMigrateMsg, MigrateMsg, QueryMsg,
        },
    state::{
        CONTRACT_CODES, COVENANT_CLOCK_ADDR, COVENANT_POL_HOLDER_ADDR, LIQUID_POOLER_ADDR,
    },
};

const CONTRACT_NAME: &str = "crates.io:counterparty-discovery-covenant-poc";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

pub const CLOCK_SALT: &[u8] = b"clock";
pub const HOLDER_SALT: &[u8] = b"pol_holder";
pub const LIQUID_POOLER_SALT: &[u8] = b"liquid_pooler";

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
    let creator_address: CanonicalAddr =
        deps.api.addr_canonicalize(env.contract.address.as_str())?;

    let covenant_denoms: BTreeSet<String> = msg.splits.keys().map(|k| k.to_string()).collect();

    let clock_instantiate2_config = get_instantiate2_salt_and_address(
        deps.as_ref(),
        CLOCK_SALT,
        &creator_address,
        msg.contract_codes.clock_code,
    )?;
    let holder_instantiate2_config = get_instantiate2_salt_and_address(
        deps.as_ref(),
        HOLDER_SALT,
        &creator_address,
        msg.contract_codes.holder_code,
    )?;
    let liquid_pooler_instantiate2_config = get_instantiate2_salt_and_address(
        deps.as_ref(),
        LIQUID_POOLER_SALT,
        &creator_address,
        msg.contract_codes.liquid_pooler_code,
    )?;

    let mut clock_whitelist: Vec<String> = Vec::with_capacity(4);
    clock_whitelist.push(holder_instantiate2_config.addr.to_string());
    clock_whitelist.push(liquid_pooler_instantiate2_config.addr.to_string());

    let holder_instantiate2_msg = covenant_two_party_pol_holder::msg::InstantiateMsg {
        clock_address: clock_instantiate2_config.addr.to_string(),
        lockup_config: msg.lockup_config,
        next_contract: liquid_pooler_instantiate2_config.addr.to_string(),
        ragequit_config: msg.ragequit_config.unwrap_or(RagequitConfig::Disabled),
        deposit_deadline: msg.deposit_deadline,
        splits: remap_splits(
            msg.splits,
            (
                msg.party_a_config.get_final_receiver_address(),
                "TODO".to_string(),
            ),
            (
                msg.party_b_config.get_final_receiver_address(),
                "TODO".to_string(),
            ),
        )?,
        fallback_split: match msg.fallback_split {
            Some(config) => Some(config.remap_receivers_to_routers(
                msg.party_a_config.get_final_receiver_address(),
                "TODO".to_string(),
                msg.party_b_config.get_final_receiver_address(),
                "TODO".to_string(),
            )?),
            None => None,
        },
        covenant_config: TwoPartyPolCovenantConfig {
            party_a: msg.party_a_config.to_two_party_pol_party(
                msg.party_a_share,
                "TODO".to_string(),
            ),
            party_b: msg.party_b_config.to_two_party_pol_party(
                msg.party_b_share,
                "TODO".to_string(),
            ),
            covenant_type: msg.covenant_type.clone(),
        },
        emergency_committee_addr: msg.emergency_committee,
    }
    .to_instantiate2_msg(
        &holder_instantiate2_config,
        env.contract.address.to_string(),
        format!("{}_holder", msg.label),
    )?;

    let liquid_pooler_instantiate2_msg = msg.liquid_pooler_config.to_instantiate2_msg(
        &liquid_pooler_instantiate2_config,
        env.contract.address.to_string(),
        format!("{}_liquid_pooler", msg.label),
        clock_instantiate2_config.addr.to_string(),
        holder_instantiate2_config.addr.to_string(),
        msg.pool_price_config,
    )?;

    let mut messages = vec![
        holder_instantiate2_msg,
        liquid_pooler_instantiate2_msg,
    ];

    let clock_instantiate2_msg = covenant_clock::msg::InstantiateMsg {
        tick_max_gas: msg.clock_tick_max_gas,
        whitelist: clock_whitelist,
    }
    .to_instantiate2_msg(
        clock_instantiate2_config.code,
        clock_instantiate2_config.salt,
        env.contract.address.to_string(),
        format!("{}-clock", msg.label),
    )?;
    messages.insert(0, clock_instantiate2_msg);

    CONTRACT_CODES.save(
        deps.storage,
        &msg.contract_codes.to_covenant_codes_config(),
    )?;
    COVENANT_POL_HOLDER_ADDR.save(deps.storage, &holder_instantiate2_config.addr)?;
    LIQUID_POOLER_ADDR.save(deps.storage, &liquid_pooler_instantiate2_config.addr)?;
    COVENANT_CLOCK_ADDR.save(deps.storage, &clock_instantiate2_config.addr)?;

    Ok(Response::default()
        .add_attribute("method", "instantiate")
        .add_attribute("clock_addr", clock_instantiate2_config.addr)
        .add_attribute("liquid_pooler_addr", liquid_pooler_instantiate2_config.addr)
        .add_attribute("holder_addr", holder_instantiate2_config.addr)
        .add_messages(messages))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::ClockAddress {} => Ok(to_json_binary(
            &COVENANT_CLOCK_ADDR.may_load(deps.storage)?,
        )?),
        QueryMsg::HolderAddress {} => Ok(to_json_binary(
            &COVENANT_POL_HOLDER_ADDR.may_load(deps.storage)?,
        )?),
        QueryMsg::LiquidPoolerAddress {} => {
            Ok(to_json_binary(&LIQUID_POOLER_ADDR.may_load(deps.storage)?)?)
        }
        QueryMsg::PartyDepositAddress { party } => {
            // TODO
            Ok(to_json_binary(&env.contract.address)?)}
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(deps: DepsMut, _env: Env, msg: MigrateMsg) -> StdResult<Response> {
    deps.api.debug("WASMDEBUG: migrate");
    match msg {
        MigrateMsg::UpdateCovenant {
            clock,
            holder,
            liquid_pooler,
            party_a_router,
            party_b_router,
            party_a_forwarder,
            party_b_forwarder,
        } => {
            let mut migrate_msgs = vec![];
            let mut resp = Response::default().add_attribute("method", "migrate_contracts");
            let contract_codes = CONTRACT_CODES.load(deps.storage)?;

            if let Some(clock) = clock {
                let msg = to_json_binary(&clock)?;
                resp = resp.add_attribute("clock_migrate", msg.to_base64());
                migrate_msgs.push(WasmMsg::Migrate {
                    contract_addr: COVENANT_CLOCK_ADDR.load(deps.storage)?.to_string(),
                    new_code_id: contract_codes.clock,
                    msg,
                });
            }

            if let Some(holder) = holder {
                let msg: Binary = to_json_binary(&holder)?;
                resp = resp.add_attribute("holder_migrate", msg.to_base64());
                migrate_msgs.push(WasmMsg::Migrate {
                    contract_addr: COVENANT_POL_HOLDER_ADDR.load(deps.storage)?.to_string(),
                    new_code_id: contract_codes.holder,
                    msg,
                });
            }

            if let Some(liquid_pooler_migrate_msg) = liquid_pooler {
                let msg: Binary = match liquid_pooler_migrate_msg {
                    LiquidPoolerMigrateMsg::Astroport(msg) => to_json_binary(&msg)?,
                    LiquidPoolerMigrateMsg::Osmosis(msg) => to_json_binary(&msg)?,
                };

                resp = resp.add_attribute("liquid_pooler_migrate", msg.to_base64());
                migrate_msgs.push(WasmMsg::Migrate {
                    contract_addr: LIQUID_POOLER_ADDR.load(deps.storage)?.to_string(),
                    new_code_id: contract_codes.liquid_pooler,
                    msg,
                });
            }

            Ok(resp.add_messages(migrate_msgs))
        }
    }
}
