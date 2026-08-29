//! What a segment may read, decided at build time.
//!
//! Both halves of the answer are already written down: `routes!` knows who
//! sits above whom, and the author declares what each segment `Installs` and
//! what each feature `Exports`. Joining them turns "no scope owns this" from a
//! panic on the first render into an error at the read itself.
//!
//! It lives here rather than in `guinea-core` because [`Feature`] does, and a
//! blanket impl has to be in the crate that owns the trait.

use std::marker::PhantomData;

use guinea_core::feature::Bound;
use guinea_core::scope::Reducer;

use super::traits::Feature;

/// Where a segment sits in its route tree, written by `routes!`.
///
/// A page belongs to one tree: its ancestry is part of the type, so the same
/// page in two trees would need two answers and gets a conflicting-impl error
/// instead of a wrong one.
pub trait Segment: 'static {
    /// `<Self as Page>::Installs` - the macro names it because it knows the
    /// backend and the trait; the author declares it on the impl.
    type Installs;
    /// The segments above, innermost first, as a cons list: `(Tabs, (Shell, ()))`.
    type Above;
}

/// Which of several impls applied - a disambiguator, not information.
///
/// Coherence is the whole reason it exists: "the head matches" and "something
/// in the tail matches" are two impls that rustc cannot see are exclusive.
/// Carrying the position in the type keeps them apart.
///
/// It leaks one character into the call site - `cx.state::<R, _>()` - because
/// Rust has no partial turbofish.
pub struct Here;
pub struct There<I>(PhantomData<I>);

/// Membership in a feature's `Exports`.
pub trait Lists<R, I> {}

impl<A> Lists<A, Here> for (A,) {}

impl<A, B> Lists<A, Here> for (A, B) {}
impl<A, B> Lists<B, There<Here>> for (A, B) {}

impl<A, B, C> Lists<A, Here> for (A, B, C) {}
impl<A, B, C> Lists<B, There<Here>> for (A, B, C) {}
impl<A, B, C> Lists<C, There<There<Here>>> for (A, B, C) {}

/// What a segment's `Installs` publishes: one feature, a reducer it claimed
/// directly, or a tuple of either.
///
/// The index's first step says which of those shapes matched, which keeps the
/// impls apart even though nothing stops a tuple implementing [`Feature`].
pub trait Provides<R, I> {}

impl<F: Feature, R, I> Provides<R, (Here, I)> for F where F::Exports: Lists<R, I> {}

impl<A, B, R, I> Provides<R, (There<Here>, I)> for (A, B) where A: Provides<R, I> {}
impl<A, B, R, I> Provides<R, (There<There<Here>>, I)> for (A, B) where B: Provides<R, I> {}

/// A reducer the segment claimed itself, without a feature between.
///
/// `cx.state::<R>()` already hands back a [`Bound<R>`], so declaring it costs
/// a segment nothing it was not already holding - and a claim that goes
/// undeclared is exactly a claim nothing below can see, which is what the
/// declaration is for.
impl<R: Reducer> Provides<R, (There<There<There<Here>>>, Here)> for Bound<R> {}

/// Proof that a segment may read `R`: it installed the feature that exports
/// it, or a segment above it did.
#[diagnostic::on_unimplemented(
    message = "`{Self}` cannot read `{R}` from here",
    label = "no feature in reach exports it",
    note = "a segment reads what it installed itself, and what a segment above it listed in `Exports`"
)]
pub trait Reaches<R, I> {}

impl<S: Segment, R, I> Reaches<R, (Here, I)> for S where S::Installs: Provides<R, I> {}
impl<S: Segment, R, I> Reaches<R, (There<Here>, I)> for S where S::Above: Reaches<R, I> {}

// The ancestors are a cons list rather than a segment, so they walk their own
// way.
impl<H: Segment, T, R, I> Reaches<R, (Here, I)> for (H, T) where H::Installs: Provides<R, I> {}
impl<H, T, R, I> Reaches<R, (There<Here>, I)> for (H, T) where T: Reaches<R, I> {}
