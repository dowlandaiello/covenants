use std::collections::BTreeMap;

use astroport::{
    asset::{Asset, PairInfo},
    pair::Cw20HookMsg,
};
use cosmwasm_std::{
    to_binary, BankMsg, Binary, Coin, CosmosMsg, Decimal, Deps, DepsMut, Env, MessageInfo,
    QuerierWrapper, Response, StdError, StdResult, Uint128, WasmMsg,
};

#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;

use covenant_utils::SplitConfig;
use cw2::set_contract_version;
use cw20::{BalanceResponse, Cw20ExecuteMsg};

use crate::{
    error::ContractError,
    msg::{
        ContractState, DenomSplits, ExecuteMsg, InstantiateMsg, MigrateMsg, QueryMsg,
        RagequitConfig, RagequitState,
    },
    state::{
        CLOCK_ADDRESS, CONTRACT_STATE, COVENANT_CONFIG, DENOM_SPLITS, DEPOSIT_DEADLINE,
        LOCKUP_CONFIG, NEXT_CONTRACT, POOL_ADDRESS, RAGEQUIT_CONFIG,
    },
};

const CONTRACT_NAME: &str = "crates.io:covenant-two-party-pol-holder";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    let pool_addr = deps.api.addr_validate(&msg.pool_address)?;
    let next_contract = deps.api.addr_validate(&msg.next_contract)?;
    let clock_addr = deps.api.addr_validate(&msg.clock_address)?;

    if msg.deposit_deadline.is_expired(&env.block) {
        return Err(ContractError::DepositDeadlineValidationError {});
    }
    if msg.lockup_config.is_expired(&env.block) {
        return Err(ContractError::LockupValidationError {});
    }

    msg.covenant_config.validate(deps.api)?;
    msg.ragequit_config.validate(
        msg.covenant_config.party_a.allocation,
        msg.covenant_config.party_b.allocation,
    )?;

    // validate the splits and convert them into map
    let explicit_splits = msg
        .clone()
        .splits
        .into_iter()
        .map(|(denom, split)| {
            let validated_split: SplitConfig = split.get_split_config()?.validate()?;
            Ok((denom, validated_split))
        })
        .collect::<Result<BTreeMap<String, SplitConfig>, ContractError>>()?;
    let fallback_split = match msg.clone().fallback_split {
        Some(split) => Some(split.get_split_config()?.validate()?),
        None => None,
    };
    let denom_splits = DenomSplits {
        explicit_splits,
        fallback_split,
    };
    DENOM_SPLITS.save(deps.storage, &denom_splits)?;
    POOL_ADDRESS.save(deps.storage, &pool_addr)?;
    NEXT_CONTRACT.save(deps.storage, &next_contract)?;
    CLOCK_ADDRESS.save(deps.storage, &clock_addr)?;
    LOCKUP_CONFIG.save(deps.storage, &msg.lockup_config)?;
    RAGEQUIT_CONFIG.save(deps.storage, &msg.ragequit_config)?;
    CONTRACT_STATE.save(deps.storage, &ContractState::Instantiated)?;
    COVENANT_CONFIG.save(deps.storage, &msg.covenant_config)?;
    DEPOSIT_DEADLINE.save(deps.storage, &msg.deposit_deadline)?;

    Ok(Response::default()
        .add_attribute("method", "two_party_pol_holder_instantiate")
        .add_attributes(msg.get_response_attributes()))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::Ragequit {} => try_ragequit(deps, env, info),
        ExecuteMsg::Claim {} => try_claim(deps, env, info),
        ExecuteMsg::Tick {} => try_tick(deps, env, info),
    }
}

/// queries the liquidity token balance of given address
fn query_liquidity_token_balance(
    querier: QuerierWrapper,
    liquidity_token: &str,
    contract_addr: String,
) -> Result<Uint128, ContractError> {
    let liquidity_token_balance: BalanceResponse = querier.query_wasm_smart(
        liquidity_token,
        &cw20::Cw20QueryMsg::Balance {
            address: contract_addr,
        },
    )?;
    Ok(liquidity_token_balance.balance)
}

/// queries the cw20 liquidity token address corresponding to a given pool
fn query_liquidity_token_address(
    querier: QuerierWrapper,
    pool: String,
) -> Result<String, ContractError> {
    let pair_info: PairInfo =
        querier.query_wasm_smart(pool, &astroport::pair::QueryMsg::Pair {})?;
    Ok(pair_info.liquidity_token.to_string())
}

// TODO: figure out best UX to implement a way to claim partial positions
// - Option<Decimal> ? None -> claim entire position, Some(%) -> claim the % of your entitlement
fn try_claim(deps: DepsMut, env: Env, info: MessageInfo) -> Result<Response, ContractError> {
    let mut covenant_config = COVENANT_CONFIG.load(deps.storage)?;
    let (mut claim_party, mut counterparty) =
        covenant_config.authorize_sender(info.sender.to_string())?;
    let pool = POOL_ADDRESS.load(deps.storage)?;
    let contract_state = CONTRACT_STATE.load(deps.storage)?;

    // we exit early if contract is not in ragequit or expired state
    // otherwise claim process is the same
    let response: Response = match contract_state {
        ContractState::Ragequit => {
            Response::default().add_attribute("method", "try_claim_ragequit")
        }
        ContractState::Expired => Response::default().add_attribute("method", "try_claim_expired"),
        _ => return Err(ContractError::ClaimError {}),
    };

    // if both parties already claimed everything we complete
    if claim_party.allocation.is_zero() && counterparty.allocation.is_zero() {
        CONTRACT_STATE.save(deps.storage, &ContractState::Complete)?;
        return Ok(Response::default()
            .add_attribute("method", "try_claim")
            .add_attribute("contract_state", "complete"));
    }

    let lp_token = query_liquidity_token_address(deps.querier, pool.to_string())?;
    let liquidity_token_balance =
        query_liquidity_token_balance(deps.querier, &lp_token, env.contract.address.to_string())?;

    // if no lp tokens are available, no point to ragequit
    if liquidity_token_balance.is_zero() {
        return Err(ContractError::NoLpTokensAvailable {});
    }

    // we figure out the amounts of underlying tokens that claiming party could receive
    let claim_party_lp_token_amount = liquidity_token_balance
        .checked_mul_floor(claim_party.allocation)
        .map_err(|_| ContractError::FractionMulError {})?;
    let claim_party_entitled_assets: Vec<Asset> = deps.querier.query_wasm_smart(
        pool.to_string(),
        &astroport::pair::QueryMsg::Share {
            amount: claim_party_lp_token_amount,
        },
    )?;
    // convert astro assets to coins
    let mut withdraw_coins: Vec<Coin> = vec![];
    for asset in claim_party_entitled_assets {
        withdraw_coins.push(asset.to_coin()?);
    }

    // generate the withdraw_liquidity hook for the claim party
    let withdraw_liquidity_hook = &Cw20HookMsg::WithdrawLiquidity { assets: vec![] };
    let withdraw_msg = &Cw20ExecuteMsg::Send {
        contract: pool.to_string(),
        amount: claim_party_lp_token_amount,
        msg: to_binary(withdraw_liquidity_hook)?,
    };

    let denom_splits = DENOM_SPLITS.load(deps.storage)?;
    let mut distribution_messages = denom_splits.get_distribution_messages(withdraw_coins);

    // we submit the withdraw liquidity message followed by transfer of
    // underlying assets to the corresponding router
    let mut withdraw_and_forward_msgs = vec![CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: lp_token.to_string(),
        msg: to_binary(withdraw_msg)?,
        funds: vec![],
    })];

    withdraw_and_forward_msgs.append(&mut distribution_messages);

    claim_party.allocation = Decimal::zero();

    // if other party had not claimed yet, we assign it the full position
    if !counterparty.allocation.is_zero() {
        counterparty.allocation = Decimal::one();
    } else {
        // otherwise both parties claimed everything and we can complete
        CONTRACT_STATE.save(deps.storage, &ContractState::Complete)?;
    }

    covenant_config.update_parties(claim_party, counterparty);

    COVENANT_CONFIG.save(deps.storage, &covenant_config)?;

    Ok(response.add_messages(withdraw_and_forward_msgs))
}

fn try_tick(deps: DepsMut, env: Env, info: MessageInfo) -> Result<Response, ContractError> {
    let state = CONTRACT_STATE.load(deps.storage)?;
    match state {
        ContractState::Instantiated => try_deposit(deps, env, info),
        ContractState::Active => check_expiration(deps, env),
        ContractState::Expired => {
            let config = COVENANT_CONFIG.load(deps.storage)?;
            let state =
                if config.party_a.allocation.is_zero() && config.party_b.allocation.is_zero() {
                    CONTRACT_STATE.save(deps.storage, &ContractState::Complete)?;
                    ContractState::Complete
                } else {
                    state
                };
            Ok(Response::default()
                .add_attribute("method", "tick")
                .add_attribute("contract_state", state.to_string()))
        }
        // ragequit and completed states do not trigger an action
        _ => Ok(Response::default()
            .add_attribute("method", "tick")
            .add_attribute("contract_state", state.to_string())),
    }
}

fn try_deposit(deps: DepsMut, env: Env, _info: MessageInfo) -> Result<Response, ContractError> {
    let config = COVENANT_CONFIG.load(deps.storage)?;
    let deposit_deadline = DEPOSIT_DEADLINE.load(deps.storage)?;

    // assert the balances
    let party_a_bal = deps.querier.query_balance(
        env.contract.address.to_string(),
        config.party_a.contribution.denom,
    )?;
    let party_b_bal = deps.querier.query_balance(
        env.contract.address.to_string(),
        config.party_b.contribution.denom,
    )?;

    let party_a_fulfilled = config.party_a.contribution.amount <= party_a_bal.amount;
    let party_b_fulfilled = config.party_b.contribution.amount <= party_b_bal.amount;

    // note: even if both parties deposit their funds in time,
    // it is important to trigger this method before the expiry block
    // if deposit deadline is due we complete and refund
    if deposit_deadline.is_expired(&env.block) {
        let refund_messages: Vec<CosmosMsg> =
            match (party_a_bal.amount.is_zero(), party_b_bal.amount.is_zero()) {
                // both balances empty, we complete
                (true, true) => {
                    CONTRACT_STATE.save(deps.storage, &ContractState::Complete)?;
                    return Ok(Response::default()
                        .add_attribute("method", "try_deposit")
                        .add_attribute("state", "complete"));
                }
                // refund party B
                (true, false) => vec![CosmosMsg::Bank(BankMsg::Send {
                    to_address: config.party_b.router,
                    amount: vec![party_b_bal],
                })],
                // refund party A
                (false, true) => vec![CosmosMsg::Bank(BankMsg::Send {
                    to_address: config.party_a.router,
                    amount: vec![party_a_bal],
                })],
                // refund both
                (false, false) => vec![
                    CosmosMsg::Bank(BankMsg::Send {
                        to_address: config.party_a.router.to_string(),
                        amount: vec![party_a_bal],
                    }),
                    CosmosMsg::Bank(BankMsg::Send {
                        to_address: config.party_b.router,
                        amount: vec![party_b_bal],
                    }),
                ],
            };
        return Ok(Response::default()
            .add_attribute("method", "try_deposit")
            .add_attribute("action", "refund")
            .add_messages(refund_messages));
    }

    if !party_a_fulfilled || !party_b_fulfilled {
        // if deposit deadline is not yet due and both parties did not fulfill we error
        return Err(ContractError::InsufficientDeposits {});
    }

    // LiquidPooler is the next contract
    let liquid_pooler = NEXT_CONTRACT.load(deps.storage)?;
    let msg = BankMsg::Send {
        to_address: liquid_pooler.to_string(),
        amount: vec![party_a_bal, party_b_bal],
    };

    // advance the state to Active
    CONTRACT_STATE.save(deps.storage, &ContractState::Active)?;

    Ok(Response::default()
        .add_attribute("method", "deposit_to_next_contract")
        .add_message(msg))
}

fn check_expiration(deps: DepsMut, env: Env) -> Result<Response, ContractError> {
    let lockup_config = LOCKUP_CONFIG.load(deps.storage)?;

    if !lockup_config.is_expired(&env.block) {
        return Ok(Response::default()
            .add_attribute("method", "check_expiration")
            .add_attribute("result", "not_due"));
    }

    // advance state to Expired to enable claims
    CONTRACT_STATE.save(deps.storage, &ContractState::Expired)?;

    Ok(Response::default()
        .add_attribute("method", "check_expiration")
        .add_attribute("contract_state", "expired"))
}

fn try_ragequit(deps: DepsMut, env: Env, info: MessageInfo) -> Result<Response, ContractError> {
    // first we error out if ragequit is disabled
    let mut rq_config = match RAGEQUIT_CONFIG.load(deps.storage)? {
        RagequitConfig::Disabled => return Err(ContractError::RagequitDisabled {}),
        RagequitConfig::Enabled(terms) => terms,
    };
    let current_state = CONTRACT_STATE.load(deps.storage)?;
    let lockup_config = LOCKUP_CONFIG.load(deps.storage)?;
    let mut covenant_config = COVENANT_CONFIG.load(deps.storage)?;
    let pool = POOL_ADDRESS.load(deps.storage)?;

    // ragequit is only possible when contract is in Active state.
    if current_state != ContractState::Active {
        return Err(ContractError::NotActive {});
    }
    // we also validate an edge case where it did expire but
    // did not receive a tick yet. tick is then required to advance.
    if lockup_config.is_expired(&env.block) {
        return Err(ContractError::Expired {});
    }

    // authorize the message sender
    let (mut rq_party, mut counterparty) =
        covenant_config.authorize_sender(info.sender.to_string())?;

    // apply the ragequit penalty and get the new splits
    let updated_denom_splits = DENOM_SPLITS.update(deps.storage, |mut splits| -> StdResult<_> {
        let new_denom_splits: DenomSplits =
            splits.apply_penalty(rq_config.penalty, &rq_party, &counterparty);
        Ok(new_denom_splits)
    })?;

    // TODO: get rid of allocation property entirely?
    rq_party.allocation -= rq_config.penalty;

    let lp_token = query_liquidity_token_address(deps.querier, pool.to_string())?;

    // We query our own liquidity token balance
    let liquidity_token_balance =
        query_liquidity_token_balance(deps.querier, &lp_token, env.contract.address.to_string())?;

    // if no lp tokens are available, no point to ragequit
    if liquidity_token_balance.is_zero() {
        return Err(ContractError::NoLpTokensAvailable {});
    }

    // we figure out the amounts of underlying tokens that rq party would receive
    let rq_party_lp_token_amount = liquidity_token_balance
        .checked_mul_floor(rq_party.allocation)
        .map_err(|_| ContractError::FractionMulError {})?;
    let rq_entitled_assets: Vec<Asset> = deps.querier.query_wasm_smart(
        pool.to_string(),
        &astroport::pair::QueryMsg::Share {
            amount: rq_party_lp_token_amount,
        },
    )?;

    // reflect the ragequit in ragequit config
    let rq_state = RagequitState::from_share_response(rq_entitled_assets, rq_party.clone())?;
    rq_config.state = Some(rq_state.clone());

    // generate the withdraw_liquidity hook for the ragequitting party
    let withdraw_liquidity_hook = &Cw20HookMsg::WithdrawLiquidity { assets: vec![] };
    let withdraw_msg = &Cw20ExecuteMsg::Send {
        contract: pool.to_string(),
        amount: rq_party_lp_token_amount,
        msg: to_binary(withdraw_liquidity_hook)?,
    };

    let balances = rq_state.coins.clone();
    let mut distribution_messages = updated_denom_splits.get_distribution_messages(balances);

    // we submit the withdraw liquidity message followed by transfer of
    // underlying assets to the corresponding router
    let mut withdraw_and_forward_msgs = vec![CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: lp_token.to_string(),
        msg: to_binary(withdraw_msg)?,
        funds: vec![],
    })];
    withdraw_and_forward_msgs.append(&mut distribution_messages);

    // after building the messages we can finalize the config updates.
    // rq party is now entitled to nothing. counterparty owns the entire position.
    rq_party.allocation = Decimal::zero();
    counterparty.allocation = Decimal::one();
    covenant_config.update_parties(rq_party.clone(), counterparty);

    // update the states
    RAGEQUIT_CONFIG.save(deps.storage, &RagequitConfig::Enabled(rq_config))?;
    COVENANT_CONFIG.save(deps.storage, &covenant_config)?;
    CONTRACT_STATE.save(deps.storage, &ContractState::Ragequit)?;

    Ok(Response::default()
        .add_attribute("method", "ragequit")
        .add_attribute("controller_chain_caller", rq_party.controller_addr)
        .add_attribute("host_chain_caller", rq_party.host_addr)
        .add_messages(withdraw_and_forward_msgs))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::ContractState {} => Ok(to_binary(&CONTRACT_STATE.load(deps.storage)?)?),
        QueryMsg::RagequitConfig {} => Ok(to_binary(&RAGEQUIT_CONFIG.load(deps.storage)?)?),
        QueryMsg::LockupConfig {} => Ok(to_binary(&LOCKUP_CONFIG.load(deps.storage)?)?),
        QueryMsg::ClockAddress {} => Ok(to_binary(&CLOCK_ADDRESS.load(deps.storage)?)?),
        QueryMsg::NextContract {} => Ok(to_binary(&NEXT_CONTRACT.load(deps.storage)?)?),
        QueryMsg::PoolAddress {} => Ok(to_binary(&POOL_ADDRESS.load(deps.storage)?)?),
        QueryMsg::ConfigPartyA {} => Ok(to_binary(&COVENANT_CONFIG.load(deps.storage)?.party_a)?),
        QueryMsg::ConfigPartyB {} => Ok(to_binary(&COVENANT_CONFIG.load(deps.storage)?.party_b)?),
        QueryMsg::DepositDeadline {} => Ok(to_binary(&DEPOSIT_DEADLINE.load(deps.storage)?)?),
        QueryMsg::Config {} => Ok(to_binary(&COVENANT_CONFIG.load(deps.storage)?)?),
        QueryMsg::DepositAddress {} => Ok(to_binary(&env.contract.address)?),
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(deps: DepsMut, env: Env, msg: MigrateMsg) -> StdResult<Response> {
    deps.api.debug("WASMDEBUG: migrate");
    match msg {
        MigrateMsg::UpdateConfig {
            clock_addr,
            next_contract,
            lockup_config,
            deposit_deadline,
            pool_address,
            ragequit_config,
            covenant_config,
        } => {
            let mut resp = Response::default().add_attribute("method", "update_config");

            if let Some(addr) = clock_addr {
                let clock_address = deps.api.addr_validate(&addr)?;
                CLOCK_ADDRESS.save(deps.storage, &clock_address)?;
                resp = resp.add_attribute("clock_addr", addr);
            }

            if let Some(addr) = next_contract {
                let next_contract_addr = deps.api.addr_validate(&addr)?;
                NEXT_CONTRACT.save(deps.storage, &next_contract_addr)?;
                resp = resp.add_attribute("next_contract", addr);
            }

            if let Some(expiry_config) = lockup_config {
                if expiry_config.is_expired(&env.block) {
                    return Err(StdError::generic_err("lockup config is already past"));
                }
                LOCKUP_CONFIG.save(deps.storage, &expiry_config)?;
                resp = resp.add_attribute("lockup_config", expiry_config.to_string());
            }

            if let Some(expiry_config) = deposit_deadline {
                if expiry_config.is_expired(&env.block) {
                    return Err(StdError::generic_err("deposit deadline is already past"));
                }
                DEPOSIT_DEADLINE.save(deps.storage, &expiry_config)?;
                resp = resp.add_attribute("deposit_deadline", expiry_config.to_string());
            }

            if let Some(addr) = pool_address {
                let pool_addr = deps.api.addr_validate(&addr)?;
                POOL_ADDRESS.save(deps.storage, &pool_addr)?;
                resp = resp.add_attribute("pool_addr", pool_addr);
            }

            if let Some(config) = ragequit_config {
                RAGEQUIT_CONFIG.save(deps.storage, &config)?;
                resp = resp.add_attributes(config.get_response_attributes());
            }

            if let Some(config) = covenant_config {
                COVENANT_CONFIG.save(deps.storage, &config)?;
                resp = resp.add_attribute("todo", "todo");
            }

            Ok(resp)
        }
        MigrateMsg::UpdateCodeId { data: _ } => todo!(),
    }
}
