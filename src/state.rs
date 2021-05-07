use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_bignumber::{Decimal256, Uint256};
use cosmwasm_std::{CanonicalAddr, Storage};
use cosmwasm_storage::{singleton, singleton_read, ReadonlySingleton, Singleton};

pub static CONFIG_KEY: &[u8] = b"config";

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
    pub struct State {
        pub owner: CanonicalAddr,
        pub total_supply: Decimal256,
        pub reserve_luna: Decimal256,
        pub reserve_anc: Decimal256,
        pub reserve_mir: Decimal256,
        pub eterra_contract: CanonicalAddr,
        pub alloc_luna: Uint256,
        pub alloc_anc: Uint256,
        pub alloc_mir: Uint256,
}

pub fn config<S: Storage>(storage: &mut S) -> Singleton<S, State> {
    singleton(storage, CONFIG_KEY)
}

pub fn config_read<S: Storage>(storage: &S) -> ReadonlySingleton<S, State> {
    singleton_read(storage, CONFIG_KEY)
}
