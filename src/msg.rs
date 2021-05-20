use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_bignumber::{Decimal256, Uint256};
use cosmwasm_std::{CanonicalAddr, HumanAddr, Uint128};
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct InitMsg {
    pub alloc_luna: Decimal256,
    pub alloc_anc: Decimal256,
    pub alloc_mir: Decimal256,
    pub eterra_code_id: u64,
    pub stable_denom: String,
    pub terraswap_factory: HumanAddr,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum HandleMsg {
    Mint {},
    RegisterETerra {},
    Burn {},
    RedeemToken { sender: HumanAddr },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    // GetCount returns the current count as a json-encoded number
    Config {},
    State {},
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct StateResponse {
    pub total_supply: Uint128,
    pub reserve_luna: Decimal256,
    pub reserve_anc: Decimal256,
    pub reserve_mir: Decimal256,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct ConfigResponse {
    pub owner_addr: HumanAddr,
    pub stable_denom: String,
    pub eterra_contract: HumanAddr,
    pub terraswap_factory: HumanAddr,
    pub alloc_luna: Decimal256,
    pub alloc_anc: Decimal256,
    pub alloc_mir: Decimal256,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct SwapRateResponse {
    pub return_amount: Decimal256,
    pub spread_amount: Decimal256,
    pub commission_amount: Decimal256,
}
