use cosmwasm_std::{Decimal, Timestamp, Uint128};
use covenant_utils::ExpiryConfig;

use crate::{
    error::ContractError,
    msg::{ContractState, RagequitConfig, RagequitTerms},
    suite_tests::suite::{
        get_default_block_info, CLOCK_ADDR, NEXT_CONTRACT, PARTY_A_ROUTER, PARTY_B_ADDR,
        PARTY_B_ROUTER, POOL,
    },
};

use super::suite::{SuiteBuilder, INITIAL_BLOCK_HEIGHT, INITIAL_BLOCK_NANOS, PARTY_A_ADDR};

#[test]
fn test_instantiate_happy_and_query_all() {
    let suite = SuiteBuilder::default().build();
    let clock = suite.query_clock_address();
    let pool = suite.query_pool();
    let next_contract = suite.query_next_contract();
    let config_party_a = suite.query_party_a();
    let config_party_b = suite.query_party_b();
    let deposit_deadline = suite.query_deposit_deadline();
    let contract_state = suite.query_contract_state();
    let lockup_config = suite.query_lockup_config();

    assert_eq!(ContractState::Instantiated, contract_state);
    assert_eq!(CLOCK_ADDR, clock);
    assert_eq!(POOL, pool);
    assert_eq!(NEXT_CONTRACT, next_contract.to_string());
    assert_eq!(PARTY_A_ROUTER, config_party_a.router);
    assert_eq!(PARTY_B_ROUTER, config_party_b.router);
    assert_eq!(ExpiryConfig::None, deposit_deadline);
    assert_eq!(ExpiryConfig::None, lockup_config);
}

#[test]
#[should_panic(expected = "Ragequit penalty must be in range of [0.0, 1.0)")]
fn test_invalid_ragequit_penalty() {
    SuiteBuilder::default()
        .with_ragequit_config(RagequitConfig::Enabled(RagequitTerms {
            penalty: Decimal::one(),
            state: None,
        }))
        .build();
}

#[test]
#[should_panic(expected = "Ragequit penalty exceeds party allocation")]
fn test_ragequit_penalty_exceeds_either_party_allocation() {
    SuiteBuilder::default()
        .with_ragequit_config(RagequitConfig::Enabled(RagequitTerms {
            penalty: Decimal::percent(51),
            state: None,
        }))
        .build();
}

#[test]
#[should_panic(expected = "party allocations must add up to 1.0")]
fn test_instantiate_invalid_allocations() {
    SuiteBuilder::default()
        .with_allocations(Decimal::percent(4), Decimal::percent(20))
        .build();
}

#[test]
#[should_panic(expected = "block height must be in the future")]
fn test_instantiate_invalid_deposit_deadline_block_based() {
    SuiteBuilder::default()
        .with_deposit_deadline(ExpiryConfig::Block(1))
        .build();
}

#[test]
#[should_panic(expected = "block time must be in the future")]
fn test_instantiate_invalid_deposit_deadline_time_based() {
    SuiteBuilder::default()
        .with_deposit_deadline(ExpiryConfig::Time(Timestamp::from_nanos(1)))
        .build();
}

#[test]
#[should_panic(expected = "invalid expiry config: block time must be in the future")]
fn test_instantiate_invalid_lockup_config_time_based() {
    SuiteBuilder::default()
        .with_lockup_config(ExpiryConfig::Time(Timestamp::from_nanos(
            INITIAL_BLOCK_NANOS - 1,
        )))
        .build();
}

#[test]
#[should_panic(expected = "invalid expiry config: block height must be in the future")]
fn test_instantiate_invalid_lockup_config_height_based() {
    SuiteBuilder::default()
        .with_lockup_config(ExpiryConfig::Block(INITIAL_BLOCK_HEIGHT - 1))
        .build();
}

#[test]
fn test_single_party_deposit_refund_block_based() {
    let mut suite = SuiteBuilder::default()
        .with_deposit_deadline(ExpiryConfig::Block(12545))
        .build();

    // party A fulfills their part of covenant but B fails to
    let coin = suite.get_party_a_coin(Uint128::new(500));
    suite.fund_coin(coin);

    // time passes, clock ticks..
    suite.pass_blocks(250);
    suite.tick(CLOCK_ADDR).unwrap();
    suite.tick(CLOCK_ADDR).unwrap();

    let holder_balance = suite.get_denom_a_balance(suite.holder.to_string());
    let router_a_balance = suite.get_denom_a_balance(suite.query_party_a().router);
    let holder_state = suite.query_contract_state();

    assert_eq!(ContractState::Complete, holder_state);
    assert_eq!(Uint128::zero(), holder_balance);
    assert_eq!(Uint128::new(500), router_a_balance);
}

#[test]
fn test_single_party_deposit_refund_time_based() {
    let current_timestamp = get_default_block_info();
    let mut suite = SuiteBuilder::default()
        .with_deposit_deadline(ExpiryConfig::Time(current_timestamp.time.plus_minutes(200)))
        .build();

    // party A fulfills their part of covenant but B fails to
    let coin = suite.get_party_a_coin(Uint128::new(500));
    suite.fund_coin(coin);

    // time passes, clock ticks..
    suite.pass_minutes(250);
    suite.tick(CLOCK_ADDR).unwrap();
    suite.tick(CLOCK_ADDR).unwrap();

    let holder_balance = suite.get_denom_a_balance(suite.holder.to_string());
    let router_a_balance = suite.get_denom_a_balance(suite.query_party_a().router);
    let holder_state = suite.query_contract_state();

    assert_eq!(ContractState::Complete, holder_state);
    assert_eq!(Uint128::zero(), holder_balance);
    assert_eq!(Uint128::new(500), router_a_balance);
}

#[test]
fn test_single_party_deposit_refund_no_deposit_deadline() {
    let mut suite = SuiteBuilder::default().build();

    // party A fulfills their part of covenant but B fails to
    let coin = suite.get_party_a_coin(Uint128::new(500));
    suite.fund_coin(coin);

    // time passes, clock ticks..
    suite.pass_minutes(25000000);
    suite.tick(CLOCK_ADDR);
    suite.tick(CLOCK_ADDR);
    let resp: ContractError = suite.tick(CLOCK_ADDR).unwrap_err().downcast().unwrap();

    // we assert that holder still holds the tokens and did not advance the state
    let holder_balance = suite.get_denom_a_balance(suite.holder.to_string());
    let holder_state = suite.query_contract_state();

    assert_eq!(ContractState::Instantiated, holder_state);
    assert_eq!(Uint128::new(500), holder_balance);
    assert_eq!(ContractError::InsufficientDeposits {}, resp);
}

#[test]
fn test_holder_active_does_not_allow_claims() {
    // unimplemented!()
}

#[test]
fn test_holder_active_not_expired_ticks() {
    let current_timestamp = get_default_block_info();
    let mut suite = SuiteBuilder::default()
        .with_deposit_deadline(ExpiryConfig::Time(current_timestamp.time.plus_minutes(200)))
        .build();

    // both parties fulfill their parts of the covenant
    let coin_a = suite.get_party_a_coin(Uint128::new(500));
    let coin_b = suite.get_party_b_coin(Uint128::new(500));
    suite.fund_coin(coin_a);
    suite.fund_coin(coin_b);

    // we tick the holder to deposit the funds and activate
    suite.tick(CLOCK_ADDR).unwrap();

    // time passes, clock ticks..
    suite.pass_minutes(50);
    let resp = suite.tick(CLOCK_ADDR).unwrap();

    let has_not_due_attribute = resp
        .events
        .into_iter()
        .flat_map(|e| e.attributes)
        .any(|attr| attr.value == "not_due");
    let holder_balance_a = suite.get_denom_a_balance(suite.holder.to_string());
    let holder_balance_b = suite.get_denom_b_balance(suite.holder.to_string());
    let splitter_balance_a = suite.get_denom_a_balance(suite.mock_deposit.to_string());
    let splitter_balance_b = suite.get_denom_b_balance(suite.mock_deposit.to_string());
    let holder_state = suite.query_contract_state();

    assert!(has_not_due_attribute);
    assert_eq!(ContractState::Active, holder_state);
    assert_eq!(Uint128::zero(), holder_balance_b);
    assert_eq!(Uint128::zero(), holder_balance_a);
    assert_eq!(Uint128::new(500), splitter_balance_b);
    assert_eq!(Uint128::new(500), splitter_balance_a);
}

#[test]
fn test_holder_active_expired_tick_advances_state() {
    let current_timestamp = get_default_block_info();
    let mut suite = SuiteBuilder::default()
        .with_lockup_config(ExpiryConfig::Time(current_timestamp.time.plus_minutes(200)))
        .build();

    // both parties fulfill their parts of the covenant
    let coin_a = suite.get_party_a_coin(Uint128::new(500));
    let coin_b = suite.get_party_b_coin(Uint128::new(500));
    suite.fund_coin(coin_a);
    suite.fund_coin(coin_b);

    // we tick the holder to deposit the funds and activate
    suite.tick(CLOCK_ADDR).unwrap();

    // time passes, clock ticks..
    suite.pass_minutes(250);
    suite.tick(CLOCK_ADDR).unwrap();

    let holder_balance_a = suite.get_denom_a_balance(suite.holder.to_string());
    let holder_balance_b = suite.get_denom_b_balance(suite.holder.to_string());
    let splitter_balance_a = suite.get_denom_a_balance(suite.mock_deposit.to_string());
    let splitter_balance_b = suite.get_denom_b_balance(suite.mock_deposit.to_string());
    let holder_state = suite.query_contract_state();

    assert_eq!(ContractState::Expired, holder_state);
    assert_eq!(Uint128::zero(), holder_balance_b);
    assert_eq!(Uint128::zero(), holder_balance_a);
    assert_eq!(Uint128::new(500), splitter_balance_b);
    assert_eq!(Uint128::new(500), splitter_balance_a);
}

#[test]
fn test_holder_ragequit_disabled() {
    let mut suite = SuiteBuilder::default()
        .with_ragequit_config(RagequitConfig::Disabled)
        .build();

    // both parties fulfill their parts of the covenant
    let coin_a = suite.get_party_a_coin(Uint128::new(500));
    let coin_b = suite.get_party_b_coin(Uint128::new(500));
    suite.fund_coin(coin_a);
    suite.fund_coin(coin_b);

    // we tick the holder to deposit the funds and activate
    suite.tick(CLOCK_ADDR).unwrap();

    suite.pass_minutes(300);

    // advance the state to expired
    suite.tick(CLOCK_ADDR).unwrap();

    let err: ContractError = suite.rq(PARTY_A_ADDR).unwrap_err().downcast().unwrap();
    let state = suite.query_contract_state();

    assert_eq!(ContractState::Active {}, state);
    assert_eq!(ContractError::RagequitDisabled {}, err);
}

#[test]
fn test_holder_ragequit_unauthorized() {
    let mut suite = SuiteBuilder::default()
        .with_ragequit_config(RagequitConfig::Enabled(RagequitTerms {
            penalty: Decimal::from_ratio(Uint128::one(), Uint128::new(10)),
            state: None,
        }))
        .build();

    // both parties fulfill their parts of the covenant
    let coin_a = suite.get_party_a_coin(Uint128::new(500));
    let coin_b = suite.get_party_b_coin(Uint128::new(500));
    suite.fund_coin(coin_a);
    suite.fund_coin(coin_b);

    // we tick the holder to deposit the funds and activate
    suite.tick(CLOCK_ADDR).unwrap();

    suite.pass_minutes(50);

    // advance the state to expired
    suite.tick(CLOCK_ADDR).unwrap();

    let err: ContractError = suite.rq("random_user").unwrap_err().downcast().unwrap();
    let state = suite.query_contract_state();

    assert_eq!(ContractState::Active {}, state);
    assert_eq!(ContractError::Unauthorized {}, err);
}

#[test]
fn test_holder_ragequit_not_in_active_state() {
    let current_timestamp = get_default_block_info();
    let mut suite = SuiteBuilder::default()
        .with_lockup_config(ExpiryConfig::Time(current_timestamp.time.plus_minutes(200)))
        .build();

    // both parties fulfill their parts of the covenant
    let coin_a = suite.get_party_a_coin(Uint128::new(500));
    let coin_b = suite.get_party_b_coin(Uint128::new(500));
    suite.fund_coin(coin_a);
    suite.fund_coin(coin_b);

    // we tick the holder to deposit the funds and activate
    suite.tick(CLOCK_ADDR).unwrap();

    suite.pass_minutes(300);

    // advance the state to expired
    suite.tick(CLOCK_ADDR).unwrap();

    let err: ContractError = suite.rq(PARTY_A_ADDR).unwrap_err().downcast().unwrap();
    let state = suite.query_contract_state();

    assert_eq!(ContractState::Expired {}, state);
    assert_eq!(ContractError::RagequitDisabled {}, err);
}

#[test]
fn test_holder_ragequit_active_but_expired() {
    let current_timestamp = get_default_block_info();
    let mut suite = SuiteBuilder::default()
        .with_ragequit_config(RagequitConfig::Enabled(RagequitTerms {
            penalty: Decimal::bps(10),
            state: None,
        }))
        .with_lockup_config(ExpiryConfig::Time(current_timestamp.time.plus_minutes(200)))
        .build();

    // both parties fulfill their parts of the covenant
    let coin_a = suite.get_party_a_coin(Uint128::new(500));
    let coin_b = suite.get_party_b_coin(Uint128::new(500));
    suite.fund_coin(coin_a);
    suite.fund_coin(coin_b);

    // we tick the holder to deposit the funds and activate
    suite.tick(CLOCK_ADDR).unwrap();

    suite.pass_minutes(300);

    let err: ContractError = suite.rq(PARTY_A_ADDR).unwrap_err().downcast().unwrap();

    assert_eq!(ContractError::Expired {}, err);
}

#[test]
#[should_panic(expected = "covenant is not in active state")]
fn test_ragequit_double_claim_fails() {
    let current_timestamp = get_default_block_info();
    let mut suite = SuiteBuilder::default()
        .with_ragequit_config(RagequitConfig::Enabled(RagequitTerms {
            penalty: Decimal::from_ratio(Uint128::one(), Uint128::new(10)),
            state: None,
        }))
        .with_lockup_config(ExpiryConfig::Time(current_timestamp.time.plus_minutes(200)))
        .build();

    // both parties fulfill their parts of the covenant
    let coin_a = suite.get_party_a_coin(Uint128::new(500));
    let coin_b = suite.get_party_b_coin(Uint128::new(500));
    suite.fund_coin(coin_a);
    suite.fund_coin(coin_b);

    // we tick the holder to deposit the funds and activate
    suite.tick(CLOCK_ADDR).unwrap();

    // we ragequit and assert balances have reached router
    suite.rq(PARTY_A_ADDR).unwrap();

    let router_a_balance = suite.get_denom_a_balance(PARTY_A_ROUTER.to_string());
    let router_b_balance = suite.get_denom_b_balance(PARTY_A_ROUTER.to_string());
    assert_eq!(Uint128::new(200), router_a_balance);
    assert_eq!(Uint128::new(200), router_b_balance);

    let state = suite.query_contract_state();
    let config = suite.query_covenant_config();
    assert_eq!(Decimal::one(), config.party_b.allocation);
    assert_eq!(Decimal::zero(), config.party_a.allocation);
    assert_eq!(ContractState::Ragequit {}, state);

    // we attempt to rq again and panic
    suite.rq(PARTY_A_ADDR).unwrap();
}

#[test]
fn test_ragequit_happy_flow_to_completion() {
    let current_timestamp = get_default_block_info();
    let mut suite = SuiteBuilder::default()
        .with_ragequit_config(RagequitConfig::Enabled(RagequitTerms {
            penalty: Decimal::from_ratio(Uint128::one(), Uint128::new(10)),
            state: None,
        }))
        .with_lockup_config(ExpiryConfig::Time(current_timestamp.time.plus_minutes(200)))
        .build();

    // both parties fulfill their parts of the covenant
    let coin_a = suite.get_party_a_coin(Uint128::new(500));
    let coin_b = suite.get_party_b_coin(Uint128::new(500));
    suite.fund_coin(coin_a);
    suite.fund_coin(coin_b);

    // we tick the holder to deposit the funds and activate
    suite.tick(CLOCK_ADDR).unwrap();

    // party A ragequits; assert balances have reached router
    suite.rq(PARTY_A_ADDR).unwrap();

    let router_a_balance = suite.get_denom_a_balance(PARTY_A_ROUTER.to_string());
    let router_b_balance = suite.get_denom_b_balance(PARTY_A_ROUTER.to_string());
    assert_eq!(Uint128::new(200), router_a_balance);
    assert_eq!(Uint128::new(200), router_b_balance);

    let state = suite.query_contract_state();
    let config = suite.query_covenant_config();
    assert_eq!(Decimal::one(), config.party_b.allocation);
    assert_eq!(Decimal::zero(), config.party_a.allocation);
    assert_eq!(ContractState::Ragequit {}, state);

    // party B claims
    suite.claim(PARTY_B_ADDR).unwrap();

    let router_a_balance = suite.get_denom_a_balance(PARTY_B_ROUTER.to_string());
    let router_b_balance = suite.get_denom_b_balance(PARTY_B_ROUTER.to_string());
    assert_eq!(Uint128::new(200), router_a_balance);
    assert_eq!(Uint128::new(200), router_b_balance);

    let state = suite.query_contract_state();
    let config = suite.query_covenant_config();
    assert_eq!(Decimal::zero(), config.party_b.allocation);
    assert_eq!(Decimal::zero(), config.party_a.allocation);
    assert_eq!(ContractState::Complete {}, state);
}

#[test]
fn test_expiry_happy_flow_to_completion() {
    let current_timestamp = get_default_block_info();
    let mut suite = SuiteBuilder::default()
        .with_lockup_config(ExpiryConfig::Time(current_timestamp.time.plus_minutes(200)))
        .build();

    // both parties fulfill their parts of the covenant
    let coin_a = suite.get_party_a_coin(Uint128::new(500));
    let coin_b = suite.get_party_b_coin(Uint128::new(500));
    suite.fund_coin(coin_a);
    suite.fund_coin(coin_b);

    // we tick the holder to deposit the funds and activate
    suite.tick(CLOCK_ADDR).unwrap();

    suite.pass_minutes(250);

    suite.tick(CLOCK_ADDR).unwrap();

    assert_eq!(ContractState::Expired {}, suite.query_contract_state());
    assert_eq!(
        Uint128::new(0),
        suite.get_denom_a_balance(PARTY_A_ROUTER.to_string())
    );
    assert_eq!(
        Uint128::new(0),
        suite.get_denom_b_balance(PARTY_A_ROUTER.to_string())
    );
    assert_eq!(
        Uint128::new(0),
        suite.get_denom_a_balance(PARTY_B_ROUTER.to_string())
    );
    assert_eq!(
        Uint128::new(0),
        suite.get_denom_b_balance(PARTY_B_ROUTER.to_string())
    );

    // party B claims
    suite.claim(PARTY_B_ADDR).unwrap();

    assert_eq!(
        Uint128::new(0),
        suite.get_denom_a_balance(PARTY_A_ROUTER.to_string())
    );
    assert_eq!(
        Uint128::new(0),
        suite.get_denom_b_balance(PARTY_A_ROUTER.to_string())
    );
    assert_eq!(
        Uint128::new(200),
        suite.get_denom_a_balance(PARTY_B_ROUTER.to_string())
    );
    assert_eq!(
        Uint128::new(200),
        suite.get_denom_b_balance(PARTY_B_ROUTER.to_string())
    );

    suite.pass_minutes(5);

    // party A claims
    suite.claim(PARTY_A_ADDR).unwrap();
    suite.tick(CLOCK_ADDR).unwrap();

    let config = suite.query_covenant_config();
    assert_eq!(Decimal::zero(), config.party_b.allocation);
    assert_eq!(Decimal::zero(), config.party_a.allocation);
    assert_eq!(
        Uint128::new(200),
        suite.get_denom_a_balance(PARTY_A_ROUTER.to_string())
    );
    assert_eq!(
        Uint128::new(200),
        suite.get_denom_b_balance(PARTY_A_ROUTER.to_string())
    );
    assert_eq!(
        Uint128::new(200),
        suite.get_denom_a_balance(PARTY_B_ROUTER.to_string())
    );
    assert_eq!(
        Uint128::new(200),
        suite.get_denom_b_balance(PARTY_B_ROUTER.to_string())
    );
    assert_eq!(ContractState::Complete {}, suite.query_contract_state());
}