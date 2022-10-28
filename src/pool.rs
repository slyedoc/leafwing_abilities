//! Pools are a reservoir of resources that can be used to pay for abilities, or keep track of character state.
//!
//! Unlike charges, pools are typically shared across abilities.
//!
//! Life, mana, energy and rage might all be modelled effectively as pools.
//! Pools have a maximum value, have a minimum value of zero, can regenerate over time, and can be spent to pay for abilities.

use bevy::ecs::prelude::*;
use bevy::utils::Duration;
use core::ops::{Add, AddAssign, Div, Mul, Sub, SubAssign};
use std::marker::PhantomData;
use thiserror::Error;

use crate::{Abilitylike, CannotUseAbility};

/// A reservoir of a resource that can be used to pay for abilities, or keep track of character state.
///
/// Each type that implements this trait should be stored on a component (or, if your actions are globally unique, a resource),
/// and contains information about the current, max and regeneration rates
pub trait Pool:
    Add<Self::Quantity>
    + AddAssign<Self::Quantity>
    + Sub<Self::Quantity>
    + SubAssign<Self::Quantity>
    + PartialEq
    + Sized
{
    /// A type that tracks the quantity within a pool.
    ///
    /// Unlike a [`Pool`] type, which stores a max, min
    type Quantity: Add<Output = Self::Quantity>
        + Sub<Output = Self::Quantity>
        + AddAssign
        + SubAssign
        + Mul<f32, Output = Self::Quantity>
        + Div<f32, Output = Self::Quantity>
        + PartialEq
        + PartialOrd
        + Copy
        + Send
        + Sync
        + Clone
        + 'static;

    /// The minimum value of the pool type.
    ///
    /// At this point, no resources remain to be spent.
    const ZERO: Self::Quantity;

    /// Creates a new pool with the specified settings.
    ///
    /// # Panics
    ///
    /// Panics if `max` is less than [`Pool::ZERO`].
    fn new(current: Self::Quantity, max: Self::Quantity, regen_per_second: Self::Quantity) -> Self;

    /// Creates a new pool, with zero initial resources.
    ///
    /// # Panics
    ///
    /// Panics if `max` is less than [`Pool::ZERO`].
    fn new_empty(max: Self::Quantity, regen_per_second: Self::Quantity) -> Self {
        Pool::new(Self::ZERO, max, regen_per_second)
    }

    /// Creates a new pool with current value set at the specified maximum.
    ///
    /// # Panics
    ///
    /// Panics if `max` is less than [`Pool::ZERO`].
    fn new_full(max: Self::Quantity, regen_per_second: Self::Quantity) -> Self {
        Pool::new(max, max, regen_per_second)
    }

    /// The current quantity of resources in the pool.
    ///
    /// # Panics
    ///
    /// Panics if `max` is less than [`Pool::ZERO`].
    fn current(&self) -> Self::Quantity;

    /// Sets the current quantity of resources in the pool.
    ///
    /// This will be bounded by the minimum and maximum values of this pool.
    /// The value that was actually set is returned.
    fn set_current(&mut self, new_quantity: Self::Quantity) -> Self::Quantity;

    /// The maximum quantity of resources that this pool can store.
    fn max(&self) -> Self::Quantity;

    /// Sets the maximum quantity of resources that this pool can store.
    ///
    /// The current value will be reduced to the new max if necessary.
    ///
    /// Has no effect if `new_max < Pool::ZERO`.
    /// Returns a [`PoolMaxLessThanZero`] error if this occurs.
    fn set_max(&mut self, new_max: Self::Quantity) -> Result<(), PoolLessThanZero>;

    /// Spend the specified amount from the pool, if there is that much available.
    ///
    /// Otherwise, return the error [`CannotUseAbility::PoolEmpty`].
    fn expend(&mut self, amount: Self::Quantity) -> Result<(), CannotUseAbility> {
        if self.current() >= amount {
            let new_current = self.current() - amount;
            self.set_current(new_current);
            Ok(())
        } else {
            Err(CannotUseAbility::PoolEmpty)
        }
    }

    /// Replenish the pool by the specified amount.
    ///
    /// This cannot cause the pool to exceed maximum value that can be stored in the pool.
    fn replenish(&mut self, amount: Self::Quantity) {
        let new_current = self.current() + amount;
        self.set_current(new_current);
    }

    /// The quantity recovered by the pool in one second.
    ///
    /// This value may be negative, in the case of automatically decaying pools (like rage).
    fn regen_per_second(&self) -> Self::Quantity;

    /// Set the quantity recovered by the pool in one second.
    ///
    /// This value may be negative, in the case of automatically decaying pools (like rage).
    fn set_regen_per_second(&mut self, new_regen_per_second: Self::Quantity);

    /// Regenerates this pool according to the elapsed `delta_time`.
    fn regenerate(&mut self, delta_time: Duration) {
        let pool_regained = self.regen_per_second() * delta_time.as_secs_f32();
        self.replenish(pool_regained)
    }
}

/// The maximum value for a [`Pool`] was set to be less than [`Pool::ZERO`].
#[derive(Debug, Clone, Copy, Error)]
#[error("The maximum quantity that can be stored in a pool must be greater than zero.")]
pub struct PoolLessThanZero;

/// Stores the cost (in terms of the [`Pool::Quantity`] of ability) associated with each ability of type `A`.
#[derive(Component, Debug)]
pub struct AbilityCosts<A: Abilitylike, P: Pool> {
    /// The underlying cost of each ability, stored in [`Actionlike::variants`] order.
    cost_vec: Vec<Option<P::Quantity>>,
    _phantom: PhantomData<A>,
}

impl<A: Abilitylike, P: Pool> Clone for AbilityCosts<A, P> {
    fn clone(&self) -> Self {
        AbilityCosts {
            cost_vec: A::variants().map(|ability| *self.get(ability)).collect(),
            _phantom: PhantomData::default(),
        }
    }
}

impl<A: Abilitylike, P: Pool> Default for AbilityCosts<A, P> {
    fn default() -> Self {
        AbilityCosts {
            cost_vec: A::variants().map(|_| None).collect(),
            _phantom: PhantomData::default(),
        }
    }
}

impl<A: Abilitylike, P: Pool> AbilityCosts<A, P> {
    /// Creates a new [`AbilityCosts`] from an iterator of `(charges, action)` pairs
    ///
    /// If a [`Pool::Quantity`] is not provided for an action, that action will have no cost in terms of the stored resource pool.
    ///
    /// To create an empty [`AbilityCosts`] struct, use the [`Default::default`] method instead.
    #[must_use]
    pub fn new(action_cost_pairs: impl IntoIterator<Item = (A, P::Quantity)>) -> Self {
        let mut ability_costs = AbilityCosts::default();
        for (action, cost) in action_cost_pairs.into_iter() {
            ability_costs.set(action, cost);
        }
        ability_costs
    }

    /// Are enough resources available in the `pool` to use the `action`?
    ///
    /// Returns `true` if the underlying resource is [`None`].
    #[inline]
    #[must_use]
    pub fn available(&self, action: A, pool: &P) -> bool {
        if let Some(cost) = self.get(action) {
            pool.current() > *cost
        } else {
            true
        }
    }

    /// Pay the ability cost for the `action` from the `pool`, if able
    ///
    /// The cost of the action is expended from the [`Pool`].
    ///
    /// If the underlying pool does not have enough resources to pay the action's cost,
    /// a [`CannotUseAbility::PoolEmpty`] error is returned and this call has no effect.
    ///
    /// Returns [`Ok(())`] if the underlying [`Pool`] can support the cost of the action.
    #[inline]
    pub fn pay_cost(&mut self, action: A, pool: &mut P) -> Result<(), CannotUseAbility> {
        if let Some(cost) = self.get(action) {
            pool.expend(*cost)
        } else {
            Ok(())
        }
    }

    /// Returns a reference to the underlying [`Pool::Quantity`] cost for `action`, if set.
    #[inline]
    #[must_use]
    pub fn get(&self, action: A) -> &Option<P::Quantity> {
        &self.cost_vec[action.index()]
    }

    /// Returns a mutable reference to the underlying [`Pool::Quantity`] cost for `action`, if set.
    #[inline]
    #[must_use]
    pub fn get_mut(&mut self, action: A) -> &mut Option<P::Quantity> {
        &mut self.cost_vec[action.index()]
    }

    /// Sets the underlying [`Pool::Quantity`] cost for `action` to the provided value.
    ///
    /// Unless you're building a new [`AbilityCosts`] struct, you likely want to use [`Self::get_mut`].
    #[inline]
    pub fn set(&mut self, action: A, cost: P::Quantity) -> &mut Self {
        let data = self.get_mut(action);
        *data = Some(cost);

        self
    }

    /// Collects a `&mut Self` into a `Self`.
    ///
    /// Used to conclude the builder pattern. Actually just calls `self.clone()`.
    #[inline]
    #[must_use]
    pub fn build(&mut self) -> Self {
        self.clone()
    }

    /// Returns an iterator of references to the underlying non-[`None`] [`Charges`]
    #[inline]
    pub fn iter(&self) -> impl Iterator<Item = &P::Quantity> {
        self.cost_vec.iter().flatten()
    }

    /// Returns an iterator of mutable references to the underlying non-[`None`] [`Charges`]
    #[inline]
    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut P::Quantity> {
        self.cost_vec.iter_mut().flatten()
    }
}

/// Stores a resource pool and the associated costs for each ability.
///
/// Note that if your abilities do not cost the given resource,
/// you can still add your [`Pool`] type as a component.
///
/// This is particularly common when working with life totals,
/// as you want the other functionality of pools (current, max, regen, depletion)
/// but often cannot spend it on abilities.
#[derive(Bundle)]
pub struct PoolBundle<A: Abilitylike, P: Pool + Component> {
    /// The resource pool used to pay for abilities
    pub pool: P,
    /// The cost of each ability in terms of this pool
    pub ability_costs: AbilityCosts<A, P>,
}
