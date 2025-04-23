# Deploy the test environment

## Deploy Uniswap without running the prover

### Deploy L1 and L2

From https://github.com/logos-co/zksync-era.

1. Run the L1 and the DB with `zkstack containers --observability false --verbose`
2. Initialize the ecosystem and the chains with `zkstack ecosystem init --deploy-erc20 --observability false --deploy-ecosystem --l1-rpc-url http://127.0.0.1:8545 --deploy-paymaster --server-db-url postgres://postgres:notsecurepassword@localhost:5432 --server-db-name server_db --verbose`
3. Run the L2 sequencer with `zkstack server --verbose`
4. Fund account with the portal with `zkstack portal --verbose`

### Deploy Uniswap v2

From https://medium.com/coinmonks/deploy-interact-with-uniswap-v2-pair-locally-with-hardhat-and-ethers-js-v6-f8f5dd436930.

1. Deploy the ERC20 tokens for the trading, plus the WETH contract:
    * MEME token address: `0xe441CF0795aF14DdB9f7984Da85CD36DB1B8790d`
    * NOMOS token address: `0xc8f8ce6491227a6a2ab92e67a64011a4eba1c6cf`
    * WETH token address: `0xf10A110E59a22b444c669C83b02f0E6d945b2b69`
2. Deploy the factory contract:
    * Address: `0xf1Ebfaa992854ECcB01Ac1F60e5b5279095cca7F`
3. Deploy the router contract:
    * Address: `0xd4CCc6A962F4261338aA84747Ed5FF1F7945686c`
4. Approve router as spender for both ETC20 tokens
5. (maybe?) Fund the Weth contract with 1 Ether
6. Add liquidity to the pair

Deployment script:

```ts
// Deploy tokens
const Token = await ethers.getContractFactory("Token");
const mehmetToken = await Token.deploy("Mehmet", "MEME", ethers.utils.parseEther("1000000"));
const nomosToken = await Token.deploy("Nomos", "NMO", ethers.utils.parseEther("1000000"));
await mehmetToken.deployed();
await nomosToken.deployed();

// Deploy WETH
const WETH9 = await ethers.getContractFactory("WETH9");
const weth = await WETH9.deploy();
await weth.deployed();

// Deploy Factory
const Factory = await ethers.getContractFactory("UniswapV2Factory");
const factory = await Factory.deploy(deployer.address);
await factory.deployed();

// Deploy Router
const Router = await ethers.getContractFactory("UniswapV2Router02");
const router = await Router.deploy(factory.address, weth.address);
await router.deployed();

// Approve router to spend tokens
await mehmetToken.approve(router.address, ethers.constants.MaxUint256);
await nomosToken.approve(router.address, ethers.constants.MaxUint256);

// Add liquidity
await router.addLiquidity(
    mehmetToken.address,
    nomosToken.address,
    ethers.utils.parseEther("10000"),
    ethers.utils.parseEther("10000"),
    0,
    0,
    deployer.address,
    Math.floor(Date.now() / 1000) + 60 * 10
);
```


## Run the Alternative uniswap app

Add `export NODE_OPTIONS=--openssl-legacy-provider`.