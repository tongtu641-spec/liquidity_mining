#![no_std]

//! # Liquidity Mining
//!
//! A Soroban smart contract that rewards users for providing liquidity to
//! a Stellar DEX pool. LPs deposit a token pair, receive an on-chain share
//! of the pool, and accrue mining rewards proportional to their share.
//! Rewards accumulate block-by-block based on a per-pool reward rate and
//! can be claimed by the LP at any time.

use soroban_sdk::{contract, contractimpl, contracttype, Address, Env, Symbol};

/// Fixed-point precision factor used for the per-share reward accumulator.
/// Rewards are tracked in `u128` with 12 decimal places of precision so
/// that fractional rewards can be accumulated across many blocks without
/// losing accuracy for small pool sizes.
const PRECISION: u128 = 1_000_000_000_000;

/// Strongly-typed storage keys. Using an enum keeps the storage layout
/// self-documenting and avoids stringly-typed bugs.
#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    /// Total LP share supply for a given pool.
    PoolTotal(Symbol),
    /// Total rewards that have been paid out of a pool (informational).
    PoolPaid(Symbol),
    /// Accumulated reward per LP share, scaled by `PRECISION`.
    AccReward(Symbol),
    /// Last ledger sequence at which the pool was updated.
    LastBlock(Symbol),
    /// Reward rate (tokens per block) emitted to LPs in this pool.
    RewardRate(Symbol),
    /// Individual LP's share in a specific pool.
    LpShare(Symbol, Address),
    /// Snapshot of the LP's reward debt, used to avoid double-paying.
    LpDebt(Symbol, Address),
}

#[contract]
pub struct LiquidityMining;

#[contractimpl]
impl LiquidityMining {
    /// Create a new liquidity-mining pool.
    ///
    /// `admin` must authorize the call. `reward_rate` is the number of
    /// reward tokens distributed to LPs per ledger sequence while the
    /// pool is active. Each `(pool_id, admin)` pair can only create one
    /// pool — calling this twice with the same `pool_id` panics.
    pub fn create_pool(env: Env, admin: Address, pool_id: Symbol, reward_rate: u64) {
        admin.require_auth();

        let total_key = DataKey::PoolTotal(pool_id.clone());
        if env.storage().instance().has(&total_key) {
            panic!("Pool already exists");
        }

        env.storage().instance().set(&total_key, &0u64);
        env.storage().instance().set(&DataKey::PoolPaid(pool_id.clone()), &0u64);
        env.storage()
            .instance()
            .set(&DataKey::AccReward(pool_id.clone()), &0u128);
        env.storage()
            .instance()
            .set(&DataKey::RewardRate(pool_id.clone()), &reward_rate);
        env.storage()
            .instance()
            .set(&DataKey::LastBlock(pool_id.clone()), &env.ledger().sequence());
    }

    /// Deposit liquidity into a pool.
    ///
    /// The LP authorizes the call and provides two token amounts. The
    /// resulting on-chain share is `min(amount_a, amount_b)` (the
    /// "balanced" portion of the deposit). Any pending rewards for the
    /// caller are settled before the share is updated so they never get
    /// diluted. Returns the LP's new total share in the pool.
    pub fn add_liquidity(
        env: Env,
        lp: Address,
        pool_id: Symbol,
        amount_a: u64,
        amount_b: u64,
    ) -> u64 {
        lp.require_auth();
        Self::update_pool(&env, &pool_id);

        let share = if amount_a < amount_b { amount_a } else { amount_b };
        if share == 0 {
            panic!("Must deposit positive amounts");
        }

        // Settle any pre-existing rewards before mutating share.
        let _ = Self::settle_rewards(&env, &lp, &pool_id);

        let share_key = DataKey::LpShare(pool_id.clone(), lp.clone());
        let current: u64 = env.storage().instance().get(&share_key).unwrap_or(0u64);
        let new_share = current + share;
        env.storage().instance().set(&share_key, &new_share);

        let total_key = DataKey::PoolTotal(pool_id.clone());
        let total: u64 = env.storage().instance().get(&total_key).unwrap_or(0u64);
        env.storage().instance().set(&total_key, &(total + share));

        // Note: in production, cross-contract calls to the SAC token
        // contracts would transfer `amount_a` and `amount_b` from the LP
        // into the pool's token accounts here.

        new_share
    }

    /// Burn `share` of the caller's LP tokens and withdraw the
    /// corresponding slice of pool liquidity.
    ///
    /// Any rewards accrued up to this point are settled and returned to
    /// the LP. Returns the LP's remaining share in the pool.
    pub fn remove_liquidity(env: Env, lp: Address, pool_id: Symbol, share: u64) -> u64 {
        lp.require_auth();
        if share == 0 {
            panic!("Share must be positive");
        }
        Self::update_pool(&env, &pool_id);

        let share_key = DataKey::LpShare(pool_id.clone(), lp.clone());
        let current: u64 = env.storage().instance().get(&share_key).unwrap_or(0u64);
        if share > current {
            panic!("Insufficient share");
        }

        // Settle rewards first so we never pay out on withdrawn share.
        let _ = Self::settle_rewards(&env, &lp, &pool_id);

        let new_share = current - share;
        env.storage().instance().set(&share_key, &new_share);

        let total_key = DataKey::PoolTotal(pool_id.clone());
        let total: u64 = env.storage().instance().get(&total_key).unwrap_or(0u64);
        env.storage().instance().set(&total_key, &(total - share));

        // Note: in production, the proportional token amounts would be
        // transferred back to the LP from the pool's token accounts.

        new_share
    }

    /// Claim all accrued mining rewards for `lp` in `pool_id`.
    ///
    /// The pool's accumulator is refreshed against the current ledger
    /// sequence and any pending reward is paid out. Returns the amount
    /// of reward that was paid.
    pub fn claim_rewards(env: Env, lp: Address, pool_id: Symbol) -> u64 {
        lp.require_auth();
        Self::update_pool(&env, &pool_id);
        Self::settle_rewards(&env, &lp, &pool_id)
    }

    /// View the LP's current share in the given pool. Returns 0 if the
    /// LP has never deposited into the pool.
    pub fn lp_share(env: Env, lp: Address, pool_id: Symbol) -> u64 {
        env.storage()
            .instance()
            .get(&DataKey::LpShare(pool_id, lp))
            .unwrap_or(0u64)
    }

    /// View the total size of a pool (the sum of every LP's share).
    /// Returns 0 if the pool has not been created.
    pub fn pool_size(env: Env, pool_id: Symbol) -> u64 {
        env.storage()
            .instance()
            .get(&DataKey::PoolTotal(pool_id))
            .unwrap_or(0u64)
    }

    /// View the amount of reward the LP can currently claim. The pool
    /// accumulator is refreshed as a side-effect so the returned value
    /// reflects the very latest ledger sequence.
    pub fn pending_reward(env: Env, lp: Address, pool_id: Symbol) -> u64 {
        Self::update_pool(&env, &pool_id);
        let share: u64 = env
            .storage()
            .instance()
            .get(&DataKey::LpShare(pool_id.clone(), lp.clone()))
            .unwrap_or(0u64);
        let acc: u128 = env
            .storage()
            .instance()
            .get(&DataKey::AccReward(pool_id.clone()))
            .unwrap_or(0u128);
        let debt: u128 = env
            .storage()
            .instance()
            .get(&DataKey::LpDebt(pool_id, lp))
            .unwrap_or(0u128);
        let accumulated = (share as u128) * acc / PRECISION;
        if accumulated > debt {
            (accumulated - debt) as u64
        } else {
            0
        }
    }

    // --------------------- internal helpers ---------------------

    /// Refresh the per-share accumulator using the number of blocks
    /// elapsed since the last update and the pool's reward rate.
    fn update_pool(env: &Env, pool_id: &Symbol) {
        let now = env.ledger().sequence();
        let last_key = DataKey::LastBlock(pool_id.clone());
        let last: u32 = env.storage().instance().get(&last_key).unwrap_or(now);

        if now <= last {
            return;
        }

        let total: u64 = env
            .storage()
            .instance()
            .get(&DataKey::PoolTotal(pool_id.clone()))
            .unwrap_or(0u64);
        let rate: u64 = env
            .storage()
            .instance()
            .get(&DataKey::RewardRate(pool_id.clone()))
            .unwrap_or(0u64);

        let acc_key = DataKey::AccReward(pool_id.clone());
        let mut acc: u128 = env.storage().instance().get(&acc_key).unwrap_or(0u128);

        if total > 0 && rate > 0 {
            let elapsed = (now - last) as u128;
            let reward = elapsed * (rate as u128);
            acc += (reward * PRECISION) / (total as u128);
            env.storage().instance().set(&acc_key, &acc);
        }
        env.storage().instance().set(&last_key, &now);
    }

    /// Pay out the LP's accrued reward and re-snapshot their debt to
    /// the current accumulator so future claims do not double-pay.
    fn settle_rewards(env: &Env, lp: &Address, pool_id: &Symbol) -> u64 {
        let share: u64 = env
            .storage()
            .instance()
            .get(&DataKey::LpShare(pool_id.clone(), lp.clone()))
            .unwrap_or(0u64);
        let acc: u128 = env
            .storage()
            .instance()
            .get(&DataKey::AccReward(pool_id.clone()))
            .unwrap_or(0u128);
        let debt_key = DataKey::LpDebt(pool_id.clone(), lp.clone());
        let debt: u128 = env.storage().instance().get(&debt_key).unwrap_or(0u128);

        let accumulated = (share as u128) * acc / PRECISION;
        let pending = if accumulated > debt {
            accumulated - debt
        } else {
            0
        };

        // Reset the LP's debt to the current snapshot for the share
        // they still hold.
        env.storage().instance().set(&debt_key, &accumulated);

        if pending > 0 {
            let pending_u64 = pending as u64;
            let paid_key = DataKey::PoolPaid(pool_id.clone());
            let paid: u64 = env.storage().instance().get(&paid_key).unwrap_or(0u64);
            env.storage().instance().set(&paid_key, &(paid + pending_u64));
            // Note: in production a reward-token transfer from the
            // pool's reward account to the LP would happen here.
            return pending_u64;
        }
        0
    }
}
