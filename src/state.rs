use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_bignumber::Decimal256;
use cosmwasm_std::{CanonicalAddr, Storage, Uint128};
use cosmwasm_storage::{singleton, singleton_read, ReadonlySingleton, Singleton};

const KEY_CONFIG: &[u8] = b"config";
const KEY_STATE: &[u8] = b"state";

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Config {
    pub owner_addr: CanonicalAddr,
    pub stable_denom: String,
    pub eterra_contract: CanonicalAddr,
    pub terraswap_factory: CanonicalAddr,
    pub alloc_luna: Decimal256,
    pub alloc_anc: Decimal256,
    pub alloc_mir: Decimal256,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct State {
    pub total_supply: Uint128,
    pub reserve_luna: Decimal256,
    pub reserve_anc: Decimal256,
    pub reserve_mir: Decimal256,
}

pub fn store_config<S: Storage>(storage: &mut S) -> Singleton<S, Config> {
    singleton(storage, KEY_CONFIG)
}

pub fn read_config<S: Storage>(storage: &S) -> ReadonlySingleton<S, Config> {
    singleton_read(storage, KEY_CONFIG)
}

pub fn store_state<S: Storage>(storage: &mut S) -> Singleton<S, State> {
    singleton(storage, KEY_STATE)
}

pub fn read_state<S: Storage>(storage: &S) -> ReadonlySingleton<S, State> {
    singleton_read(storage, KEY_STATE)
}
