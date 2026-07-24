fn main() {
    windows_reactor_setup::as_self_contained();
    guinea_codegen::l10n::build("locales");
}
