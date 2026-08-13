pub use guinea_core::l10n::{L10n, Localization};

pub fn use_l10n<S>(cx: &mut windows_reactor::RenderCx) -> S
where
    S: Clone + PartialEq + Default + 'static,
{
    let (current, set_current) = cx.use_state(L10n::<S>::current());

    cx.use_effect_with_cleanup((), move || {
        let set_current = set_current.clone();
        let subscription = L10n::<S>::subscribe(move |latest| set_current.call(latest));
        Some(move || drop(subscription))
    });

    current
}

#[macro_export]
macro_rules! fluent_loader {
    (locales: $locales:literal, fallback_language: $fallback:literal $(,)?) => {
        ::fluent_templates::static_loader! {
            static LOCALES = {
                locales: $locales,
                fallback_language: $fallback,
            };
        }

        #[derive(Default)]
        struct Args(::std::collections::HashMap<::std::borrow::Cow<'static, str>, ::fluent_bundle::FluentValue<'static>>);

        impl Args {
            fn new() -> Self {
                Self::default()
            }

            fn set(&mut self, key: &'static str, value: ::fluent_bundle::FluentValue<'static>) {
                self.0.insert(::std::borrow::Cow::Borrowed(key), value);
            }
        }
        
        #[derive(Clone, PartialEq, Default)]
        pub struct L10n(::unic_langid::LanguageIdentifier);

        impl L10n {
            pub fn new(lang: ::unic_langid::LanguageIdentifier) -> Self {
                Self(lang)
            }
            
            pub fn language(&self) -> &::unic_langid::LanguageIdentifier {
                &self.0
            }

            fn get_raw(&self, id: &str, args: &Args) -> String {
                use ::fluent_templates::Loader;
                LOCALES.lookup_with_args(&self.0, id, &args.0)
            }
        }

        impl $crate::l10n::Localization for L10n {
            fn for_tag(tag: &str) -> ::std::option::Option<Self> {
                tag.parse::<::unic_langid::LanguageIdentifier>().ok().map(Self::new)
            }

            fn tag(&self) -> ::std::string::String {
                self.0.to_string()
            }
        }

        include!(concat!(env!("OUT_DIR"), "/l10n_accessors.rs"));
    };
}
