# Offsite demo deployment checklist

## Deploy a local testnet

1. Build the nomos binaries with `cargo build -p nomos-node -p nomos-executor --release`
2. Run the testnet with `RISC0_DEV_MODE=1 cargo test   local_testnet  --all --lib --bins --tests --examples --all-features -- --nocapture`
3. Take note of the executor address

## Create the ecosystem and deploy the chain

1. Delete existing ecosystem by deleting root `Zkstack.yaml` and `chains` folder.
2. If needed, install the local hacked `zkstack` binary with `cargo install --path ./zkstack_cli/crates/zkstack --locked zkstack --force`
3. Create a new ecosystem with `zkstack ecosystem create --ecosystem-name  offsite-split-25 --l1-network localhost --link-to-code /home/antonio/Developer/zksync-era-fork --chain-name sz-poc --chain-id 55555 --prover-mode gpu --wallet-creation localhost --l1-batch-commit-data-generator-mode validium --evm-emulator true --start-containers true --update-submodules true --verbose` and select `Eth` as base token
4. Cd into the ecosystem folder with `cd offsite_split_25`
5. Initialize the ecosystem with `zkstack ecosystem init --deploy-erc20 false --deploy-ecosystem true --l1-rpc-url http://127.0.0.1:8545 --deploy-paymaster true --server-db-url postgres://postgres:notsecurepassword@localhost:5432 --server-db-name sz-poc-server --observability false --update-submodules true --verbose` with Nomos DA URL set to `https://testnet.nomos.tech/node/3/`, App ID to `01ea21912cdcbdd9189d49d07b61543ffdf7064355640eb6cc6fc6d902056d1b`, username and password as needed
6. Start the server with `zkstack server --components=api,tree,eth,state_keeper,housekeeper,commitment_generator,da_dispatcher,proof_data_handler,vm_runner_protective_reads,vm_runner_bwip --verbose`
7. Start the prover with `zkstack prover init --bellman-cuda true --bellman-cuda-dir /home/antonio/Developer/era-bellman-cuda --setup-compressor-key false --setup-keys false --setup-database true --prover-db-url postgres://postgres:notsecurepassword@localhost:5432 --prover-db-name sz-poc-prover --dont-drop false --use-default false --verbose`
8. Start the gateway with `zkstack prover run --component=gateway --docker false --verbose`
9.  Start the witness generator with `zkstack prover run --component=witness-generator --round=all-rounds --docker false --verbose`
10. Start the circuit prover with `zkstack prover run --component=circuit-prover -l 15 -h 1 --docker false --verbose`
11. After the circuit prover proves a batch (see from the logs), start the compressor with `zkstack prover run --component=compressor --docker false --verbose`
12. Check the server logs for DA posting

## Fund the account

1. Deploy the portal with `zkstack portal --verbose`
2. Fund account with 10000000 ETH from the portal using a Chrome instance that does not enforce CORS `open -na /Applications/Google\ Chrome.app --args --user-data-dir="/var/tmp/chrome-dev-disabled-security" --disable-web-security`

## Deploy Uniswap

1. Run the script in the sz-poc repo using `60000000` as gas limit.
2. Copy the router address

## Run the Alternative uniswap app

1. Modify the uniswap interface to point to the new router
2. Run with `NODE_OPTIONS=--openssl-legacy-provider npm start`

## Perform the swap

[MAYBE] Always estimate 2x gas. E.g. 500000.
