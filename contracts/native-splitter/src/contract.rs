use std::collections::BTreeMap;

#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    ensure, to_json_binary, Binary, CosmosMsg, Deps, DepsMut, Env, MessageInfo, Order, Response,
    StdError, StdResult,
};
use covenant_utils::{
    clock::{enqueue_msg, verify_clock},
    split::SplitConfig,
};
use cw2::set_contract_version;

use crate::error::ContractError;
use crate::msg::{ExecuteMsg, InstantiateMsg, MigrateMsg, QueryMsg};
use crate::state::{CLOCK_ADDRESS, FALLBACK_SPLIT, SPLIT_CONFIG_MAP};

const CONTRACT_NAME: &str = env!("CARGO_PKG_NAME");
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    let mut resp = Response::default().add_attribute("method", "native_splitter_instantiate");

    let clock_address = deps.api.addr_validate(&msg.clock_address)?;
    CLOCK_ADDRESS.save(deps.storage, &clock_address)?;
    resp = resp.add_attribute("clock_addr", msg.clock_address.to_string());

    // we validate the splits and store them per-denom
    for (denom, split) in msg.splits {
        split.validate_shares_and_receiver_addresses(deps.api)?;
        SPLIT_CONFIG_MAP.save(deps.storage, denom.to_string(), &split)?;
    }

    // if a fallback split is provided we validate and store it
    if let Some(split) = msg.fallback_split {
        resp = resp.add_attributes(vec![split.get_response_attribute("fallback".to_string())]);
        split.validate_shares_and_receiver_addresses(deps.api)?;
        FALLBACK_SPLIT.save(deps.storage, &split)?;
    } else {
        resp = resp.add_attribute("fallback", "None");
    }

    Ok(resp
        .add_message(enqueue_msg(msg.clock_address.as_str())?)
        .add_attribute("clock_address", clock_address))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::Tick {} => {
            verify_clock(&info.sender, &CLOCK_ADDRESS.load(deps.storage)?)
                .map_err(|_| ContractError::NotClock)?;

            try_distribute(deps, env)
        }
        ExecuteMsg::DistributeFallback { denoms } => try_distribute_fallback(deps, env, denoms),
    }
}

pub fn try_distribute(deps: DepsMut, env: Env) -> Result<Response, ContractError> {
    // first we query the contract balances
    let mut distribution_messages: Vec<CosmosMsg> = vec![];

    // then we iterate over our split config and try to match the entries to available balances
    for entry in SPLIT_CONFIG_MAP.range(deps.storage, None, None, Order::Ascending) {
        let (denom, config) = entry?;
        let balance = deps
            .querier
            .query_balance(env.contract.address.clone(), denom.to_string())?;

        if !balance.amount.is_zero() {
            let mut transfer_messages =
                config.get_transfer_messages(balance.amount, balance.denom.to_string(), None)?;
            distribution_messages.append(&mut transfer_messages);
        }
    }

    Ok(Response::default()
        .add_attribute("method", "try_distribute")
        .add_messages(distribution_messages))
}

fn try_distribute_fallback(
    deps: DepsMut,
    env: Env,
    denoms: Vec<String>,
) -> Result<Response, ContractError> {
    let mut distribution_messages: Vec<CosmosMsg> = vec![];

    if let Some(split) = FALLBACK_SPLIT.may_load(deps.storage)? {
        for denom in denoms {
            // we do not distribute the main covenant denoms
            // according to the fallback split
            ensure!(
                !SPLIT_CONFIG_MAP.has(deps.storage, denom.to_string()),
                ContractError::Std(StdError::generic_err("unauthorized denom distribution"))
            );

            let balance = deps
                .querier
                .query_balance(env.contract.address.to_string(), denom)?;
            if !balance.amount.is_zero() {
                let mut fallback_messages =
                    split.get_transfer_messages(balance.amount, balance.denom, None)?;
                distribution_messages.append(&mut fallback_messages);
            }
        }
    } else {
        return Err(StdError::generic_err("no fallback split defined").into());
    }

    Ok(Response::default()
        .add_attribute("method", "try_distribute_fallback")
        .add_messages(distribution_messages))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::ClockAddress {} => Ok(to_json_binary(&CLOCK_ADDRESS.may_load(deps.storage)?)?),
        QueryMsg::DenomSplit { denom } => Ok(to_json_binary(&query_split(deps, denom)?)?),
        QueryMsg::Splits {} => Ok(to_json_binary(&query_all_splits(deps)?)?),
        QueryMsg::FallbackSplit {} => Ok(to_json_binary(&FALLBACK_SPLIT.may_load(deps.storage)?)?),
        QueryMsg::DepositAddress {} => Ok(to_json_binary(&Some(env.contract.address))?),
    }
}

pub fn query_all_splits(deps: Deps) -> Result<Vec<(String, SplitConfig)>, StdError> {
    let mut splits: Vec<(String, SplitConfig)> = vec![];

    for entry in SPLIT_CONFIG_MAP.range(deps.storage, None, None, Order::Ascending) {
        let (denom, config) = entry?;
        splits.push((denom, config));
    }

    Ok(splits)
}

pub fn query_split(deps: Deps, denom: String) -> Result<SplitConfig, StdError> {
    for entry in SPLIT_CONFIG_MAP.range(deps.storage, None, None, Order::Ascending) {
        let (entry_denom, config) = entry?;
        if entry_denom == denom {
            return Ok(config);
        }
    }

    Ok(SplitConfig {
        receivers: BTreeMap::new(),
    })
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(deps: DepsMut, _env: Env, msg: MigrateMsg) -> Result<Response, StdError> {
    match msg {
        MigrateMsg::UpdateConfig {
            clock_addr,
            splits,
            fallback_split,
        } => {
            let mut resp = Response::default().add_attribute("method", "update_config");

            if let Some(clock_addr) = clock_addr {
                CLOCK_ADDRESS.save(deps.storage, &deps.api.addr_validate(&clock_addr)?)?;
                resp = resp.add_attribute("clock_addr", clock_addr);
            }

            if let Some(splits) = splits {
                // clear all current split configs before storing new values
                SPLIT_CONFIG_MAP.clear(deps.storage);
                for (denom, split) in splits {
                    // we validate each split before storing it
                    SPLIT_CONFIG_MAP.save(deps.storage, denom.to_string(), &split)?;
                }
            }

            if let Some(split) = fallback_split {
                FALLBACK_SPLIT.save(deps.storage, &split)?;
                resp =
                    resp.add_attributes(vec![split.get_response_attribute("fallback".to_string())]);
            }

            Ok(resp)
        }
        MigrateMsg::UpdateCodeId { data: _ } => {
            // This is a migrate message to update code id,
            // Data is optional base64 that we can parse to any data we would like in the future
            // let data: SomeStruct = from_binary(&data)?;
            Ok(Response::default())
        }
    }
}
