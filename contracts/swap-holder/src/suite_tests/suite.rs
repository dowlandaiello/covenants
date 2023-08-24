use crate::msg::{ExecuteMsg, InstantiateMsg, LockupConfig, CovenantPartiesConfig, CovenantTerms, CovenantParty, RefundConfig, QueryMsg, ContractState};
use cosmwasm_std::{Addr, Uint128, Coin};
use cw_multi_test::{App, AppResponse, Executor, SudoMsg};

use super::swap_holder_contract;

pub const ADMIN: &str = "admin";

pub const DENOM_A: &str = "denom_a";
pub const DENOM_B: &str = "denom_b";

pub const PARTY_A_ADDR: &str = "party_a";
pub const PARTY_B_ADDR: &str = "party_b";

pub const CLOCK_ADDR: &str = "clock_address";
pub const NEXT_CONTRACT: &str = "next_contract";

pub const INITIAL_BLOCK_HEIGHT: u64 = 12345;
pub const INITIAL_BLOCK_NANOS: u64 = 1571797419879305533;

pub struct Suite {
    pub app: App,
    // pub covenant_terms: CovenantTerms,
    // pub covenant_paries: CovenantPartiesConfig,
    // pub lockup_config: LockupConfig,
    // pub clock_address: String,
    // pub next_contract: String,
    pub holder: Addr,
}

pub struct SuiteBuilder {
    pub instantiate: InstantiateMsg,
    pub app: App,
}

impl Default for SuiteBuilder {
    fn default() -> Self {
        Self {
            instantiate: InstantiateMsg {
                clock_address: CLOCK_ADDR.to_string(),
                next_contract: NEXT_CONTRACT.to_string(),
                lockup_config: LockupConfig::None,
                parties_config: CovenantPartiesConfig {
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
                },
                covenant_terms: CovenantTerms {
                    party_a_amount: Uint128::new(400),
                    party_b_amount: Uint128::new(20),
                },
            },
            app: App::default(),
        }
    }
}

impl SuiteBuilder {
    pub fn with_lockup_config(mut self, config: LockupConfig) -> Self {
        self.instantiate.lockup_config = config;
        self
    }

    pub fn with_parties_config(mut self, config: CovenantPartiesConfig) -> Self {
        self.instantiate.parties_config = config;
        self
    }

    pub fn with_covenant_terms(mut self, terms: CovenantTerms) -> Self {
        self.instantiate.covenant_terms = terms;
        self
    }

    pub fn build(mut self) -> Suite {
        let mut app = self.app;
        let holder_code = app.store_code(swap_holder_contract());

        let holder = app
            .instantiate_contract(
                holder_code,
                Addr::unchecked(ADMIN),
                &self.instantiate,
                &[],
                "holder",
                Some(ADMIN.to_string()),
            )
            .unwrap();

        Suite {
            app,
            holder,
            // admin: Addr::unchecked(ADMIN),
            // pool_address: self.instantiate.pool_address,
            // covenant_terms: todo!(),
            // covenant_paries: todo!(),
            // lockup_config: todo!(),
            // clock_address: todo!(),
            // next_contract: todo!(),
        }
    }
}

// actions
impl Suite {
    pub fn tick(&mut self, caller: &str) -> Result<AppResponse, anyhow::Error> {
        self.app
            .execute_contract(
                Addr::unchecked(caller),
                self.holder.clone(),
                &ExecuteMsg::Tick {},
                &[],
            )
    }
}

// queries
impl Suite {
    pub fn query_next_contract(&self) -> Addr {
        self.app
            .wrap()
            .query_wasm_smart(&self.holder, &QueryMsg::NextContract {})
            .unwrap()
    }

    pub fn query_lockup_config(&self) -> LockupConfig {
        self.app
            .wrap()
            .query_wasm_smart(&self.holder, &QueryMsg::LockupConfig {})
            .unwrap()
    }

    pub fn query_covenant_parties(&self) -> CovenantPartiesConfig {
        self.app
            .wrap()
            .query_wasm_smart(&self.holder, &QueryMsg::CovenantParties {})
            .unwrap()
    }

    pub fn query_covenant_terms(&self) -> CovenantTerms {
        self.app
            .wrap()
            .query_wasm_smart(&self.holder, &QueryMsg::CovenantTerms {})
            .unwrap()
    }

    pub fn query_clock_address(&self) -> Addr {
        self.app
            .wrap()
            .query_wasm_smart(&self.holder, &QueryMsg::ClockAddress {})
            .unwrap()
    }

    pub fn query_contract_state(&self) -> ContractState {
        self.app
            .wrap()
            .query_wasm_smart(&self.holder, &QueryMsg::ContractState {})
            .unwrap()
    }
}

// helper
impl Suite {
    pub fn pass_blocks(&mut self, n: u64) {
        self.app.update_block(|mut b| b.height += n);
    }

    pub fn pass_minutes(&mut self, n: u64) {
        self.app.update_block(|mut b| b.time = b.time.plus_minutes(n));
    }

    pub fn fund_coin(&mut self, coin: Coin) -> AppResponse {
        self.app
            .sudo(SudoMsg::Bank(
                cw_multi_test::BankSudo::Mint {
                    to_address: self.holder.to_string(),
                    amount: vec![coin],
                },
            ))
            .unwrap()
    }
}
