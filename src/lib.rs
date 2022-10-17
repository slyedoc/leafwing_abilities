#![deny(missing_docs)]
#![forbid(unsafe_code)]
#![warn(clippy::doc_markdown)]
#![doc = include_str!("../README.md")]

use crate::cooldown::CooldownState;
use bevy::ecs::prelude::*;
use charges::{ChargeState, Charges};
use cooldown::Cooldown;
use leafwing_input_manager::Actionlike;

mod ability_state;
pub mod charges;
pub mod cooldown;
pub mod plugin;
pub mod systems;
pub use ability_state::*;

// Importing the derive macro
pub use leafwing_abilities_macros::Abilitylike;

/// Everything you need to get started
pub mod prelude {
    pub use crate::charges::{ChargeState, Charges};
    pub use crate::cooldown::{Cooldown, CooldownState};

    pub use crate::plugin::AbilityPlugin;
    pub use crate::{AbilitiesBundle, AbilityState, Abilitylike};
}

/// Allows a type to be used as a gameplay action in an input-agnostic fashion
///
/// Actions are modelled as "virtual buttons", cleanly abstracting over messy, customizable inputs
/// in a way that can be easily consumed by your game logic.
///
/// This trait should be implemented on the `A` type that you want to pass into [`InputManagerPlugin`](crate::plugin::InputManagerPlugin).
///
/// Generally, these types will be very small (often data-less) enums.
/// As a result, the APIs in this crate accept actions by value, rather than reference.
/// While `Copy` is not a required trait bound,
/// users are strongly encouraged to derive `Copy` on these enums whenever possible to improve ergonomics.
///
/// # Example
/// ```rust
/// use leafwing_input_manager::Actionlike;
///
/// #[derive(Actionlike, PartialEq, Eq, Clone, Copy, Hash)]
/// enum PlayerAction {
///    // Movement
///    Up,
///    Down,
///    Left,
///    Right,
///    // Abilities
///    Ability1,
///    Ability2,
///    Ability3,
///    Ability4,
///    Ultimate,
/// }
/// ```
pub trait Abilitylike: Actionlike {
    /// Is this ability ready?
    ///
    /// If this ability has charges, at least one charge must be available.
    /// If this ability has a cooldown but no charges, the cooldown must be ready.
    /// Otherwise, returns `true`.
    ///
    /// Calls [`action_ready`], which can be used manually if you already know the [`Charges`] and [`Cooldown`] of interest.
    fn ready(&self, charges: &ChargeState<Self>, cooldowns: &CooldownState<Self>) -> bool {
        let charges = charges.get(self.clone());
        let cooldowns = cooldowns.get(self.clone());

        action_ready(charges, cooldowns)
    }

    /// Triggers this ability, depleting a charge if available.
    ///
    /// Returns `true` if the ability could be used, and `false` if it could not be.
    /// Abilities can only be used if they are ready.
    ///     
    /// Calls [`trigger_action`], which can be used manually if you already know the [`Charges`] and [`Cooldown`] of interest.
    fn trigger(
        &self,
        charges: &mut ChargeState<Self>,
        cooldowns: &mut CooldownState<Self>,
    ) -> bool {
        let charges = charges.get_mut(self.clone());
        let cooldowns = cooldowns.get_mut(self.clone());

        trigger_action(charges, cooldowns)
    }
}

/// Checks if a [`Charges`], [`Cooldown`] pair associated with an action is ready to use.
///
/// If this action has charges, at least one charge must be available.
/// If this action has a cooldown but no charges, the cooldown must be ready.
/// Otherwise, returns `true`.
#[inline]
#[must_use]
pub fn action_ready(charges: &Option<Charges>, cooldown: &Option<Cooldown>) -> bool {
    if let Some(charges) = charges {
        charges.charges() > 0
    } else if let Some(cooldown) = cooldown {
        cooldown.ready()
    } else {
        true
    }
}

/// Triggers an implicit action, depleting a charge if available.
///
/// If no `charges` is [`None`], this will be based off the [`Cooldown`] alone, triggering it if possible.
#[inline]
pub fn trigger_action(charges: &mut Option<Charges>, cooldown: &mut Option<Cooldown>) -> bool {
    if !action_ready(charges, cooldown) {
        return false;
    }

    if let Some(ref mut charges) = charges {
        charges.expend();
    } else if let Some(ref mut cooldown) = cooldown {
        cooldown.trigger();
    }

    true
}

/// This [`Bundle`] allows entities to manage their [`Abilitylike`] actions effectively.
///
/// Use with [`AbilityPlugin`](crate::plugin::AbilityPlugin), providing the same enum type to both.
#[derive(Bundle, Clone, Debug, PartialEq)]
pub struct AbilitiesBundle<A: Abilitylike> {
    /// A [`Cooldowns`] component
    pub cooldowns: CooldownState<A>,
    /// A [`ActionCharges`] component
    pub charges: ChargeState<A>,
}

// Cannot use derive(Default), as it forces an undesirable bound on our generics
impl<A: Abilitylike> Default for AbilitiesBundle<A> {
    fn default() -> Self {
        Self {
            cooldowns: CooldownState::default(),
            charges: ChargeState::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::charges::Charges;
    use crate::cooldown::Cooldown;
    use crate::{action_ready, trigger_action};

    #[test]
    fn action_ready_no_cooldown_no_charges() {
        assert!(action_ready(&None, &None));
    }

    #[test]
    fn action_ready_just_cooldown() {
        let mut cooldown = Some(Cooldown::from_secs(1.));
        assert!(action_ready(&None, &cooldown));

        cooldown.as_mut().map(|c| c.trigger());
        assert!(!action_ready(&None, &cooldown));
    }

    #[test]
    fn action_ready_just_charges() {
        let mut charges = Some(Charges::simple(1));

        assert!(action_ready(&charges, &None));

        charges.as_mut().map(|c| c.expend());
        assert!(!action_ready(&charges, &None));
    }

    #[test]
    fn action_ready_cooldown_and_charges() {
        let mut charges = Some(Charges::simple(1));
        let mut cooldown = Some(Cooldown::from_secs(1.));
        // Both available
        assert!(action_ready(&charges, &cooldown));

        // Charge on cooldown, cooldown ready
        charges.as_mut().map(|c| c.expend());
        assert!(!action_ready(&charges, &cooldown));

        // Just charges
        charges.as_mut().map(|c| c.replenish());
        cooldown.as_mut().map(|c| c.trigger());
        assert!(action_ready(&charges, &cooldown));

        // Neither
        charges.as_mut().map(|c| c.expend());
        assert!(!action_ready(&charges, &cooldown));
    }

    #[test]
    fn trigger_action_no_cooldown_no_charges() {
        let outcome = trigger_action(&mut None, &mut None);
        assert!(outcome);
    }

    #[test]
    fn trigger_action_just_cooldown() {
        let mut cooldown = Some(Cooldown::from_secs(1.));
        assert!(trigger_action(&mut None, &mut cooldown));

        cooldown.as_mut().map(|c| c.trigger());
        assert!(!trigger_action(&mut None, &mut cooldown));
        assert!(!action_ready(&None, &cooldown));
    }

    #[test]
    fn trigger_action_just_charges() {
        let mut charges = Some(Charges::simple(1));

        assert!(trigger_action(&mut charges, &mut None));

        charges.as_mut().map(|c| c.expend());
        assert!(!trigger_action(&mut charges, &mut None));
        assert!(!action_ready(&charges, &None));
    }

    #[test]
    fn trigger_action_cooldown_and_charges() {
        let mut charges = Some(Charges::simple(1));
        let mut cooldown = Some(Cooldown::from_secs(1.));
        // Both available
        assert!(trigger_action(&mut charges, &mut cooldown));
        assert!(!action_ready(&charges, &cooldown));

        // None available
        assert!(!trigger_action(&mut charges, &mut cooldown));

        // Just charges
        charges.as_mut().map(|c| c.replenish());
        assert!(trigger_action(&mut charges, &mut cooldown));

        // Just cooldown
        charges.as_mut().map(|c| c.expend());
        cooldown.as_mut().map(|c| c.refresh());
        assert!(!trigger_action(&mut charges, &mut cooldown));
    }
}