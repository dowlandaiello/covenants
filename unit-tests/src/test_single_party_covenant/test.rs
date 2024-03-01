use cosmwasm_std::{coin, Addr, Uint128};
use cw_multi_test::Executor;

use crate::setup::{
    base_suite::BaseSuiteMut, DENOM_ATOM, DENOM_ATOM_ON_NTRN, DENOM_LS_ATOM_ON_NTRN,
    DENOM_LS_ATOM_ON_STRIDE,
};

use super::suite::Suite;

#[test]
fn test_covenant() {
    let mut suite = Suite::new_with_stable_pool();

    suite.get_and_fund_depositors(coin(1_000_000_000_000_u128, DENOM_ATOM));

    // Verify forwarders got their split from the splitter
    let lp_forwarder_ica = suite.get_ica(suite.lp_forwarder_addr.clone());
    let ls_forwarder_ica = suite.get_ica(suite.ls_forwarder_addr.clone());

    while suite
        .app
        .wrap()
        .query_all_balances(lp_forwarder_ica.clone())
        .unwrap()
        .is_empty()
    {
        suite.tick("Wait for lp_forwarder ICA to get its split");
    }

    while suite
        .app
        .wrap()
        .query_all_balances(ls_forwarder_ica.clone())
        .unwrap()
        .is_empty()
    {
        suite.tick("Wait for lp_forwarder ICA to get its split");
    }

    let lp_forwarder_ica_balance = suite
        .app
        .wrap()
        .query_balance(lp_forwarder_ica.clone(), DENOM_ATOM)
        .unwrap();
    let ls_forwarder_ica_balance = suite
        .app
        .wrap()
        .query_balance(ls_forwarder_ica.clone(), DENOM_ATOM)
        .unwrap();

    assert_eq!(lp_forwarder_ica_balance.amount.u128(), 500_000_000_000_u128);
    assert_eq!(ls_forwarder_ica_balance.amount.u128(), 500_000_000_000_u128);

    // Wait for forwarders to forward the funds to the correct addrs
    let lser_ica = suite.get_ica(suite.lser_addr.clone());

    // lser_ica should get his half on stride (lsAtom on stride)
    while suite
        .app
        .wrap()
        .query_all_balances(lser_ica.clone())
        .unwrap()
        .is_empty()
    {
        suite.tick("Wait for lser ICA to get his lsAtom");
    }

    // lper should get his atom (atom on neutron)
    while suite
        .app
        .wrap()
        .query_all_balances(suite.lper_addr.clone())
        .unwrap()
        .is_empty()
    {
        suite.tick("Wait for lser ICA to get his lsAtom");
    }

    // Make sure the correct denoms are received on the correct addrs
    let lser_ica_balance = suite
        .app
        .wrap()
        .query_balance(lser_ica, DENOM_LS_ATOM_ON_STRIDE)
        .unwrap();
    let lper_balance = suite
        .app
        .wrap()
        .query_balance(suite.lper_addr.clone(), DENOM_ATOM_ON_NTRN)
        .unwrap();

    assert_eq!(lser_ica_balance.amount.u128(), 500_000_000_000_u128);
    assert_eq!(lper_balance.amount.u128(), 500_000_000_000_u128);

    // TODO: Currently we need to manually send the LS tokens from stride to the lper
    // TODO: When autopilot will be able to auto send over IBC, we can wait on the lper to receive both denoms
    suite
        .app
        .execute_contract(
            suite.admin.clone(),
            suite.lser_addr.clone(),
            &covenant_stride_liquid_staker::msg::ExecuteMsg::Transfer {
                amount: 500_000_000_000_u128.into(),
            },
            &[],
        )
        .unwrap();

    // We only check that lper got the ls tokens, as we already have the native atom check
    let lper_balance = suite
        .app
        .wrap()
        .query_balance(suite.lper_addr.clone(), DENOM_LS_ATOM_ON_NTRN)
        .unwrap();
    assert_eq!(lper_balance.amount.u128(), 500_000_000_000_u128);

    // Wait until lper provide liquidity
    while suite
        .app
        .wrap()
        .query_balance(suite.lper_addr.clone(), DENOM_LS_ATOM_ON_NTRN)
        .unwrap()
        .amount
        .u128()
        > 100_000_000_000_u128
    {
        suite.tick("Wait for lper to provide liquidity");
    }

    suite.app.update_block(|b| {
        b.height += 5;
        b.time = b.time.plus_seconds(15)
    });

    // Verify lper has the lp tokens after providing liquidity
    let lper_lp_token_balance = suite
        .app
        .wrap()
        .query_wasm_smart::<cw20::BalanceResponse>(
            suite.lp_token_addr.clone(),
            &cw20::Cw20QueryMsg::Balance {
                address: suite.lper_addr.to_string(),
            },
        )
        .unwrap();

    assert!(lper_lp_token_balance.balance > Uint128::zero());

    // Try to claim, but we still in the lockup period so this should fail.
    suite
        .app
        .execute_contract(
            suite.party_local_receiver.clone(),
            suite.holder_addr.clone(),
            &covenant_single_party_pol_holder::msg::ExecuteMsg::Claim {},
            &[],
        )
        .unwrap_err();

    // pass the lockup period, and try to withdraw the liquidity
    suite.app.update_block(|b| {
        b.height += 100000;
        b.time = b.time.plus_seconds(100000 * 3)
    });

    suite
        .app
        .execute_contract(
            suite.party_local_receiver.clone(),
            suite.holder_addr.clone(),
            &covenant_single_party_pol_holder::msg::ExecuteMsg::Claim {},
            &[],
        )
        .unwrap();

    let router_addr = suite
        .app
        .wrap()
        .query_wasm_smart::<Addr>(
            suite.covenant_addr.clone(),
            &covenant_single_party_pol::msg::QueryMsg::InterchainRouterAddress {},
        )
        .unwrap();

    while suite
        .app
        .wrap()
        .query_all_balances(suite.party_receiver.clone())
        .unwrap()
        .is_empty()
    {
        suite.tick("Wait for party_receiver to get funds");
    }

    let receiver_balance = suite
        .app
        .wrap()
        .query_balance(suite.party_receiver.clone(), DENOM_ATOM)
        .unwrap();

    // We used pfm, so the receiver should have close to 1_000_000_000_000 uatom
    assert!(receiver_balance.amount.u128() > 900_000_000_000_u128);
}

#[test]
fn test_covenant_with_xyk_pool() {
    let mut suite = Suite::new_with_xyk_pool();

    suite.get_and_fund_depositors(coin(1_000_000_000_000_u128, DENOM_ATOM));

    // Verify forwarders got their split from the splitter
    let lp_forwarder_ica = suite.get_ica(suite.lp_forwarder_addr.clone());
    let ls_forwarder_ica = suite.get_ica(suite.ls_forwarder_addr.clone());

    while suite
        .app
        .wrap()
        .query_all_balances(lp_forwarder_ica.clone())
        .unwrap()
        .is_empty()
    {
        suite.tick("Wait for lp_forwarder ICA to get its split");
    }

    while suite
        .app
        .wrap()
        .query_all_balances(ls_forwarder_ica.clone())
        .unwrap()
        .is_empty()
    {
        suite.tick("Wait for lp_forwarder ICA to get its split");
    }

    let lp_forwarder_ica_balance = suite
        .app
        .wrap()
        .query_balance(lp_forwarder_ica.clone(), DENOM_ATOM)
        .unwrap();
    let ls_forwarder_ica_balance = suite
        .app
        .wrap()
        .query_balance(ls_forwarder_ica.clone(), DENOM_ATOM)
        .unwrap();

    assert_eq!(lp_forwarder_ica_balance.amount.u128(), 500_000_000_000_u128);
    assert_eq!(ls_forwarder_ica_balance.amount.u128(), 500_000_000_000_u128);

    // Wait for forwarders to forward the funds to the correct addrs
    let lser_ica = suite.get_ica(suite.lser_addr.clone());

    // lser_ica should get his half on stride (lsAtom on stride)
    while suite
        .app
        .wrap()
        .query_all_balances(lser_ica.clone())
        .unwrap()
        .is_empty()
    {
        suite.tick("Wait for lser ICA to get his lsAtom");
    }

    // lper should get his atom (atom on neutron)
    while suite
        .app
        .wrap()
        .query_all_balances(suite.lper_addr.clone())
        .unwrap()
        .is_empty()
    {
        suite.tick("Wait for lser ICA to get his lsAtom");
    }

    // Make sure the correct denoms are received on the correct addrs
    let lser_ica_balance = suite
        .app
        .wrap()
        .query_balance(lser_ica, DENOM_LS_ATOM_ON_STRIDE)
        .unwrap();
    let lper_balance = suite
        .app
        .wrap()
        .query_balance(suite.lper_addr.clone(), DENOM_ATOM_ON_NTRN)
        .unwrap();

    assert_eq!(lser_ica_balance.amount.u128(), 500_000_000_000_u128);
    assert_eq!(lper_balance.amount.u128(), 500_000_000_000_u128);

    // TODO: Currently we need to manually send the LS tokens from stride to the lper
    // TODO: When autopilot will be able to auto send over IBC, we can wait on the lper to receive both denoms
    suite
        .app
        .execute_contract(
            suite.admin.clone(),
            suite.lser_addr.clone(),
            &covenant_stride_liquid_staker::msg::ExecuteMsg::Transfer {
                amount: 500_000_000_000_u128.into(),
            },
            &[],
        )
        .unwrap();

    // We only check that lper got the ls tokens, as we already have the native atom check
    let lper_balance = suite
        .app
        .wrap()
        .query_balance(suite.lper_addr.clone(), DENOM_LS_ATOM_ON_NTRN)
        .unwrap();
    assert_eq!(lper_balance.amount.u128(), 500_000_000_000_u128);

    // Wait until lper provide liquidity
    while suite
        .app
        .wrap()
        .query_balance(suite.lper_addr.clone(), DENOM_LS_ATOM_ON_NTRN)
        .unwrap()
        .amount
        .u128()
        > 100_000_000_000_u128
    {
        suite.tick("Wait for lper to provide liquidity");
    }

    suite.app.update_block(|b| {
        b.height += 5;
        b.time = b.time.plus_seconds(15)
    });

    // Verify lper has the lp tokens after providing liquidity
    let lper_lp_token_balance = suite
        .app
        .wrap()
        .query_wasm_smart::<cw20::BalanceResponse>(
            suite.lp_token_addr.clone(),
            &cw20::Cw20QueryMsg::Balance {
                address: suite.lper_addr.to_string(),
            },
        )
        .unwrap();

    assert!(lper_lp_token_balance.balance > Uint128::zero());

    // Try to claim, but we still in the lockup period so this should fail.
    suite
        .app
        .execute_contract(
            suite.party_local_receiver.clone(),
            suite.holder_addr.clone(),
            &covenant_single_party_pol_holder::msg::ExecuteMsg::Claim {},
            &[],
        )
        .unwrap_err();

    // pass the lockup period, and try to withdraw the liquidity
    suite.app.update_block(|b| {
        b.height += 100000;
        b.time = b.time.plus_seconds(100000 * 3)
    });

    suite
        .app
        .execute_contract(
            suite.party_local_receiver.clone(),
            suite.holder_addr.clone(),
            &covenant_single_party_pol_holder::msg::ExecuteMsg::Claim {},
            &[],
        )
        .unwrap();

    let router_addr = suite
        .app
        .wrap()
        .query_wasm_smart::<Addr>(
            suite.covenant_addr.clone(),
            &covenant_single_party_pol::msg::QueryMsg::InterchainRouterAddress {},
        )
        .unwrap();

    while suite
        .app
        .wrap()
        .query_all_balances(suite.party_receiver.clone())
        .unwrap()
        .is_empty()
    {
        suite.tick("Wait for party_receiver to get funds");
    }

    let receiver_balance = suite
        .app
        .wrap()
        .query_balance(suite.party_receiver.clone(), DENOM_ATOM)
        .unwrap();

    // We used pfm, so the receiver should have close to 1_000_000_000_000 uatom
    assert!(receiver_balance.amount.u128() > 900_000_000_000_u128);
}

#[test]
fn test_covenant_with_uneven_pool() {
    let mut suite = Suite::new_with_xyk_pool();

    suite.astro_swap(coin(512_345_678_987, DENOM_ATOM_ON_NTRN));

    suite.get_and_fund_depositors(coin(1_000_000_000_000_u128, DENOM_ATOM));

    // Verify forwarders got their split from the splitter
    let lp_forwarder_ica = suite.get_ica(suite.lp_forwarder_addr.clone());
    let ls_forwarder_ica = suite.get_ica(suite.ls_forwarder_addr.clone());

    while suite
        .app
        .wrap()
        .query_all_balances(lp_forwarder_ica.clone())
        .unwrap()
        .is_empty()
    {
        suite.tick("Wait for lp_forwarder ICA to get its split");
    }

    while suite
        .app
        .wrap()
        .query_all_balances(ls_forwarder_ica.clone())
        .unwrap()
        .is_empty()
    {
        suite.tick("Wait for lp_forwarder ICA to get its split");
    }

    let lp_forwarder_ica_balance = suite
        .app
        .wrap()
        .query_balance(lp_forwarder_ica.clone(), DENOM_ATOM)
        .unwrap();
    let ls_forwarder_ica_balance = suite
        .app
        .wrap()
        .query_balance(ls_forwarder_ica.clone(), DENOM_ATOM)
        .unwrap();

    assert_eq!(lp_forwarder_ica_balance.amount.u128(), 500_000_000_000_u128);
    assert_eq!(ls_forwarder_ica_balance.amount.u128(), 500_000_000_000_u128);

    // Wait for forwarders to forward the funds to the correct addrs
    let lser_ica = suite.get_ica(suite.lser_addr.clone());

    // lser_ica should get his half on stride (lsAtom on stride)
    while suite
        .app
        .wrap()
        .query_all_balances(lser_ica.clone())
        .unwrap()
        .is_empty()
    {
        suite.tick("Wait for lser ICA to get his lsAtom");
    }

    // lper should get his atom (atom on neutron)
    while suite
        .app
        .wrap()
        .query_all_balances(suite.lper_addr.clone())
        .unwrap()
        .is_empty()
    {
        suite.tick("Wait for lser ICA to get his lsAtom");
    }

    // Make sure the correct denoms are received on the correct addrs
    let lser_ica_balance = suite
        .app
        .wrap()
        .query_balance(lser_ica, DENOM_LS_ATOM_ON_STRIDE)
        .unwrap();
    let lper_balance = suite
        .app
        .wrap()
        .query_balance(suite.lper_addr.clone(), DENOM_ATOM_ON_NTRN)
        .unwrap();

    assert_eq!(lser_ica_balance.amount.u128(), 500_000_000_000_u128);
    assert_eq!(lper_balance.amount.u128(), 500_000_000_000_u128);

    // TODO: Currently we need to manually send the LS tokens from stride to the lper
    // TODO: When autopilot will be able to auto send over IBC, we can wait on the lper to receive both denoms
    suite
        .app
        .execute_contract(
            suite.admin.clone(),
            suite.lser_addr.clone(),
            &covenant_stride_liquid_staker::msg::ExecuteMsg::Transfer {
                amount: 500_000_000_000_u128.into(),
            },
            &[],
        )
        .unwrap();

    // We only check that lper got the ls tokens, as we already have the native atom check
    let lper_balance = suite
        .app
        .wrap()
        .query_balance(suite.lper_addr.clone(), DENOM_LS_ATOM_ON_NTRN)
        .unwrap();
    assert_eq!(lper_balance.amount.u128(), 500_000_000_000_u128);

    // Wait until lper provide liquidity
    while suite
        .app
        .wrap()
        .query_balance(suite.lper_addr.clone(), DENOM_LS_ATOM_ON_NTRN)
        .unwrap()
        .amount
        .u128()
        > 100_000_000_000_u128
    {
        suite.tick("Wait for lper to provide liquidity");
    }

    suite.app.update_block(|b| {
        b.height += 5;
        b.time = b.time.plus_seconds(15)
    });

    // Verify lper has the lp tokens after providing liquidity
    let lper_lp_token_balance = suite
        .app
        .wrap()
        .query_wasm_smart::<cw20::BalanceResponse>(
            suite.lp_token_addr.clone(),
            &cw20::Cw20QueryMsg::Balance {
                address: suite.lper_addr.to_string(),
            },
        )
        .unwrap();
    assert!(lper_lp_token_balance.balance > Uint128::zero());

    // Try to claim, but we still in the lockup period so this should fail.
    suite
        .app
        .execute_contract(
            suite.party_local_receiver.clone(),
            suite.holder_addr.clone(),
            &covenant_single_party_pol_holder::msg::ExecuteMsg::Claim {},
            &[],
        )
        .unwrap_err();

    // pass the lockup period, and try to withdraw the liquidity
    suite.app.update_block(|b| {
        b.height += 100000;
        b.time = b.time.plus_seconds(100000 * 3)
    });

    suite
        .app
        .execute_contract(
            suite.party_local_receiver.clone(),
            suite.holder_addr.clone(),
            &covenant_single_party_pol_holder::msg::ExecuteMsg::Claim {},
            &[],
        )
        .unwrap();

    let router_addr = suite
        .app
        .wrap()
        .query_wasm_smart::<Addr>(
            suite.covenant_addr.clone(),
            &covenant_single_party_pol::msg::QueryMsg::InterchainRouterAddress {},
        )
        .unwrap();

    while suite
        .app
        .wrap()
        .query_all_balances(suite.party_receiver.clone())
        .unwrap()
        .is_empty()
    {
        suite.tick("Wait for party_receiver to get funds");
    }

    let receiver_balance = suite
        .app
        .wrap()
        .query_balance(suite.party_receiver.clone(), DENOM_ATOM)
        .unwrap();

    // We used pfm, so the receiver should have close to 1_000_000_000_000 uatom
    assert!(receiver_balance.amount.u128() > 900_000_000_000_u128);
}

#[test]
fn test_covenant_with_uneven_pool_stable() {
    let mut suite = Suite::new_with_stable_pool();

    suite.astro_swap(coin(512_345_678_987, DENOM_ATOM_ON_NTRN));
    suite.astro_swap(coin(712_345_678_987, DENOM_LS_ATOM_ON_NTRN));

    suite.get_and_fund_depositors(coin(1_000_000_000_000_u128, DENOM_ATOM));

    // Verify forwarders got their split from the splitter
    let lp_forwarder_ica = suite.get_ica(suite.lp_forwarder_addr.clone());
    let ls_forwarder_ica = suite.get_ica(suite.ls_forwarder_addr.clone());

    while suite
        .app
        .wrap()
        .query_all_balances(lp_forwarder_ica.clone())
        .unwrap()
        .is_empty()
    {
        suite.tick("Wait for lp_forwarder ICA to get its split");
    }

    while suite
        .app
        .wrap()
        .query_all_balances(ls_forwarder_ica.clone())
        .unwrap()
        .is_empty()
    {
        suite.tick("Wait for lp_forwarder ICA to get its split");
    }

    let lp_forwarder_ica_balance = suite
        .app
        .wrap()
        .query_balance(lp_forwarder_ica.clone(), DENOM_ATOM)
        .unwrap();
    let ls_forwarder_ica_balance = suite
        .app
        .wrap()
        .query_balance(ls_forwarder_ica.clone(), DENOM_ATOM)
        .unwrap();

    assert_eq!(lp_forwarder_ica_balance.amount.u128(), 500_000_000_000_u128);
    assert_eq!(ls_forwarder_ica_balance.amount.u128(), 500_000_000_000_u128);

    // Wait for forwarders to forward the funds to the correct addrs
    let lser_ica = suite.get_ica(suite.lser_addr.clone());

    // lser_ica should get his half on stride (lsAtom on stride)
    while suite
        .app
        .wrap()
        .query_all_balances(lser_ica.clone())
        .unwrap()
        .is_empty()
    {
        suite.tick("Wait for lser ICA to get his lsAtom");
    }

    // lper should get his atom (atom on neutron)
    while suite
        .app
        .wrap()
        .query_all_balances(suite.lper_addr.clone())
        .unwrap()
        .is_empty()
    {
        suite.tick("Wait for lser ICA to get his lsAtom");
    }

    // Make sure the correct denoms are received on the correct addrs
    let lser_ica_balance = suite
        .app
        .wrap()
        .query_balance(lser_ica, DENOM_LS_ATOM_ON_STRIDE)
        .unwrap();
    let lper_balance = suite
        .app
        .wrap()
        .query_balance(suite.lper_addr.clone(), DENOM_ATOM_ON_NTRN)
        .unwrap();

    assert_eq!(lser_ica_balance.amount.u128(), 500_000_000_000_u128);
    assert_eq!(lper_balance.amount.u128(), 500_000_000_000_u128);

    // TODO: Currently we need to manually send the LS tokens from stride to the lper
    // TODO: When autopilot will be able to auto send over IBC, we can wait on the lper to receive both denoms
    suite
        .app
        .execute_contract(
            suite.admin.clone(),
            suite.lser_addr.clone(),
            &covenant_stride_liquid_staker::msg::ExecuteMsg::Transfer {
                amount: 500_000_000_000_u128.into(),
            },
            &[],
        )
        .unwrap();

    // We only check that lper got the ls tokens, as we already have the native atom check
    let lper_balance = suite
        .app
        .wrap()
        .query_balance(suite.lper_addr.clone(), DENOM_LS_ATOM_ON_NTRN)
        .unwrap();
    assert_eq!(lper_balance.amount.u128(), 500_000_000_000_u128);

    // Wait until lper provide liquidity
    while suite
        .app
        .wrap()
        .query_balance(suite.lper_addr.clone(), DENOM_LS_ATOM_ON_NTRN)
        .unwrap()
        .amount
        .u128()
        > 100_000_000_000_u128
    {
        suite.tick("Wait for lper to provide liquidity");
    }

    // We provided liquidty but the pool is out of range for our single sided liquidity, so we should have leftovers
    let lper_balance = suite
        .app
        .wrap()
        .query_all_balances(suite.lper_addr.clone())
        .unwrap();
    assert!(lper_balance.len() == 1);
    assert!(lper_balance[0].amount.u128() > 10_000_000_u128);

    suite.app.update_block(|b| {
        b.height += 5;
        b.time = b.time.plus_seconds(15)
    });

    // Verify lper has the lp tokens after providing liquidity
    let lper_lp_token_balance = suite
        .app
        .wrap()
        .query_wasm_smart::<cw20::BalanceResponse>(
            suite.lp_token_addr.clone(),
            &cw20::Cw20QueryMsg::Balance {
                address: suite.lper_addr.to_string(),
            },
        )
        .unwrap();
    assert!(lper_lp_token_balance.balance > Uint128::zero());

    // Try to claim, but we still in the lockup period so this should fail.
    suite
        .app
        .execute_contract(
            suite.party_local_receiver.clone(),
            suite.holder_addr.clone(),
            &covenant_single_party_pol_holder::msg::ExecuteMsg::Claim {},
            &[],
        )
        .unwrap_err();

    // pass the lockup period, and try to withdraw the liquidity
    suite.app.update_block(|b| {
        b.height += 100000;
        b.time = b.time.plus_seconds(100000 * 3)
    });

    suite
        .app
        .execute_contract(
            suite.party_local_receiver.clone(),
            suite.holder_addr.clone(),
            &covenant_single_party_pol_holder::msg::ExecuteMsg::Claim {},
            &[],
        )
        .unwrap();

    let router_addr = suite
        .app
        .wrap()
        .query_wasm_smart::<Addr>(
            suite.covenant_addr.clone(),
            &covenant_single_party_pol::msg::QueryMsg::InterchainRouterAddress {},
        )
        .unwrap();

    while suite
        .app
        .wrap()
        .query_all_balances(suite.party_receiver.clone())
        .unwrap()
        .is_empty()
    {
        suite.tick("Wait for party_receiver to get funds");
    }

    let receiver_balance = suite
        .app
        .wrap()
        .query_balance(suite.party_receiver.clone(), DENOM_ATOM)
        .unwrap();

    // We used pfm, so the receiver should have close to 1_000_000_000_000 uatom
    assert!(receiver_balance.amount.u128() > 900_000_000_000_u128);
}

#[test]
fn test_covenant_with_single_sided() {
    let mut suite = Suite::new_with_stable_pool();

    suite.astro_swap(coin(345_678_987, DENOM_ATOM_ON_NTRN));

    suite.get_and_fund_depositors(coin(1_000_000_000_000_u128, DENOM_ATOM));

    // Verify forwarders got their split from the splitter
    let lp_forwarder_ica = suite.get_ica(suite.lp_forwarder_addr.clone());
    let ls_forwarder_ica = suite.get_ica(suite.ls_forwarder_addr.clone());

    while suite
        .app
        .wrap()
        .query_all_balances(lp_forwarder_ica.clone())
        .unwrap()
        .is_empty()
    {
        suite.tick("Wait for lp_forwarder ICA to get its split");
    }

    while suite
        .app
        .wrap()
        .query_all_balances(ls_forwarder_ica.clone())
        .unwrap()
        .is_empty()
    {
        suite.tick("Wait for lp_forwarder ICA to get its split");
    }

    let lp_forwarder_ica_balance = suite
        .app
        .wrap()
        .query_balance(lp_forwarder_ica.clone(), DENOM_ATOM)
        .unwrap();
    let ls_forwarder_ica_balance = suite
        .app
        .wrap()
        .query_balance(ls_forwarder_ica.clone(), DENOM_ATOM)
        .unwrap();

    assert_eq!(lp_forwarder_ica_balance.amount.u128(), 500_000_000_000_u128);
    assert_eq!(ls_forwarder_ica_balance.amount.u128(), 500_000_000_000_u128);

    // Wait for forwarders to forward the funds to the correct addrs
    let lser_ica = suite.get_ica(suite.lser_addr.clone());

    // lser_ica should get his half on stride (lsAtom on stride)
    while suite
        .app
        .wrap()
        .query_all_balances(lser_ica.clone())
        .unwrap()
        .is_empty()
    {
        suite.tick("Wait for lser ICA to get his lsAtom");
    }

    // lper should get his atom (atom on neutron)
    while suite
        .app
        .wrap()
        .query_all_balances(suite.lper_addr.clone())
        .unwrap()
        .is_empty()
    {
        suite.tick("Wait for lser ICA to get his lsAtom");
    }

    // Make sure the correct denoms are received on the correct addrs
    let lser_ica_balance = suite
        .app
        .wrap()
        .query_balance(lser_ica, DENOM_LS_ATOM_ON_STRIDE)
        .unwrap();
    let lper_balance = suite
        .app
        .wrap()
        .query_balance(suite.lper_addr.clone(), DENOM_ATOM_ON_NTRN)
        .unwrap();

    assert_eq!(lser_ica_balance.amount.u128(), 500_000_000_000_u128);
    assert_eq!(lper_balance.amount.u128(), 500_000_000_000_u128);

    // TODO: Currently we need to manually send the LS tokens from stride to the lper
    // TODO: When autopilot will be able to auto send over IBC, we can wait on the lper to receive both denoms
    suite
        .app
        .execute_contract(
            suite.admin.clone(),
            suite.lser_addr.clone(),
            &covenant_stride_liquid_staker::msg::ExecuteMsg::Transfer {
                amount: 500_000_000_000_u128.into(),
            },
            &[],
        )
        .unwrap();

    // We only check that lper got the ls tokens, as we already have the native atom check
    let lper_balance = suite
        .app
        .wrap()
        .query_balance(suite.lper_addr.clone(), DENOM_LS_ATOM_ON_NTRN)
        .unwrap();
    assert_eq!(lper_balance.amount.u128(), 500_000_000_000_u128);

    // Wait until lper provide liquidity
    while suite
        .app
        .wrap()
        .query_balance(suite.lper_addr.clone(), DENOM_LS_ATOM_ON_NTRN)
        .unwrap()
        .amount
        .u128()
        > 100_000_000_000_u128
    {
        suite.tick("Wait for lper to provide liquidity");
    }

    // do couple more ticks to provide single sided liquidity
    suite.tick("Wait for lper to provide single sided liquidity");
    suite.tick("Wait for lper to provide single sided liquidity");
    suite.tick("Wait for lper to provide single sided liquidity");
    suite.tick("Wait for lper to provide single sided liquidity");
    suite.tick("Wait for lper to provide single sided liquidity");
    suite.tick("Wait for lper to provide single sided liquidity");
    suite.tick("Wait for lper to provide single sided liquidity");
    suite.tick("Wait for lper to provide single sided liquidity");
    suite.tick("Wait for lper to provide single sided liquidity");
    suite.tick("Wait for lper to provide single sided liquidity");

    // We provided liquidty but the pool is out of range for our single sided liquidity, so we should have leftovers
    let lper_balance = suite
        .app
        .wrap()
        .query_all_balances(suite.lper_addr.clone())
        .unwrap();
    assert!(lper_balance.is_empty());

    suite.app.update_block(|b| {
        b.height += 5;
        b.time = b.time.plus_seconds(15)
    });

    // Verify lper has the lp tokens after providing liquidity
    let lper_lp_token_balance = suite
        .app
        .wrap()
        .query_wasm_smart::<cw20::BalanceResponse>(
            suite.lp_token_addr.clone(),
            &cw20::Cw20QueryMsg::Balance {
                address: suite.lper_addr.to_string(),
            },
        )
        .unwrap();
    assert!(lper_lp_token_balance.balance > Uint128::zero());

    // Try to claim, but we still in the lockup period so this should fail.
    suite
        .app
        .execute_contract(
            suite.party_local_receiver.clone(),
            suite.holder_addr.clone(),
            &covenant_single_party_pol_holder::msg::ExecuteMsg::Claim {},
            &[],
        )
        .unwrap_err();

    // pass the lockup period, and try to withdraw the liquidity
    suite.app.update_block(|b| {
        b.height += 100000;
        b.time = b.time.plus_seconds(100000 * 3)
    });

    suite
        .app
        .execute_contract(
            suite.party_local_receiver.clone(),
            suite.holder_addr.clone(),
            &covenant_single_party_pol_holder::msg::ExecuteMsg::Claim {},
            &[],
        )
        .unwrap();

    let router_addr = suite
        .app
        .wrap()
        .query_wasm_smart::<Addr>(
            suite.covenant_addr.clone(),
            &covenant_single_party_pol::msg::QueryMsg::InterchainRouterAddress {},
        )
        .unwrap();

    // let router_balances = suite
    //     .app
    //     .wrap()
    //     .query_all_balances(router_addr.clone())
    //     .unwrap();
    // println!("router balances: {router_balances:?}");

    while suite
        .app
        .wrap()
        .query_all_balances(suite.party_receiver.clone())
        .unwrap()
        .is_empty()
    {
        suite.tick("Wait for party_receiver to get funds");
    }

    let receiver_balance = suite
        .app
        .wrap()
        .query_balance(suite.party_receiver.clone(), DENOM_ATOM)
        .unwrap();

    // We used pfm, so the receiver should have close to 1_000_000_000_000 uatom
    assert!(receiver_balance.amount.u128() > 900_000_000_000_u128);
}