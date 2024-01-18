#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    instantiate2_address, to_json_binary, Addr, Binary, CanonicalAddr, CodeInfoResponse, Deps,
    DepsMut, Env, MessageInfo, Response, StdResult, Uint128, WasmMsg, Decimal,
};

use covenant_astroport_liquid_pooler::msg::{
    AssetData, PresetAstroLiquidPoolerFields, SingleSideLpLimits,
};
use covenant_clock::msg::PresetClockFields;
use covenant_ibc_forwarder::msg::PresetIbcForwarderFields;
use covenant_native_splitter::msg::{NativeDenomSplit, SplitReceiver, PresetNativeSplitterFields};
use covenant_single_party_pol_holder::msg::PresetHolderFields;
use covenant_stride_liquid_staker::msg::PresetStrideLsFields;
use cw2::set_contract_version;
use sha2::{Digest, Sha256};


use crate::{
    error::ContractError,
    msg::{CovenantPartyConfig, InstantiateMsg, MigrateMsg, QueryMsg},
    state::{
        COVENANT_CLOCK_ADDR, HOLDER_ADDR,
        LIQUID_POOLER_ADDR, LIQUID_STAKER_ADDR, PRESET_CLOCK_FIELDS, PRESET_HOLDER_FIELDS, PRESET_LIQUID_POOLER_FIELDS,
        PRESET_LIQUID_STAKER_FIELDS, PRESET_SPLITTER_FIELDS, SPLITTER_ADDR, HOLDER_FORWARDER_ADDR, LS_FORWARDER_ADDR, PRESET_LS_FORWARDER_FIELDS, PRESET_HOLDER_FORWARDER_FIELDS,
    },
};

const CONTRACT_NAME: &str = "crates.io:covenant-single-party-pol";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

pub const CLOCK_SALT: &[u8] = b"clock";
pub const HOLDER_SALT: &[u8] = b"pol_holder";
pub const NATIVE_SPLITTER: &[u8] = b"native_splitter";

pub const LS_FORWARDER_SALT: &[u8] = b"ls_forwarder";
pub const HOLDER_FORWARDER_SALT: &[u8] = b"holder_forwarder";

pub const LIQUID_POOLER_SALT: &[u8] = b"liquid_pooler";
pub const LIQUID_STAKER_SALT: &[u8] = b"liquid_staker";

fn get_precomputed_address(
    deps: Deps,
    code_id: u64,
    creator: &CanonicalAddr,
    salt: &[u8],
) -> Result<Addr, ContractError> {
    let CodeInfoResponse { checksum, .. } = deps.querier.query_wasm_code_info(code_id)?;

    let precomputed_address = instantiate2_address(&checksum, creator, salt)?;

    Ok(deps.api.addr_humanize(&precomputed_address)?)
}

pub fn generate_contract_salt(salt_str: &[u8]) -> Binary {
    let mut hasher = Sha256::new();
    hasher.update(salt_str);
    hasher.finalize().to_vec().into()
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
    let clock_salt = generate_contract_salt(CLOCK_SALT);
    let native_splitter_salt = generate_contract_salt(NATIVE_SPLITTER);
    let ls_forwarder_salt = generate_contract_salt(LS_FORWARDER_SALT);
    let holder_forwarder_salt = generate_contract_salt(HOLDER_FORWARDER_SALT);
    let liquid_staker_salt = generate_contract_salt(LIQUID_STAKER_SALT);
    let liquid_pooler_salt = generate_contract_salt(LIQUID_POOLER_SALT);
    let holder_salt = generate_contract_salt(HOLDER_SALT);

    let creator_address = deps.api.addr_canonicalize(env.contract.address.as_str())?;

    let clock_address = get_precomputed_address(
        deps.as_ref(),
        msg.contract_codes.clock_code,
        &creator_address,
        &clock_salt,
    )?;

    let splitter_address = get_precomputed_address(
        deps.as_ref(),
        msg.contract_codes.native_splitter_code,
        &creator_address,
        &native_splitter_salt,
    )?;

    let ls_forwarder_address = get_precomputed_address(
        deps.as_ref(),
        msg.contract_codes.ibc_forwarder_code,
        &creator_address,
        &ls_forwarder_salt,
    )?;

    let holder_forwarder_address = get_precomputed_address(
        deps.as_ref(),
        msg.contract_codes.ibc_forwarder_code,
        &creator_address,
        &holder_forwarder_salt,
    )?;

    let liquid_staker_address = get_precomputed_address(
        deps.as_ref(),
        msg.contract_codes.liquid_staker_code,
        &creator_address,
        &liquid_staker_salt,
    )?;

    let liquid_pooler_address = get_precomputed_address(
        deps.as_ref(),
        msg.contract_codes.liquid_pooler_code,
        &creator_address,
        &liquid_pooler_salt,
    )?;

    let holder_address = get_precomputed_address(
        deps.as_ref(),
        msg.contract_codes.holder_code,
        &creator_address,
        &holder_salt,
    )?;

    HOLDER_ADDR.save(deps.storage, &holder_address)?;
    LIQUID_POOLER_ADDR.save(deps.storage, &liquid_pooler_address)?;
    LIQUID_STAKER_ADDR.save(deps.storage, &liquid_staker_address)?;
    COVENANT_CLOCK_ADDR.save(deps.storage, &clock_address)?;
    SPLITTER_ADDR.save(deps.storage, &splitter_address)?;

    let mut clock_whitelist = Vec::with_capacity(7);
    clock_whitelist.push(splitter_address.to_string());
    clock_whitelist.push(liquid_pooler_address.to_string());
    clock_whitelist.push(liquid_staker_address.to_string());
    clock_whitelist.push(holder_address.to_string());

    let preset_ls_forwarder_fields = match msg.clone().ls_forwarder_config {
        CovenantPartyConfig::Interchain(config) => {
            LS_FORWARDER_ADDR.save(deps.storage, &ls_forwarder_address)?;
            clock_whitelist.insert(0, ls_forwarder_address.to_string());

            let preset = PresetIbcForwarderFields {
                remote_chain_connection_id: config.party_chain_connection_id,
                remote_chain_channel_id: config.party_to_host_chain_channel_id,
                denom: config.remote_chain_denom,
                amount: config.contribution.amount,
                label: format!("{}_ls_ibc_forwarder", msg.label),
                code_id: msg.contract_codes.ibc_forwarder_code,
                ica_timeout: msg.timeouts.ica_timeout,
                ibc_transfer_timeout: msg.timeouts.ibc_transfer_timeout,
                ibc_fee: msg.preset_ibc_fee.to_ibc_fee(),
            };
            PRESET_LS_FORWARDER_FIELDS.save(deps.storage, &preset)?;

            Some(preset)
        }
        CovenantPartyConfig::Native(_) => None,
    };

    let preset_holder_forwarder_fields = match msg.clone().holder_forwarder_config {
        CovenantPartyConfig::Interchain(config) => {
            HOLDER_FORWARDER_ADDR.save(deps.storage, &holder_forwarder_address)?;
            clock_whitelist.insert(0, holder_forwarder_address.to_string());

            let preset = PresetIbcForwarderFields {
                remote_chain_connection_id: config.party_chain_connection_id,
                remote_chain_channel_id: config.party_to_host_chain_channel_id,
                denom: config.remote_chain_denom,
                amount: config.contribution.amount,
                label: format!("{}_holder_ibc_forwarder", msg.label),
                code_id: msg.contract_codes.ibc_forwarder_code,
                ica_timeout: msg.timeouts.ica_timeout,
                ibc_transfer_timeout: msg.timeouts.ibc_transfer_timeout,
                ibc_fee: msg.preset_ibc_fee.to_ibc_fee(),
            };
            PRESET_HOLDER_FORWARDER_FIELDS.save(deps.storage, &preset)?;

            Some(preset)
        }
        CovenantPartyConfig::Native(_) => None,
    };



    let preset_clock_fields = PresetClockFields {
        tick_max_gas: msg.clock_tick_max_gas,
        whitelist: clock_whitelist,
        code_id: msg.contract_codes.clock_code,
        label: format!("{}-clock", msg.label),
    };
    PRESET_CLOCK_FIELDS.save(deps.storage, &preset_clock_fields)?;

    // Holder
    let preset_holder_fields = PresetHolderFields {
        code_id: msg.contract_codes.holder_code,
        label: format!("{}-holder", msg.label),
        withdrawer: Some(info.sender.to_string()),
        withdraw_to: Some(info.sender.to_string()),
        lockup_period: msg.lockup_period,
    };
    PRESET_HOLDER_FIELDS.save(deps.storage, &preset_holder_fields)?;

    // Liquid staker
    let preset_liquid_staker_fields = PresetStrideLsFields {
        label: format!("{}_stride_liquid_staker", msg.label),
        ls_denom: msg.ls_info.ls_denom,
        stride_neutron_ibc_transfer_channel_id: msg.ls_info.ls_chain_to_neutron_channel_id,
        neutron_stride_ibc_connection_id: msg.ls_info.ls_neutron_connection_id,
        ica_timeout: msg.timeouts.ica_timeout,
        ibc_transfer_timeout: msg.timeouts.ibc_transfer_timeout,
        ibc_fee: msg.preset_ibc_fee.to_ibc_fee(),
        code_id: msg.contract_codes.liquid_staker_code,
    };
    PRESET_LIQUID_STAKER_FIELDS.save(deps.storage, &preset_liquid_staker_fields)?;

    // Liquid pooler
    let preset_liquid_pooler_fields = PresetAstroLiquidPoolerFields {
        slippage_tolerance: None,
        assets: AssetData {
            asset_a_denom: msg.ls_info.ls_denom_on_neutron,
            asset_b_denom: msg.holder_forwarder_config.get_native_denom(),
        },
        single_side_lp_limits: SingleSideLpLimits {
            asset_a_limit: msg.party_a_single_side_limit,
            asset_b_limit: msg.party_b_single_side_limit,
        },
        label: format!("{}_liquid_pooler", msg.label),
        code_id: msg.contract_codes.liquid_pooler_code,
        expected_pool_ratio: msg.expected_pool_ratio,
        acceptable_pool_ratio_delta: msg.acceptable_pool_ratio_delta,
        pair_type: msg.pool_pair_type,
    };
    PRESET_LIQUID_POOLER_FIELDS.save(deps.storage, &preset_liquid_pooler_fields)?;

    let preset_splitter_fields = PresetNativeSplitterFields {
        remote_chain_channel_id: msg.native_splitter_config.channel_id,
        remote_chain_connection_id: msg.native_splitter_config.connection_id,
        code_id: msg.contract_codes.native_splitter_code,
        label: format!("{}_remote_chain_splitter", msg.label),
        denom: msg.native_splitter_config.denom,
        amount: msg.native_splitter_config.amount,
        ibc_fee: msg.preset_ibc_fee.to_ibc_fee(),
        ica_timeout: msg.timeouts.ica_timeout,
        ibc_transfer_timeout: msg.timeouts.ibc_transfer_timeout,
    };
    PRESET_SPLITTER_FIELDS.save(deps.storage, &preset_splitter_fields)?;

    let mut messages = vec![
        preset_clock_fields.to_instantiate2_msg(env.contract.address.to_string(), clock_salt)?,
        preset_liquid_staker_fields.to_instantiate2_msg(
            env.contract.address.to_string(),
            liquid_staker_salt,
            clock_address.to_string(),
            liquid_pooler_address.to_string(),
        )?,
        preset_holder_fields.to_instantiate2_msg(
            env.contract.address.to_string(),
            holder_salt,
            liquid_pooler_address.to_string(),
        )?,
        preset_liquid_pooler_fields.to_instantiate2_msg(
            env.contract.address.to_string(),
            liquid_pooler_salt,
            msg.pool_address,
            clock_address.to_string(),
            holder_address.to_string(),
        )?,
        preset_splitter_fields.to_instantiate2_msg(
            env.contract.address.to_string(),
            native_splitter_salt,
            clock_address.to_string(),
            vec![
                NativeDenomSplit {
                    denom: "uatom".to_string(),
                    receivers: vec![
                        SplitReceiver {
                            addr: ls_forwarder_address.to_string(),
                            share: Decimal::from_ratio(Uint128::new(1), Uint128::new(2)),
                        },
                        SplitReceiver {
                            addr: holder_forwarder_address.to_string(),
                            share: Decimal::from_ratio(Uint128::new(1), Uint128::new(2)),
                        },
                    ]
                },
            ],
        )?,
    ];

    if let Some(fields) = preset_ls_forwarder_fields {
        messages.push(fields.to_instantiate2_msg(
            env.contract.address.to_string(),
            ls_forwarder_salt,
            clock_address.to_string(),
            liquid_staker_address.to_string(),
        )?);
    }

    if let Some(fields) = preset_holder_forwarder_fields {
        messages.push(fields.to_instantiate2_msg(
            env.contract.address.to_string(),
            holder_forwarder_salt,
            clock_address.to_string(),
            holder_address.to_string(),
        )?);
    };

    Ok(Response::default()
        .add_messages(messages)
        .add_attribute("method", "instantiate"))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::ClockAddress {} => Ok(to_json_binary(
            &COVENANT_CLOCK_ADDR.may_load(deps.storage)?,
        )?),
        QueryMsg::HolderAddress {} => Ok(to_json_binary(&HOLDER_ADDR.may_load(deps.storage)?)?),
        QueryMsg::IbcForwarderAddress { ty } => {
            let resp = if ty == "holder" {
                HOLDER_FORWARDER_ADDR.may_load(deps.storage)?
            } else if ty == "ls" {
                LS_FORWARDER_ADDR.may_load(deps.storage)?
            } else {
                Some(Addr::unchecked("not found"))
            };
            Ok(to_json_binary(&resp)?)
        }
        QueryMsg::LiquidStakerAddress {} => {
            Ok(to_json_binary(&LIQUID_STAKER_ADDR.may_load(deps.storage)?)?)
        }
        QueryMsg::LiquidPoolerAddress {} => {
            Ok(to_json_binary(&LIQUID_POOLER_ADDR.may_load(deps.storage)?)?)
        }
        QueryMsg::SplitterAddress {} => Ok(to_json_binary(&SPLITTER_ADDR.load(deps.storage)?)?),
        QueryMsg::PartyDepositAddress {} => {
            let splitter_address = SPLITTER_ADDR.load(deps.storage)?;
            let ica: Option<Addr> = deps.querier.query_wasm_smart(
                splitter_address,
                &covenant_utils::neutron_ica::CovenantQueryMsg::DepositAddress {},
            )?;

            Ok(to_json_binary(&ica)?)
        }
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(deps: DepsMut, _env: Env, msg: MigrateMsg) -> StdResult<Response> {
    deps.api.debug("WASMDEBUG: migrate");
    match msg {
        MigrateMsg::MigrateContracts {
            clock,
            ls_forwarder,
            holder_forwarder,
            holder: _, // TODO: Holder
            liquid_pooler,
            splitter,
        } => {
            let mut migrate_msgs = vec![];
            let mut resp = Response::default().add_attribute("method", "migrate_contracts");

            if let Some(clock) = clock {
                let msg = to_json_binary(&clock)?;
                let clock_fields = PRESET_CLOCK_FIELDS.load(deps.storage)?;
                resp = resp.add_attribute("clock_migrate", msg.to_base64());
                migrate_msgs.push(WasmMsg::Migrate {
                    contract_addr: COVENANT_CLOCK_ADDR.load(deps.storage)?.to_string(),
                    new_code_id: clock_fields.code_id,
                    msg,
                });
            }

            if let Some(forwarder) = ls_forwarder {
                let msg: Binary = to_json_binary(&forwarder)?;
                let forwarder_fields = PRESET_LS_FORWARDER_FIELDS.load(deps.storage)?;
                resp = resp.add_attribute("ls_forwarder_migrate", msg.to_base64());
                migrate_msgs.push(WasmMsg::Migrate {
                    contract_addr: LS_FORWARDER_ADDR.load(deps.storage)?.to_string(),
                    new_code_id: forwarder_fields.code_id,
                    msg,
                });
            }

            if let Some(forwarder) = holder_forwarder {
                let msg: Binary = to_json_binary(&forwarder)?;
                let forwarder_fields = PRESET_HOLDER_FORWARDER_FIELDS.load(deps.storage)?;
                resp = resp.add_attribute("holder_forwarder_migrate", msg.to_base64());
                migrate_msgs.push(WasmMsg::Migrate {
                    contract_addr: HOLDER_FORWARDER_ADDR.load(deps.storage)?.to_string(),
                    new_code_id: forwarder_fields.code_id,
                    msg,
                });
            }

            if let Some(liquid_pooler) = liquid_pooler {
                let msg: Binary = to_json_binary(&liquid_pooler)?;
                let liquid_pooler_fields = PRESET_LIQUID_POOLER_FIELDS.load(deps.storage)?;
                resp = resp.add_attribute("liquid_pooler_migrate", msg.to_base64());
                migrate_msgs.push(WasmMsg::Migrate {
                    contract_addr: LIQUID_POOLER_ADDR.load(deps.storage)?.to_string(),
                    new_code_id: liquid_pooler_fields.code_id,
                    msg,
                });
            }

            if let Some(splitter) = splitter {
                let msg: Binary = to_json_binary(&splitter)?;
                let splitter_fields = PRESET_SPLITTER_FIELDS.load(deps.storage)?;
                resp = resp.add_attribute("splitter_migrate", msg.to_base64());
                migrate_msgs.push(WasmMsg::Migrate {
                    contract_addr: SPLITTER_ADDR.load(deps.storage)?.to_string(),
                    new_code_id: splitter_fields.code_id,
                    msg,
                });
            }

            // if let Some(holder) = holder {
            //     let msg: Binary = to_json_binary(&holder)?;
            //     let holder_fields = PRESET_HOLDER_FIELDS.load(deps.storage)?;
            //     resp = resp.add_attribute("holder_migrate", msg.to_base64());
            //     migrate_msgs.push(WasmMsg::Migrate {
            //         contract_addr: COVENANT_POL_HOLDER_ADDR.load(deps.storage)?.to_string(),
            //         new_code_id: holder_fields.code_id,
            //         msg,
            //     });
            // }

            Ok(resp.add_messages(migrate_msgs))
        }
    }
}
