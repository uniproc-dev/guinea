/// Declares message types. `messages! { pub Group { .. } }` additionally
/// declares a feature's UI contract; the flat form declares actor-internal
/// messages.
#[macro_export]
macro_rules! messages {
    ( pub $group:ident {
        $( $name:ident
            $( { $($f_name:ident : $f_typ:ty),* $(,)? } )?
            $( ( $($t_typ:ty),* $(,)? ) )?
        ),* $(,)?
    } ) => {
        $(
            $crate::messages!(@declare $name $( { $($f_name : $f_typ),* } )? $( ( $($t_typ),* ) )? );
        )*

        #[derive(Debug, Clone, Copy)]
        pub struct $group;

        impl $crate::actor::group::ActionsGroup for $group {
            type Dispatch = $crate::actor::group::GroupDispatch<$group>;
            type Members = $crate::messages!(@cons $($name),*);
        }

        $(
            impl $crate::actor::group::InGroup<$group> for $name {}
        )*
    };

    ( $($name:ident $( { $($f_name:ident : $f_typ:ty),* $(,)? } )? $( ( $($t_typ:ty),* $(,)? ) )? ),* $(,)? ) => {
        $(
            $crate::messages!(@declare $name $( { $($f_name : $f_typ),* } )? $( ( $($t_typ),* ) )? );
        )*
    };

    (@cons) => { () };
    (@cons $head:ident $(, $tail:ident)* ) => {
        ($head, $crate::messages!(@cons $($tail),*))
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
