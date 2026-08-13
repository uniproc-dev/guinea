/// Declares which messages a handler may send. Optional: an actor that
/// declares no graph gets [`Open`], which allows everything.
#[diagnostic::on_unimplemented(
    message = "handler `{M}` does not declare sending `{Out}`",
    label = "not declared",
    note = "add the edge in `actor! {{ handlers {{ {M} => {{ send {Out} }} }} }}`"
)]
pub trait Allows<M, Out> {}

/// The default: no graph declared, nothing restricted.
pub struct Open;

impl<M, Out> Allows<M, Out> for Open {}
