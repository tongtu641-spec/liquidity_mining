# liquidity_mining

## Project Title

liquidity_mining

## Project Description

Liquidity providers (LPs) are the backbone of any decentralized exchange, but on most chains the only way to incentivize them is through inflationary token emissions paid out by a separate, off-chain bot. **liquidity_mining** is a Soroban smart contract that brings that incentive loop on-chain: LPs deposit a balanced pair of tokens into a pool, the contract mints them an on-chain share, and a fixed reward rate accrues block-by-block to every LP in proportion to their share. Rewards are settled in a `claim_rewards` call that requires the LP's own authorization, so no custodian is involved and no off-chain cron is required.

The MVP focuses on the four core actions — create pool, add liquidity, remove liquidity, claim rewards — plus two read-only views for share and pool size. Real token transfers are stubbed out with comments because the goal of this build is the reward accounting, not the asset plumbing.

## Project Vision

The long-term vision is a fully on-chain, fully trustless DEX-mining primitive that any Stellar-based AMM can drop in front of their pool. Once the reward accounting is proven, the same contract can sit next to Soroban's native DEX order books, route fee revenue back into the same accumulator, and turn every pool on Stellar into a yield-bearing position without any centralized operator.

## Key Features

- **Per-pool reward rate** — each pool defines its own emission rate (tokens per ledger sequence), so projects can spin up incentive programs of any size.
- **Proportional share accounting** — LP shares are computed as `min(amount_a, amount_b)`, the classic AMM "balanced liquidity" formula, keeping the pool exposure honest.
- **Block-by-block reward accrual** — a `u128` accumulator with `1e12` precision tracks rewards per share, so the contract always knows exactly what each LP is owed without iterating over LPs.
- **Self-custodial claims** — `claim_rewards` requires the LP's own `require_auth()` and pays out only the LP's slice, so no one else can drain the pool.
- **Read-only views** — `lp_share`, `pool_size`, and `pending_reward` let wallets and front-ends render accurate balances and APY estimates without sending a transaction.
- **Storage-key enum** — uses a `#[contracttype]` enum for storage keys, keeping the layout typed and collision-free.

## Contract

- **Network:** Stellar Testnet (Public)
- **Scope:** finance dApp — see `contracts/liquidity_mining/src/lib.rs` for the full liquidity_mining business logic.
- **Functions exposed:** see `Key Features` above and the `pub fn` list in `lib.rs`.
- **Contract ID:** `CACJ5BWQSXC4BZLNSAGBHYDXDOHD5UCOSXNHC6H2LY5KWAO5ZPWIHLCV`
- **Explorer template:** `https://stellar.expert/explorer/testnet/tx/fe427a5b68b2728eef7095a87986dbe516a20680cf2abf2329d17d749f2eb8b1`


## Future Scope

- **Real SAC token integration** — wire `add_liquidity` / `remove_liquidity` / `claim_rewards` to Stellar Asset Contract `transfer` calls so deposits, withdrawals, and reward payouts move real on-chain assets.
- **Multi-pool router** — add a `pools()` view that enumerates every pool created by this contract, plus a helper for clients that want to discover them.
- **Time-weighted reward boosts** — extend the accumulator to give LPs a multiplier for long lock-ups, mirroring the boost curves used by Curve and Convex.
- **Governance-controlled reward rate** — wrap `reward_rate` behind an admin that is itself a Soroban governance contract, so emissions can be tuned by token-holder vote.
- **Frontend dashboard** — a small HTML/JS UI that connects via Freighter, lists pools, and lets the user deposit, withdraw, and claim in one click.
- **Mainnet hardening** — full unit-test coverage, a fuzz harness for the accumulator math, and a `contracterror` enum to replace every `panic!` with a typed revert reason before mainnet deployment.

## Profile

- **Name:** <!-- Fill github name -->
- **Project:** `liquidity_mining` (finance)
- **Built with:** Soroban SDK 25, Rust, Stellar Testnet
