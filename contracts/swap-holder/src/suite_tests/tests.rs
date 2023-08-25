use cosmwasm_std::{Addr, Uint128, Timestamp, Coin};
use covenant_utils::{LockupConfig, CovenantParty, RefundConfig, CovenantPartiesConfig, CovenantTerms, SwapCovenantTerms};

use crate::{msg::ContractState, suite_tests::suite::{PARTY_A_ADDR, DENOM_A, PARTY_B_ADDR, DENOM_B, CLOCK_ADDR, INITIAL_BLOCK_HEIGHT, INITIAL_BLOCK_NANOS}, error::ContractError};

use super::suite::SuiteBuilder;

#[test]
fn test_instantiate_happy_and_query_all() {
    let suite = SuiteBuilder::default().build();
    let next_contract = suite.query_next_contract();
    let clock_address = suite.query_clock_address();
    let lockup_config = suite.query_lockup_config();
    let covenant_parties = suite.query_covenant_parties();
    let covenant_terms = suite.query_covenant_terms();

    assert_eq!(next_contract, "contract0");
    assert_eq!(clock_address, "clock_address");
    assert_eq!(lockup_config, LockupConfig::None);
    assert_eq!(covenant_parties, CovenantPartiesConfig {
        party_a: CovenantParty {
            addr: Addr::unchecked(PARTY_A_ADDR.to_string()),
            provided_denom: DENOM_A.to_string(),
            refund_config: RefundConfig::Native(Addr::unchecked(PARTY_A_ADDR.to_string())),
        },
        party_b: CovenantParty {
            addr: Addr::unchecked(PARTY_B_ADDR.to_string()),
            provided_denom: DENOM_B.to_string(),
            refund_config: RefundConfig::Native(Addr::unchecked(PARTY_B_ADDR.to_string())),
        },
    });
    assert_eq!(covenant_terms, CovenantTerms::TokenSwap(SwapCovenantTerms {
        party_a_amount: Uint128::new(400),
        party_b_amount: Uint128::new(20),
    }));
}

#[test]
#[should_panic(expected = "invalid lockup config: block height must be in the future")]
fn test_instantiate_past_lockup_block_height() {
    SuiteBuilder::default()
        .with_lockup_config(LockupConfig::Block(1))
        .build();
}

#[test]
#[should_panic(expected = "invalid lockup config: block time must be in the future")]
fn test_instantiate_past_lockup_block_time() {
    SuiteBuilder::default()
        .with_lockup_config(LockupConfig::Time(Timestamp::from_seconds(1)))
        .build();
}

#[test]
fn test_tick_unauthorized() {
    let mut suite = SuiteBuilder::default().build();
    println!("{}", suite.app.block_info().height);
    let resp = suite.tick("not-the-clock")
        .unwrap_err()
        .downcast()
        .unwrap();

    assert!(matches!(resp, ContractError::Unauthorized {}))
}

#[test]
fn test_forward_block_expired_covenant() {
    let mut suite = SuiteBuilder::default()
        .with_lockup_config(LockupConfig::Block(INITIAL_BLOCK_HEIGHT + 50))
        .build();
    suite.pass_blocks(100);

    let state = suite.query_contract_state();
    assert_eq!(state, ContractState::Instantiated);
    suite.tick(CLOCK_ADDR).unwrap();

    let state = suite.query_contract_state();
    assert_eq!(state, ContractState::Expired);
}

#[test]
fn test_forward_time_expired_covenant() {
    let mut suite = SuiteBuilder::default()
        .with_lockup_config(LockupConfig::Time(Timestamp::from_nanos(
            INITIAL_BLOCK_NANOS + 50
        )))
        .build();
    suite.pass_minutes(100);

    let state = suite.query_contract_state();
    assert_eq!(state, ContractState::Instantiated);
    suite.tick(CLOCK_ADDR).unwrap();

    let state = suite.query_contract_state();
    assert_eq!(state, ContractState::Expired);
}


#[test]
#[should_panic(expected = "Insufficient funds to forward")]
fn test_forward_tick_insufficient_funds() {
    let mut suite = SuiteBuilder::default().build();

    suite.fund_coin(Coin {
        denom: DENOM_A.to_string(),
        amount: Uint128::new(10),
    });
    suite.fund_coin(Coin {
        denom: DENOM_B.to_string(),
        amount: Uint128::new(10),
    });

    suite.tick(CLOCK_ADDR).unwrap();
}

#[test]
fn test_covenant_query_endpoint() {
    let mut suite = SuiteBuilder::default().build();
    let coin_a = Coin {
        denom: DENOM_A.to_string(),
        amount: Uint128::new(500),
    };
    let coin_b = Coin {
        denom: DENOM_B.to_string(),
        amount: Uint128::new(500),
    };
    suite.fund_coin(coin_a.clone());
    suite.fund_coin(coin_b.clone());

    suite.tick(CLOCK_ADDR).unwrap();
    suite.pass_blocks(10);

    let state = suite.query_contract_state();
    assert_eq!(state, ContractState::Complete);
    
    let splitter_balances = suite.query_native_splitter_balances();
    assert_eq!(2, splitter_balances.len());
    assert_eq!(coin_a, splitter_balances[0]);
    assert_eq!(coin_b, splitter_balances[1]);

    let resp: String = suite.app
        .wrap()
        .query_wasm_smart(suite.mock_deposit, &covenant_utils::neutron_ica::QueryMsg::DepositAddress {})
        .unwrap();

    println!("resp: {:?}", resp);
}


#[test]
fn test_forward_tick() {
    let mut suite = SuiteBuilder::default().build();
    let coin_a = Coin {
        denom: DENOM_A.to_string(),
        amount: Uint128::new(500),
    };
    let coin_b = Coin {
        denom: DENOM_B.to_string(),
        amount: Uint128::new(500),
    };

    suite.fund_coin(coin_a.clone());
    suite.fund_coin(coin_b.clone());

    suite.tick(CLOCK_ADDR).unwrap();
    suite.pass_blocks(10);

    let state = suite.query_contract_state();
    assert_eq!(state, ContractState::Complete);
    
    let splitter_balances = suite.query_native_splitter_balances();
    assert_eq!(2, splitter_balances.len());
    assert_eq!(coin_a, splitter_balances[0]);
    assert_eq!(coin_b, splitter_balances[1]);
}

#[test]
fn test_refund_nothing_to_refund() {
    let mut suite = SuiteBuilder::default()
        .with_lockup_config(LockupConfig::Block(21345))
        .build();

    suite.pass_blocks(10000);

    // first tick acknowledges the expiration
    suite.tick(CLOCK_ADDR).unwrap();
    let state = suite.query_contract_state();
    assert_eq!(state, ContractState::Expired);
    
    // second tick completes
    suite.tick(CLOCK_ADDR).unwrap();
    let state = suite.query_contract_state();
    assert_eq!(state, ContractState::Complete);

    let party_a_bal = suite.query_party_denom(DENOM_A.to_string(), suite.party_a.addr.to_string());
    let party_b_bal = suite.query_party_denom(DENOM_B.to_string(), suite.party_b.addr.to_string());

    assert_eq!(Uint128::zero(), party_a_bal.amount);
    assert_eq!(Uint128::zero(), party_b_bal.amount);
}


#[test]
fn test_refund_party_a() {
    let mut suite = SuiteBuilder::default()
        .with_lockup_config(LockupConfig::Block(21345))
        .build();
    
    let coin_a = Coin {
        denom: DENOM_A.to_string(),
        amount: Uint128::new(500),
    };

    suite.fund_coin(coin_a.clone());
    suite.pass_blocks(10000);

    // first tick acknowledges the expiration
    suite.tick(CLOCK_ADDR).unwrap();
    let state = suite.query_contract_state();
    assert_eq!(state, ContractState::Expired);
    
    // second tick refunds
    suite.tick(CLOCK_ADDR).unwrap();
    // third tick acknowledges the refund and completes
    suite.tick(CLOCK_ADDR).unwrap();
    let state = suite.query_contract_state();
    assert_eq!(state, ContractState::Complete);

    let party_a_bal = suite.query_party_denom(DENOM_A.to_string(), suite.party_a.addr.to_string());
    let party_b_bal = suite.query_party_denom(DENOM_B.to_string(), suite.party_b.addr.to_string());

    assert_eq!(Uint128::new(500), party_a_bal.amount);
    assert_eq!(Uint128::zero(), party_b_bal.amount);
}


#[test]
fn test_refund_party_b() {
    let mut suite = SuiteBuilder::default()
        .with_lockup_config(LockupConfig::Block(21345))
        .build();
    
    let coin_b = Coin {
        denom: DENOM_B.to_string(),
        amount: Uint128::new(500),
    };
    suite.fund_coin(coin_b.clone());
    
    suite.pass_blocks(10000);

    // first tick acknowledges the expiration
    suite.tick(CLOCK_ADDR).unwrap();
    let state = suite.query_contract_state();
    assert_eq!(state, ContractState::Expired);

    // second refunds
    suite.tick(CLOCK_ADDR).unwrap();
    // third tick completes
    suite.tick(CLOCK_ADDR).unwrap();

    let state = suite.query_contract_state();
    assert_eq!(state, ContractState::Complete);

    let party_a_bal = suite.query_party_denom(DENOM_A.to_string(), suite.party_a.addr.to_string());
    let party_b_bal = suite.query_party_denom(DENOM_B.to_string(), suite.party_b.addr.to_string());

    assert_eq!(Uint128::zero(), party_a_bal.amount);
    assert_eq!(Uint128::new(500), party_b_bal.amount);
}



#[test]
fn test_refund_both_parties() {
    let mut suite = SuiteBuilder::default()
        .with_lockup_config(LockupConfig::Block(21345))
        .build();
    let coin_a = Coin {
        denom: DENOM_A.to_string(),
        amount: Uint128::new(300),
    };
    suite.fund_coin(coin_a.clone());
    let coin_b = Coin {
        denom: DENOM_B.to_string(),
        amount: Uint128::new(10),
    };
    suite.fund_coin(coin_b.clone());

    suite.pass_blocks(10000);

    // first tick acknowledges the expiration
    suite.tick(CLOCK_ADDR).unwrap();
    let state = suite.query_contract_state();
    assert_eq!(state, ContractState::Expired);

    // second tick refunds the parties
    suite.tick(CLOCK_ADDR).unwrap();
    // third tick acknowledges the refund and completes
    suite.tick(CLOCK_ADDR).unwrap();

    let state = suite.query_contract_state();
    assert_eq!(state, ContractState::Complete);

    let party_a_bal = suite.query_party_denom(DENOM_A.to_string(), suite.party_a.addr.to_string());
    let party_b_bal = suite.query_party_denom(DENOM_B.to_string(), suite.party_b.addr.to_string());

    assert_eq!(Uint128::new(300), party_a_bal.amount);
    assert_eq!(Uint128::new(10), party_b_bal.amount);
}
