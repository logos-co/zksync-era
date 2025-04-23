# Deploy the test environment

## Deploy Uniswap without running the prover

### Deploy L1 and L2

From https://github.com/logos-co/zksync-era.

1. Run the L1 and the DB with `zkstack containers --observability false --verbose`
2. Initialize the ecosystem and the chains with `zkstack ecosystem init --deploy-erc20 --observability false --deploy-ecosystem --l1-rpc-url http://127.0.0.1:8545 --deploy-paymaster --server-db-url postgres://postgres:notsecurepassword@localhost:5432 --server-db-name server_db --verbose`
3. Run the L2 sequencer with `zkstack server --verbose`
4. Fund account with 10000000 ETH from the portal with `zkstack portal --verbose` using a Chrome instance that does not enforce CORS `open /Applications/Google\ Chrome.app --args --user-data-dir="/var/tmp/chrome-dev-disabled-security" --disable-web-security --disable-site-isolation-trials`

### Deploy Uniswap v2

From https://medium.com/coinmonks/deploy-interact-with-uniswap-v2-pair-locally-with-hardhat-and-ethers-js-v6-f8f5dd436930.

Run the script in the sz-poc repo.

1. Deploy the ERC20 tokens for the trading, plus the WETH contract, with `10000000000000000000000000` supply:
    * L2: MEME token address: `0xe441CF0795aF14DdB9f7984Da85CD36DB1B8790d`
    * L1: `0xDABa4e2Eef03b13a153a88B7E53846b55190a778`
    * L2 NOMOS token address: `0xc8F8cE6491227a6a2Ab92e67a64011a4Eba1C6CF`
    * L1: `0x0517c47E5438ce0Feec1dF6ce7cE06537c96f5F6`
    * WETH token address: `0xf10A110E59a22b444c669C83b02f0E6d945b2b69`
    * L1: `0xef885492431c734851357D6dEc63ba1821760c54`
2. Deploy the factory contract:
    * Address: `0xf1Ebfaa992854ECcB01Ac1F60e5b5279095cca7F`
    * L1: `0x8f02B21dd1D7CD27A40F300faCc3B3265bFD214B`
3. Deploy the router contract:
    * Address: `0x038d81BF8797f92648a11CfeD322c2785a8Fffa1`
    * L1: `0xeB7F9217bb284a1Fc3C4B3EaCA72178F53FC6019`

300000 gas -> Change fee estimate to 2x

Weth9 contract:

```
pragma solidity >=0.4.22 <0.6;

contract WETH9 {
    string public name     = "Wrapped Ether";
    string public symbol   = "WETH";
    uint8  public decimals = 18;

    event  Approval(address indexed src, address indexed guy, uint wad);
    event  Transfer(address indexed src, address indexed dst, uint wad);
    event  Deposit(address indexed dst, uint wad);
    event  Withdrawal(address indexed src, uint wad);

    mapping (address => uint)                       public  balanceOf;
    mapping (address => mapping (address => uint))  public  allowance;

    function() external payable {
        deposit();
    }
    function deposit() public payable {
        balanceOf[msg.sender] += msg.value;
        emit Deposit(msg.sender, msg.value);
    }
    function withdraw(uint wad) public {
        require(balanceOf[msg.sender] >= wad);
        balanceOf[msg.sender] -= wad;
        msg.sender.transfer(wad);
        emit Withdrawal(msg.sender, wad);
    }

    function totalSupply() public view returns (uint) {
        return address(this).balance;
    }

    function approve(address guy, uint wad) public returns (bool) {
        allowance[msg.sender][guy] = wad;
        emit Approval(msg.sender, guy, wad);
        return true;
    }

    function transfer(address dst, uint wad) public returns (bool) {
        return transferFrom(msg.sender, dst, wad);
    }

    function transferFrom(address src, address dst, uint wad)
        public
        returns (bool)
    {
        require(balanceOf[src] >= wad);

        if (src != msg.sender && allowance[src][msg.sender] != uint(-1)) {
            require(allowance[src][msg.sender] >= wad);
            allowance[src][msg.sender] -= wad;
        }

        balanceOf[src] -= wad;
        balanceOf[dst] += wad;

        emit Transfer(src, dst, wad);

        return true;
    }
}
```

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

## Perform the swap

Always estimate 2x gas. E.g. 500000.
