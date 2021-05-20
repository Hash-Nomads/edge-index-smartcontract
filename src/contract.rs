use std::{
    ops::{Mul, Sub},
    str::FromStr,
};

use cosmwasm_bignumber::{Decimal256, Uint256};
use cosmwasm_std::{
    coins, log, to_binary, Api, BankMsg, Binary, CanonicalAddr, Coin, CosmosMsg, Decimal, Env,
    Extern, HandleResponse, HandleResult, HumanAddr, InitResponse, Querier, StdError, StdResult,
    Storage, Uint128, WasmMsg,
};
use terra_cosmwasm::{create_swap_msg, TerraMsg, TerraMsgWrapper};

use crate::{
    math::decimal_division,
    msg::{ConfigResponse, HandleMsg, InitMsg, QueryMsg, StateResponse},
    state::{read_config, read_state, store_config, store_state, Config, State},
};
use cw20::{Cw20CoinHuman, Cw20HandleMsg, MinterResponse};
use terraswap::{
    asset::{Asset, AssetInfo, PairInfo},
    hook::InitHook,
    pair::{Cw20HookMsg as TerraswapCw20HookMsg, HandleMsg as TerraswapHandleMsg},
    querier::{query_balance, query_token_balance},
};
use terraswap::{querier::query_pair_info, token::InitMsg as TokenInitMsg};

pub fn init<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    msg: InitMsg,
) -> StdResult<InitResponse> {
    let config = Config {
        owner_addr: deps.api.canonical_address(&env.message.sender)?,
        terraswap_factory: deps.api.canonical_address(&msg.terraswap_factory)?,
        alloc_luna: msg.alloc_luna,
        alloc_anc: msg.alloc_anc,
        alloc_mir: msg.alloc_mir,
        stable_denom: msg.stable_denom.clone(),
        eterra_contract: CanonicalAddr::default(),
    };

    let state = State {
        total_supply: Uint128::zero(),
        reserve_luna: Decimal256::zero(),
        reserve_anc: Decimal256::zero(),
        reserve_mir: Decimal256::zero(),
    };

    store_config(&mut deps.storage).save(&config)?;
    store_state(&mut deps.storage).save(&state)?;

    Ok(InitResponse {
        messages: vec![CosmosMsg::Wasm(WasmMsg::Instantiate {
            code_id: msg.eterra_code_id,
            send: vec![],
            label: None,
            msg: to_binary(&TokenInitMsg {
                name: format!("Edge Terra {}", msg.stable_denom[1..].to_uppercase()),
                symbol: "eTerra".to_string(),
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
        HandleMsg::RegisterETerra {} => register_eterra(deps, env),
        HandleMsg::Burn {} => burn(deps, env),
        HandleMsg::RedeemToken { sender } => redeem_token(deps, env, sender),
    }
}
pub fn register_eterra<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
) -> HandleResult<TerraMsgWrapper> {
    let mut state = read_config(&deps.storage).load()?;
    if state.eterra_contract != CanonicalAddr::default() {
        return Err(StdError::unauthorized());
    }

    state.eterra_contract = deps.api.canonical_address(&env.message.sender)?;
    store_config(&mut deps.storage).save(&state)?;
    Ok(HandleResponse {
        messages: vec![],
        log: vec![log("eterra", env.message.sender)],
        data: None,
    })
}
pub fn mint<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
) -> StdResult<HandleResponse<TerraMsgWrapper>> {
    let config = read_config(&deps.storage).load()?;
    let mut state: State = read_state(&deps.storage).load()?;

    // check base denom deposit
    let deposit_amount: Uint256 = env
        .message
        .sent_funds
        .iter()
        .find(|c| c.denom == config.stable_denom)
        .map(|c| Uint256::from(c.amount))
        .unwrap_or_else(Uint256::zero);
    // Cannot deposit zero amount
    if deposit_amount.is_zero() {
        return Err(StdError::generic_err(format!(
            "Deposit amount must be greater than 0 {}",
            config.stable_denom,
        )));
    }
    let mut mint_amount = Uint128(0);
    let terraswap_factory_raw = deps.api.human_address(&config.terraswap_factory)?;
    let mut messages = vec![];
    // swap stable denom => anc
    let pair_info: PairInfo = query_pair_info(
        &deps,
        &terraswap_factory_raw,
        &[
            AssetInfo::NativeToken {
                denom: config.stable_denom.clone().to_string(),
            },
            AssetInfo::Token {
                contract_addr: HumanAddr::from("terra1747mad58h0w4y589y3sk84r5efqdev9q4r02pc"),
            },
        ],
    )?;
    let swap_asset = Asset {
        info: AssetInfo::NativeToken {
            denom: config.stable_denom.clone(),
        },
        amount: deposit_amount.into(),
    };
    let amount = (swap_asset.deduct_tax(&deps)?).amount;
    let amount = decimal_division(
        amount * config.alloc_anc.into(),
        Decimal::from_str("10000")?,
    );
    mint_amount += amount;
    messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: pair_info.contract_addr,
        msg: to_binary(&TerraswapHandleMsg::Swap {
            offer_asset: Asset {
                amount,
                info: AssetInfo::NativeToken {
                    denom: config.stable_denom.clone(),
                },
            },
            max_spread: None,
            belief_price: None,
            to: None,
        })?,
        send: vec![Coin {
            denom: config.stable_denom.clone(),
            amount,
        }],
    }));

    // swap stable denom => mirror
    let pair_info: PairInfo = query_pair_info(
        &deps,
        &terraswap_factory_raw,
        &[
            AssetInfo::NativeToken {
                denom: config.stable_denom.clone().to_string(),
            },
            AssetInfo::Token {
                contract_addr: HumanAddr::from("terra10llyp6v3j3her8u3ce66ragytu45kcmd9asj3u"),
            },
        ],
    )?;
    let amount = (swap_asset.deduct_tax(&deps)?).amount;
    let amount = decimal_division(
        amount * config.alloc_mir.into(),
        Decimal::from_str("10000")?,
    );
    mint_amount += amount;
    messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: pair_info.contract_addr,
        msg: to_binary(&TerraswapHandleMsg::Swap {
            offer_asset: Asset {
                amount,
                info: AssetInfo::NativeToken {
                    denom: config.stable_denom.clone(),
                },
            },
            max_spread: None,
            belief_price: None,
            to: None,
        })?,
        send: vec![Coin {
            denom: config.stable_denom.clone(),
            amount,
        }],
    }));

    // swap stable denom => luna denom
    let amount = (swap_asset.deduct_tax(&deps)?).amount;
    let amount = decimal_division(
        amount * config.alloc_luna.into(),
        Decimal::from_str("10000")?,
    );
    mint_amount += amount;
    messages.push(create_swap_msg(
        env.contract.address,
        Coin {
            denom: config.stable_denom.clone(),
            amount,
        },
        "uluna".to_string(),
    ));

    // mint eTerra
    messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: deps.api.human_address(&config.eterra_contract)?,
        send: vec![],
        msg: to_binary(&Cw20HandleMsg::Mint {
            recipient: env.message.sender.clone(),
            amount: mint_amount.into(),
        })?,
    }));

    state.total_supply += mint_amount;
    store_state(&mut deps.storage).save(&state)?;

    Ok(HandleResponse {
        messages,
        log: vec![log("mint", mint_amount.to_string())],
        data: None,
    })
}

pub fn burn<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
) -> StdResult<HandleResponse<TerraMsgWrapper>> {
    let config = read_config(&deps.storage).load()?;
    let mut state = read_state(&deps.storage).load()?;
    let available_amount = query_token_balance(
        &deps,
        &deps.api.human_address(&config.eterra_contract)?,
        &env.contract.address,
    )?;
    let terraswap_factory_raw = deps.api.human_address(&config.terraswap_factory)?;
    let mut messages: Vec<CosmosMsg<TerraMsgWrapper>> = vec![];

    // anc
    let pair_info: PairInfo = query_pair_info(
        &deps,
        &terraswap_factory_raw,
        &[
            AssetInfo::NativeToken {
                denom: config.stable_denom.to_string(),
            },
            AssetInfo::Token {
                contract_addr: HumanAddr::from("terra1747mad58h0w4y589y3sk84r5efqdev9q4r02pc"),
            },
        ],
    )?;
    let balance_anc = query_token_balance(
        &deps,
        &HumanAddr::from("terra1747mad58h0w4y589y3sk84r5efqdev9q4r02pc"),
        &env.contract.address,
    )?;

    let amount = decimal_division(
        available_amount.multiply_ratio(balance_anc, state.total_supply) * config.alloc_anc.into(),
        Decimal::from_str("10000")?,
    );
    messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: deps.api.human_address(&config.eterra_contract)?,
        msg: to_binary(&Cw20HandleMsg::Send {
            contract: pair_info.contract_addr,
            amount,
            msg: Some(to_binary(&TerraswapCw20HookMsg::Swap {
                max_spread: None,
                belief_price: None,
                to: None,
            })?),
        })?,
        send: vec![],
    }));
    // mirror
    let pair_info: PairInfo = query_pair_info(
        &deps,
        &terraswap_factory_raw,
        &[
            AssetInfo::NativeToken {
                denom: config.stable_denom.to_string(),
            },
            AssetInfo::Token {
                contract_addr: HumanAddr::from("terra10llyp6v3j3her8u3ce66ragytu45kcmd9asj3u"),
            },
        ],
    )?;
    let amount = decimal_division(
        available_amount.multiply_ratio(balance_anc, state.total_supply) * config.alloc_mir.into(),
        Decimal::from_str("10000")?,
    );
    messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: deps.api.human_address(&config.eterra_contract)?,
        msg: to_binary(&Cw20HandleMsg::Send {
            contract: pair_info.contract_addr,
            amount,
            msg: Some(to_binary(&TerraswapCw20HookMsg::Swap {
                max_spread: None,
                belief_price: None,
                to: None,
            })?),
        })?,
        send: vec![],
    }));

    let amount = decimal_division(
        available_amount.multiply_ratio(balance_anc, state.total_supply) * config.alloc_luna.into(),
        Decimal::from_str("10000")?,
    );
    messages.push(create_swap_msg(
        env.contract.address.clone(),
        Coin {
            denom: "uluna".to_string(),
            amount,
        },
        config.stable_denom.clone(),
    ));
    messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: deps.api.human_address(&config.eterra_contract)?,
        send: vec![],
        msg: to_binary(&Cw20HandleMsg::Burn {
            amount: available_amount.into(),
        })?,
    }));

    messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: env.contract.address,
        send: vec![],
        msg: to_binary(&HandleMsg::RedeemToken {
            sender: env.message.sender,
        })?,
    }));

    state.total_supply = state.total_supply.sub(available_amount)?;
    store_state(&mut deps.storage).save(&state)?;

    Ok(HandleResponse {
        messages,
        log: vec![log("burn", available_amount.to_string())],
        data: None,
    })
}

pub fn redeem_token<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    sender: HumanAddr,
) -> StdResult<HandleResponse<TerraMsgWrapper>> {
    // this is just meant as a call-back to ourself
    if env.message.sender != env.contract.address {
        return Err(StdError::unauthorized());
    }
    let config = read_config(&deps.storage).load()?;
    let balance = deps
        .querier
        .query_balance(env.contract.address.clone(), "uusd")
        .unwrap()
        .amount;
    Ok(HandleResponse {
        messages: vec![CosmosMsg::Bank(BankMsg::Send {
            from_address: env.contract.address,
            to_address: sender,
            amount: coins(balance.into(), &config.stable_denom),
        })],
        log: vec![log("redeem", balance)],
        data: None,
    })
}

pub fn query<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
    msg: QueryMsg,
) -> StdResult<Binary> {
    match msg {
        QueryMsg::Config {} => to_binary(&query_config(deps)?),
        QueryMsg::State {} => to_binary(&query_state(deps)?),
    }
}

fn query_config<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
) -> StdResult<ConfigResponse> {
    let config = read_config(&deps.storage).load()?;
    Ok(ConfigResponse {
        terraswap_factory: deps.api.human_address(&config.terraswap_factory)?,
        owner_addr: deps.api.human_address(&config.owner_addr)?,
        eterra_contract: deps.api.human_address(&config.eterra_contract)?,
        stable_denom: config.stable_denom,
        alloc_luna: config.alloc_luna,
        alloc_anc: config.alloc_anc,
        alloc_mir: config.alloc_mir,
    })
}

fn query_state<S: Storage, A: Api, Q: Querier>(deps: &Extern<S, A, Q>) -> StdResult<StateResponse> {
    let state = read_state(&deps.storage).load()?;
    Ok(StateResponse {
        total_supply: state.total_supply,
        reserve_luna: state.reserve_luna,
        reserve_anc: state.reserve_anc,
        reserve_mir: state.reserve_mir,
    })
}

#[cfg(test)]
mod tests {
    use crate::mock_querier::mock_dependencies;

    use super::*;
    use cosmwasm_std::{
        testing::{mock_env, MockApi, MOCK_CONTRACT_ADDR},
        HumanAddr,
    };
    use terra_cosmwasm::TerraRoute;

    #[test]
    fn proper_initialization() {
        let mut deps = mock_dependencies(20, &[]);
        let msg = InitMsg {
            alloc_luna: Decimal256::from_uint256(5000u128),
            alloc_mir: Decimal256::from_uint256(2500u128),
            alloc_anc: Decimal256::from_uint256(2500u128),
            stable_denom: "uusd".to_string(),
            eterra_code_id: 123u64,
            terraswap_factory: HumanAddr("terraswapfactory".to_string()),
        };
        let env = mock_env("creator", &[]);

        // we can just call .unwrap() to assert this was a success
        let res = init(&mut deps, env, msg).unwrap();
        assert_eq!(
            res.messages,
            vec![CosmosMsg::Wasm(WasmMsg::Instantiate {
                code_id: 123u64,
                send: vec![],
                label: None,
                msg: to_binary(&TokenInitMsg {
                    name: "Edge Terra USD".to_string(),
                    symbol: "eTerra".to_string(),
                    decimals: 6u8,
                    initial_balances: vec![Cw20CoinHuman {
                        address: HumanAddr::from(MOCK_CONTRACT_ADDR),
                        amount: Uint128(0),
                    }],
                    mint: Some(MinterResponse {
                        minter: HumanAddr::from(MOCK_CONTRACT_ADDR),
                        cap: None,
                    }),
                    init_hook: Some(InitHook {
                        contract_addr: HumanAddr::from(MOCK_CONTRACT_ADDR),
                        msg: to_binary(&HandleMsg::RegisterETerra {}).unwrap(),
                    })
                })
                .unwrap(),
            })]
        );

        // Register edge token contract
        let msg = HandleMsg::RegisterETerra {};
        let env = mock_env("ETerra", &[]);
        let _res = handle(&mut deps, env, msg).unwrap();

        // Cannot register again
        let msg = HandleMsg::RegisterETerra {};
        let env = mock_env("ETerra", &[]);
        let _res = handle(&mut deps, env, msg).unwrap_err();

        let config = query_config(&deps).unwrap();
        assert_eq!("terraswapfactory", config.terraswap_factory.as_str());
        assert_eq!("uusd", config.stable_denom.as_str());
    }

    #[test]
    fn mint() {
        let mut deps = mock_dependencies(20, &[]);
        let msg = InitMsg {
            alloc_luna: Decimal256::from_uint256(5000u128),
            alloc_mir: Decimal256::from_uint256(2500u128),
            alloc_anc: Decimal256::from_uint256(2500u128),
            stable_denom: "uusd".to_string(),
            eterra_code_id: 123u64,
            terraswap_factory: HumanAddr("terraswapfactory".to_string()),
        };
        let env = mock_env("creator", &[]);
        deps.querier.with_terraswap_pairs(&[
            (&"uusdANC".to_string(), &HumanAddr::from("pairANC")),
            (&"uusdMIRROR".to_string(), &HumanAddr::from("pairMIRROR")),
        ]);

        // we can just call .unwrap() to assert this was a success
        let res = init(&mut deps, env, msg).unwrap();
        assert_eq!(
            res.messages,
            vec![CosmosMsg::Wasm(WasmMsg::Instantiate {
                code_id: 123u64,
                send: vec![],
                label: None,
                msg: to_binary(&TokenInitMsg {
                    name: "Edge Terra USD".to_string(),
                    symbol: "eTerra".to_string(),
                    decimals: 6u8,
                    initial_balances: vec![Cw20CoinHuman {
                        address: HumanAddr::from(MOCK_CONTRACT_ADDR),
                        amount: Uint128(0),
                    }],
                    mint: Some(MinterResponse {
                        minter: HumanAddr::from(MOCK_CONTRACT_ADDR),
                        cap: None,
                    }),
                    init_hook: Some(InitHook {
                        contract_addr: HumanAddr::from(MOCK_CONTRACT_ADDR),
                        msg: to_binary(&HandleMsg::RegisterETerra {}).unwrap(),
                    })
                })
                .unwrap(),
            })]
        );

        // Register edge token contract
        let msg = HandleMsg::RegisterETerra {};
        let env = mock_env("ETerra", &[]);
        let _res = handle(&mut deps, env, msg).unwrap();

        // Cannot register again
        let msg = HandleMsg::RegisterETerra {};
        let env = mock_env("ETerra", &[]);
        let _res = handle(&mut deps, env, msg).unwrap_err();

        let msg = HandleMsg::Mint {};
        let env = mock_env(
            "alice",
            &[Coin {
                denom: "uusd".to_string(),
                amount: Uint128(100000u128),
            }],
        );
        let res = handle(&mut deps, env, msg).unwrap();
        let _balance = deps
            .querier
            .query_balance(MOCK_CONTRACT_ADDR, "uusd")
            .unwrap()
            .amount;

        assert_eq!(Uint128(0u128), _balance);
        assert_eq!(res.messages.len(), 4);
        assert_eq!(res.log, vec![log("mint", "100000"),]);
        assert_eq!(
            res.messages[0],
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: HumanAddr::from("pairANC"),
                msg: to_binary(&TerraswapHandleMsg::Swap {
                    offer_asset: Asset {
                        info: AssetInfo::NativeToken {
                            denom: "uusd".to_string()
                        },
                        amount: Uint128(25000u128), // 1000 * 25%
                    },
                    max_spread: None,
                    belief_price: None,
                    to: None,
                })
                .unwrap(),
                send: vec![Coin {
                    amount: Uint128(25000u128),
                    denom: "uusd".to_string(),
                }],
            })
        );
        assert_eq!(
            res.messages[1],
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: HumanAddr::from("pairMIRROR"),
                msg: to_binary(&TerraswapHandleMsg::Swap {
                    offer_asset: Asset {
                        info: AssetInfo::NativeToken {
                            denom: "uusd".to_string()
                        },
                        amount: Uint128(25000u128), // 1000 * 25$
                    },
                    max_spread: None,
                    belief_price: None,
                    to: None,
                })
                .unwrap(),
                send: vec![Coin {
                    amount: Uint128(25000u128),
                    denom: "uusd".to_string(),
                }],
            })
        );
        assert_eq!(
            res.messages[2],
            CosmosMsg::Custom(TerraMsgWrapper {
                route: TerraRoute::Market,
                msg_data: TerraMsg::Swap {
                    trader: HumanAddr::from("cosmos2contract"),
                    offer_coin: Coin {
                        amount: Uint128(50000u128),
                        denom: "uusd".to_string()
                    },
                    ask_denom: "uluna".to_string()
                }
            })
        );
        assert_eq!(
            res.messages[3],
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: HumanAddr::from("ETerra"),
                send: vec![],
                msg: to_binary(&Cw20HandleMsg::Mint {
                    recipient: HumanAddr::from("alice"),
                    amount: Uint128(100000u128),
                })
                .unwrap(),
            })
        );
    }
    #[test]
    fn burn() {
        let mut deps = mock_dependencies(
            20,
            &[Coin {
                denom: "uluna".to_string(),
                amount: Uint128(1000000u128),
            }],
        );
        let msg = InitMsg {
            alloc_luna: Decimal256::from_uint256(5000u128),
            alloc_mir: Decimal256::from_uint256(2500u128),
            alloc_anc: Decimal256::from_uint256(2500u128),
            stable_denom: "uusd".to_string(),
            eterra_code_id: 123u64,
            terraswap_factory: HumanAddr("terraswapfactory".to_string()),
        };
        let env = mock_env("creator", &[]);
        deps.querier.with_terraswap_pairs(&[
            (&"uusdANC".to_string(), &HumanAddr::from("pairANC")),
            (&"uusdMIRROR".to_string(), &HumanAddr::from("pairMIRROR")),
        ]);
        deps.querier.with_token_balances(&[
            (
                &HumanAddr::from("ETerra"),
                &[(&HumanAddr::from(MOCK_CONTRACT_ADDR), &Uint128(5000u128))], // User balance sended
            ),
            (
                &HumanAddr::from("terra1747mad58h0w4y589y3sk84r5efqdev9q4r02pc"),
                &[(&HumanAddr::from(MOCK_CONTRACT_ADDR), &Uint128(1000000u128))],
            ),
            (
                &HumanAddr::from("uusdMIRROR"),
                &[(&HumanAddr::from(MOCK_CONTRACT_ADDR), &Uint128(1000000u128))],
            ),
        ]);

        // we can just call .unwrap() to assert this was a success
        let res = init(&mut deps, env, msg).unwrap();
        assert_eq!(
            res.messages,
            vec![CosmosMsg::Wasm(WasmMsg::Instantiate {
                code_id: 123u64,
                send: vec![],
                label: None,
                msg: to_binary(&TokenInitMsg {
                    name: "Edge Terra USD".to_string(),
                    symbol: "eTerra".to_string(),
                    decimals: 6u8,
                    initial_balances: vec![Cw20CoinHuman {
                        address: HumanAddr::from(MOCK_CONTRACT_ADDR),
                        amount: Uint128(0),
                    }],
                    mint: Some(MinterResponse {
                        minter: HumanAddr::from(MOCK_CONTRACT_ADDR),
                        cap: None,
                    }),
                    init_hook: Some(InitHook {
                        contract_addr: HumanAddr::from(MOCK_CONTRACT_ADDR),
                        msg: to_binary(&HandleMsg::RegisterETerra {}).unwrap(),
                    })
                })
                .unwrap(),
            })]
        );

        // Register edge token contract
        let msg = HandleMsg::RegisterETerra {};
        let env = mock_env("ETerra", &[]);
        let _res = handle(&mut deps, env, msg).unwrap();

        // Cannot register again
        let msg = HandleMsg::RegisterETerra {};
        let env = mock_env("ETerra", &[]);
        let _res = handle(&mut deps, env, msg).unwrap_err();

        let mut state: State = read_state(&deps.storage).load().unwrap();
        state.total_supply = Uint128::from(20000u128);
        store_state(&mut deps.storage).save(&state).unwrap();

        let msg = HandleMsg::Burn {};
        let env = mock_env("alice", &[]);
        let res = handle(&mut deps, env, msg).unwrap();

        // assert_eq!(res.log, vec![]);
        assert_eq!(
            res.messages[0],
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: HumanAddr::from("ETerra"),
                msg: to_binary(&Cw20HandleMsg::Send {
                    contract: HumanAddr::from("pairANC"),
                    amount: Uint128(62500u128),
                    msg: Some(
                        to_binary(&TerraswapCw20HookMsg::Swap {
                            max_spread: None,
                            belief_price: None,
                            to: None,
                        })
                        .unwrap()
                    ),
                })
                .unwrap(),
                send: vec![],
            })
        );
        assert_eq!(
            res.messages[1],
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: HumanAddr::from("ETerra"),
                msg: to_binary(&Cw20HandleMsg::Send {
                    contract: HumanAddr::from("pairMIRROR"),
                    amount: Uint128(62500u128),
                    msg: Some(
                        to_binary(&TerraswapCw20HookMsg::Swap {
                            max_spread: None,
                            belief_price: None,
                            to: None,
                        })
                        .unwrap()
                    ),
                })
                .unwrap(),
                send: vec![],
            })
        );
        assert_eq!(
            res.messages[2],
            CosmosMsg::Custom(TerraMsgWrapper {
                route: TerraRoute::Market,
                msg_data: TerraMsg::Swap {
                    trader: HumanAddr::from("cosmos2contract"),
                    offer_coin: Coin {
                        amount: Uint128(125000u128),
                        denom: "uluna".to_string()
                    },
                    ask_denom: "uusd".to_string()
                }
            })
        );
        assert_eq!(
            res.messages[3],
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: HumanAddr::from("ETerra"),
                send: vec![],
                msg: to_binary(&Cw20HandleMsg::Burn {
                    amount: Uint128(5000u128),
                })
                .unwrap(),
            })
        );
        assert_eq!(
            res.messages[4],
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: HumanAddr::from(MOCK_CONTRACT_ADDR),
                send: vec![],
                msg: to_binary(&HandleMsg::RedeemToken {
                    sender: HumanAddr::from("alice")
                })
                .unwrap()
            })
        )
    }
    #[test]
    fn redeem_token() {
        let mut deps = mock_dependencies(
            20,
            &[
                Coin {
                    denom: "uluna".to_string(),
                    amount: Uint128(1000000u128),
                },
                Coin {
                    denom: "uusd".to_string(),
                    amount: Uint128(10000u128),
                },
            ],
        );
        let msg = InitMsg {
            alloc_luna: Decimal256::from_uint256(5000u128),
            alloc_mir: Decimal256::from_uint256(2500u128),
            alloc_anc: Decimal256::from_uint256(2500u128),
            stable_denom: "uusd".to_string(),
            eterra_code_id: 123u64,
            terraswap_factory: HumanAddr("terraswapfactory".to_string()),
        };
        let env = mock_env("creator", &[]);

        // we can just call .unwrap() to assert this was a success
        let _res = init(&mut deps, env, msg).unwrap();
        // Register edge token contract
        let msg = HandleMsg::RegisterETerra {};
        let env = mock_env("ETerra", &[]);
        let _res = handle(&mut deps, env, msg).unwrap();

        let msg = HandleMsg::RedeemToken {
            sender: HumanAddr::from("alice"),
        };
        let env = mock_env("alice", &[]);
        // invalid sender
        let _res = handle(&mut deps, env, msg.clone()).unwrap_err();
        let env = mock_env(HumanAddr::from(MOCK_CONTRACT_ADDR), &[]);
        let res = handle(&mut deps, env, msg).unwrap();
        assert_eq!(
            res.messages[0],
            CosmosMsg::Bank(BankMsg::Send {
                from_address: HumanAddr::from(MOCK_CONTRACT_ADDR),
                to_address: HumanAddr::from("alice"),
                amount: coins(10000u128, &"uusd".to_string()),
            })
        )
    }
}
