cargo wasm
docker run --rm -v "$(pwd)":/code --mount type=volume,source="$(basename "$(pwd)")_cache",target=/code/target --mount type=volume,source=registry_cache,target=/usr/local/cargo/registry cosmwasm/rust-optimizer:0.11.3
terracli tx wasm store artifacts/my_first_contract.wasm  --from dev2 --chain-id=tequila-0004 --fees=400000uluna --gas=auto --broadcast-mode=block