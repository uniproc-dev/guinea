pub trait L10nSink<Strings> {
    fn load(&self, strings: Strings);
}

#[macro_export]
macro_rules! l10n_push_fields {
    ($target:expr, $strings:expr; $(($field:ident, $setter:ident)),* $(,)?) => {{
        $( $target.$setter($strings.$field.into()); )*
    }};
}
