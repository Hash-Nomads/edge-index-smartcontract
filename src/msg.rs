use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_bignumber::{Decimal256, Uint256};
use cosmwasm_std::HumanAddr;
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct InitMsg {
    pub alloc_luna: Uint256,
    pub alloc_anc: Uint256,
    pub alloc_mir: Uint256,
    pub aterra_code_id: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum HandleMsg {
    Mint {},
    RegisterETerra {},
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    // GetCount returns the current count as a json-encoded number
    GetCount {},
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct StateResponse {
    pub owner: HumanAddr,
    pub total_supply: Decimal256,
    pub reserve_luna: Decimal256,
    pub reserve_anc: Decimal256,
    pub reserve_mir: Decimal256,
    pub eterra_contract: HumanAddr,
    pub alloc_luna: Uint256,
    pub alloc_anc: Uint256,
    pub alloc_mir: Uint256,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct SwapRateResponse {
    pub return_amount: Decimal256,
    pub spread_amount: Decimal256,
    pub commission_amount: Decimal256,
}
