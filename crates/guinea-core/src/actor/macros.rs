/// Declares message types - the actions an actor handles, and whatever it
/// sends itself.
///
/// It no longer declares a group. A group existed to prove that an action the
/// view emits belongs to a list somebody wired, and that proof is now
/// `DrivenBy: Handler<M>` - which the compiler checks at the `emit` itself,
/// with nothing to keep in sync and nothing to leave unwired.
#[macro_export]
macro_rules! messages {
    ( $($name:ident $( { $($f_name:ident : $f_typ:ty),* $(,)? } )? $( ( $($t_typ:ty),* $(,)? ) )? ),* $(,)? ) => {
        $(
            $crate::messages!(@declare $name $( { $($f_name : $f_typ),* } )? $( ( $($t_typ),* ) )? );
        )*
    };

    (@declare $name:ident { $($f_name:ident : $f_typ:ty),* $(,)? } ) => {
        #[derive(Debug, Clone)]
        pub struct $name {
            $( pub $f_name : $f_typ ),*
        }
        impl $crate::actor::traits::Message for $name {}
    };

    (@declare $name:ident ( $first:ty $(, $rest:ty)* $(,)? ) ) => {
        #[derive(Debug, Clone)]
        pub struct $name(pub $first, $(pub $rest),*);
        impl $crate::actor::traits::Message for $name {}
    };

    (@declare $name:ident $($_:tt)? ) => {
        #[derive(Debug, Clone)]
        pub struct $name;
        impl $crate::actor::traits::Message for $name {}
    };
}
