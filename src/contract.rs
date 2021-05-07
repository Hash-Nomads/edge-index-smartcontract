use cosmwasm_bignumber::{Decimal256, Uint256};
use cosmwasm_std::{
    log, to_binary, Api, Binary, CanonicalAddr, Coin, CosmosMsg, Env, Extern, HandleResponse,
    HandleResult, InitResponse, Querier, StdError, StdResult, Storage, Uint128, WasmMsg,
};
use terra_cosmwasm::{create_swap_msg, TerraMsgWrapper, TerraQuerier};

use crate::msg::{HandleMsg, InitMsg, QueryMsg, StateResponse};
use crate::state::{config, config_read, State};
use cw20::{Cw20CoinHuman, Cw20HandleMsg, MinterResponse};
use terraswap::hook::InitHook;
use terraswap::token::InitMsg as TokenInitMsg;

pub fn init<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    msg: InitMsg,
) -> StdResult<InitResponse> {
    let state = State {
        owner: deps.api.canonical_address(&env.message.sender)?,
        total_supply: Decimal256::zero(),
        reserve_luna: Decimal256::zero(),
        reserve_anc: Decimal256::zero(),
        reserve_mir: Decimal256::zero(),
        eterra_contract: CanonicalAddr::default(),
        alloc_luna: msg.alloc_luna,
        alloc_anc: msg.alloc_anc,
        alloc_mir: msg.alloc_mir,
    };

    config(&mut deps.storage).save(&state)?;

    Ok(InitResponse {
        messages: vec![CosmosMsg::Wasm(WasmMsg::Instantiate {
            code_id: msg.aterra_code_id,
            send: vec![],
            label: None,
            msg: to_binary(&TokenInitMsg {
                name: format!("Egde Terra {}", "US".to_uppercase()),
                symbol: format!("e{}T", "US".to_uppercase()),
                decimals: 6u8,
                initial_balances: vec![Cw20CoinHuman {
                    address: env.contract.address.clone(),
                    amount: Uint128(0),
                }],
                mint: Some(MinterResponse {
                    minter: env.contract.address.clone(),
                    cap: None,
                }),
                init_hook: Some(InitHook {
                    contract_addr: env.contract.address,
                    msg: to_binary(&HandleMsg::RegisterETerra {})?,
                }),
            })?,
        })],
        log: vec![],
    })
}

pub fn handle<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    msg: HandleMsg,
) -> StdResult<HandleResponse<TerraMsgWrapper>> {
    match msg {
        HandleMsg::Mint {} => mint(deps, env),
        HandleMsg::RegisterETerra {} => register_aterra(deps, env),
        // HandleMsg::Reset { count } => try_reset(deps, env, count),
    }
}

pub fn register_aterra<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
) -> HandleResult<TerraMsgWrapper> {
    let mut state = config_read(&deps.storage).load()?;
    if state.eterra_contract != CanonicalAddr::default() {
        return Err(StdError::unauthorized());
    }

    state.eterra_contract = deps.api.canonical_address(&env.message.sender)?;
    config(&mut deps.storage).save(&state)?;
    Ok(HandleResponse {
        messages: vec![],
        log: vec![log("aterra", env.message.sender)],
        data: None,
    })
}
pub fn mint<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
) -> StdResult<HandleResponse<TerraMsgWrapper>> {
    let mut state = config_read(&deps.storage).load()?;
    let deposit_amount: Uint256 = env
        .message
        .sent_funds
        .iter()
        .find(|c| c.denom == "uusd")
        .map(|c| Uint256::from(c.amount))
        .unwrap_or_else(Uint256::zero);

    let querier = TerraQuerier::new(&deps.querier);
    let luna_swap_rate: Uint256 = querier
        .query_swap(
            Coin {
                denom: "uluna".to_string(),
                amount: Uint128::from(100000u128),
            },
            "uusd",
        )?
        .receive
        .amount
        .into();
    let anc_swap_rate = Uint256::from(6068u128);
    let mir_swap_rate = Uint256::from(131045u128);
    let divider: Decimal256 = Decimal256::from_uint256(
        (state.reserve_anc * anc_swap_rate)
            + (state.reserve_luna * luna_swap_rate)
            + (state.reserve_mir * mir_swap_rate),
    );

    let cal_dividered: Decimal256 = if divider.is_zero() {
        Decimal256::from_uint256(100000u128)
    } else {
        divider
    };
    let uluna_amount: Uint256 =
        state.alloc_luna * deposit_amount * Decimal256::from_uint256(100000u128)
            / Decimal256::from_uint256(luna_swap_rate)
            / Decimal256::from_uint256(10000u128);
    let umir_amount: Uint256 =
        state.alloc_mir * deposit_amount * Decimal256::from_uint256(100000u128)
            / Decimal256::from_uint256(mir_swap_rate)
            / Decimal256::from_uint256(10000u128);
    let uanc_amount: Uint256 =
        state.alloc_anc * deposit_amount * Decimal256::from_uint256(100000u128)
            / Decimal256::from_uint256(anc_swap_rate)
            / Decimal256::from_uint256(10000u128);

    state.reserve_luna += Decimal256::from_uint256(luna_swap_rate * uluna_amount);
    state.reserve_anc += Decimal256::from_uint256(anc_swap_rate * uanc_amount);
    state.reserve_mir += Decimal256::from_uint256(mir_swap_rate * umir_amount);
    config(&mut deps.storage).save(&state)?;
    let supply = if state.total_supply.is_zero() {
        Decimal256::one()
    } else {
        state.total_supply
    };
    let mint_amount: Uint256 = supply * deposit_amount / cal_dividered;
    state.total_supply += Decimal256::from_uint256(mint_amount);
    let contract_addr = env.contract.address;

    Ok(HandleResponse {
        messages: vec![
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: deps.api.human_address(&state.eterra_contract)?,
                send: vec![],
                msg: to_binary(&Cw20HandleMsg::Mint {
                    recipient: env.message.sender.clone(),
                    amount: deposit_amount.into(),
                })?,
            }),
            create_swap_msg(
                contract_addr.clone(),
                Coin {
                    denom: "uusd".to_string(),
                    amount: uluna_amount.into(),
                },
                "uluna".to_string(),
            ),
            // create_swap_msg(
            //     contract_addr.clone(),
            //     Coin {
            //         denom: "umir".to_string(),
            //         amount: umir_amount.into(),
            //     },
            //     "uusd".to_string(),
            // ),
            // create_swap_msg(
            //     contract_addr.clone(),
            //     Coin {
            //         denom: "uanc".to_string(),
            //         amount: uanc_amount.into(),
            //     },
            //     "uusd".to_string(),
            // ),
        ],
        log: vec![
            log("reserve_luna", state.reserve_luna),
            log("luna_swap", luna_swap_rate),
            log("luna_amount", uluna_amount),
            log("mir_swap", mir_swap_rate),
            log("mir_amount", umir_amount),
            log("anc_amount", uanc_amount),
            log("anc_swap", anc_swap_rate),
            log("divider_cal", cal_dividered),
            log("mint_amount", mint_amount),
            // log("mint", mint_`amount),
        ],
        data: None,
    })
}
/// Swap all coins to stable_denom
/// and execute `swap_hook`
/// Executor: itself
pub fn swap_to_stable_denom<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
    env: Env,
    deposit_amount: Uint256,
    state: State,
    mint_amount: Uint256,
) -> HandleResult<TerraMsgWrapper> {
    if env.message.sender != env.contract.address {
        return Err(StdError::unauthorized());
    }

    let contract_addr = env.contract.address;
    let uluna_amount: Uint256 =
        state.alloc_luna / Decimal256::from_uint256(10000u128) * deposit_amount;
    let umir_amount: Uint256 =
        state.alloc_mir / Decimal256::from_uint256(10000u128) * deposit_amount;
    let uanc_amount: Uint256 =
        state.alloc_anc / Decimal256::from_uint256(10000u128) * deposit_amount;

    Ok(HandleResponse {
        messages: vec![
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: deps.api.human_address(&state.eterra_contract)?,
                send: vec![],
                msg: to_binary(&Cw20HandleMsg::Mint {
                    recipient: env.message.sender.clone(),
                    amount: mint_amount.into(),
                })?,
            }),
            create_swap_msg(
                contract_addr.clone(),
                Coin {
                    denom: "uluna".to_string(),
                    amount: uluna_amount.into(),
                },
                "uusd".to_string(),
            ),
            create_swap_msg(
                contract_addr.clone(),
                Coin {
                    denom: "umir".to_string(),
                    amount: umir_amount.into(),
                },
                "uusd".to_string(),
            ),
            create_swap_msg(
                contract_addr.clone(),
                Coin {
                    denom: "uanc".to_string(),
                    amount: uanc_amount.into(),
                },
                "uusd".to_string(),
            ),
        ],
        log: vec![],
        data: None,
    })
}

pub fn query<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
    msg: QueryMsg,
) -> StdResult<Binary> {
    match msg {
        QueryMsg::GetCount {} => to_binary(&query_state(deps)?),
    }
}

fn query_state<S: Storage, A: Api, Q: Querier>(deps: &Extern<S, A, Q>) -> StdResult<StateResponse> {
    let state = config_read(&deps.storage).load()?;
    Ok(StateResponse {
        owner: deps.api.human_address(&state.owner)?,
        total_supply: state.total_supply,
        reserve_luna: state.reserve_luna,
        reserve_anc: state.reserve_anc,
        reserve_mir: state.reserve_mir,
        eterra_contract: deps.api.human_address(&state.eterra_contract)?,
        alloc_luna: state.alloc_luna,
        alloc_anc: state.alloc_anc,
        alloc_mir: state.alloc_mir,
    })
}

// #[cfg(test)]
// mod tests {
//     use super::*;
//     use cosmwasm_std::testing::{mock_dependencies, mock_env};
//     use cosmwasm_std::{coins, from_binary, StdError};

//     #[test]
//     fn proper_initialization() {
//         let mut deps = mock_dependencies(20, &[]);

//         let msg = InitMsg { count: 17 };
//         let env = mock_env("creator", &coins(1000, "earth"));

//         // we can just call .unwrap() to assert this was a success
//         let res = init(&mut deps, env, msg).unwrap();
//         assert_eq!(0, res.messages.len());

//         // it worked, let's query the state
//         let res = query(&deps, QueryMsg::GetCount {}).unwrap();
//         let value: CountResponse = from_binary(&res).unwrap();
//         assert_eq!(17, value.count);
//     }

//     #[test]
//     fn increment() {
//         let mut deps = mock_dependencies(20, &coins(2, "token"));

//         let msg = InitMsg { count: 17 };
//         let env = mock_env("creator", &coins(2, "token"));
//         let _res = init(&mut deps, env, msg).unwrap();

//         // beneficiary can release it
//         let env = mock_env("anyone", &coins(2, "token"));
//         let msg = HandleMsg::Increment {};
//         let _res = handle(&mut deps, env, msg).unwrap();

//         // should increase counter by 1
//         let res = query(&deps, QueryMsg::GetCount {}).unwrap();
//         let value: CountResponse = from_binary(&res).unwrap();
//         assert_eq!(18, value.count);
//     }

//     #[test]
//     fn reset() {
//         let mut deps = mock_dependencies(20, &coins(2, "token"));

//         let msg = InitMsg { count: 17 };
//         let env = mock_env("creator", &coins(2, "token"));
//         let _res = init(&mut deps, env, msg).unwrap();

//         // beneficiary can release it
//         let unauth_env = mock_env("anyone", &coins(2, "token"));
//         let msg = HandleMsg::Reset { count: 5 };
//         let res = handle(&mut deps, unauth_env, msg);
//         match res {
//             Err(StdError::Unauthorized { .. }) => {}
//             _ => panic!("Must return unauthorized error"),
//         }

//         // only the original creator can reset the counter
//         let auth_env = mock_env("creator", &coins(2, "token"));
//         let msg = HandleMsg::Reset { count: 5 };
//         let _res = handle(&mut deps, auth_env, msg).unwrap();

//         // should now be 5
//         let res = query(&deps, QueryMsg::GetCount {}).unwrap();
//         let value: CountResponse = from_binary(&res).unwrap();
//         assert_eq!(5, value.count);
//     }
// }
