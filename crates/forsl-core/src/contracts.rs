//! Compile-time registry of port/binding contracts, populated by the
//! `#[port]`/`#[bindings]` macros (see `forsl-codegen::contracts`) at their
//! definition site - no source re-parsing, no JSON round-trip.

pub struct PortStubMeta {
    pub trait_name: &'static str,
    pub msg_type_name: &'static str,
    pub feature: &'static str,
    /// Methods on the trait other than `send(&self, msg: ...)` - currently
    /// only `UiProcessesPort::get_selected_pid(&self) -> i32`, a synchronous
    /// UI->domain query that doesn't fit the fire-and-forget message shape.
    ///
    /// TODO: `get_selected_pid` is the only port method that breaks the
    /// `send(msg)`-only convention, and this whole mechanism (here, the
    /// parsing in `forsl-codegen::contracts::parse_port_extra_methods`, and
    /// the stub-side handler+call-count codegen in
    /// `forsl-codegen::stub_gen::generate_port_extra_method`) exists only to
    /// accommodate it. Once selected-pid tracking is reworked to flow
    /// through a message/signal instead of a synchronous query, delete
    /// `extra_methods` and this whole side-channel.
    pub extra_methods: &'static [PortExtraMethod],
}

pub struct PortExtraMethod {
    pub name: &'static str,
    pub args: &'static [(&'static str, &'static str)],
    pub output_ty: Option<&'static str>,
}

pub struct BindingStubMeta {
    pub trait_name: &'static str,
    pub feature: &'static str,
    pub methods: &'static [BindingMethodMeta],
}

pub struct BindingMethodMeta {
    pub name: &'static str,
    /// Types of the handler closure's arguments (from the `where F: Fn(...)`
    /// bound) - used both for stub generation and (via `slint_arg_types`
    /// below) for Slint `.slint` global codegen.
    pub arg_types: &'static [&'static str],
    /// `#[manual]` on the trait method - the adapter hand-writes this
    /// method's body instead of it being generated wholesale from the
    /// registry (see `slint-adapter/build.rs`).
    pub is_manual: bool,
    pub tracing_skip: bool,
    pub tracing_target: Option<&'static str>,
    /// Slint-specific rendering hints for `.slint` global codegen - opaque to
    /// generic consumers (e.g. `domain-test-kit`'s stub generator ignores
    /// these entirely). A different backend would carry its own equivalents.
    pub slint_name: Option<&'static str>,
    pub slint_arg_types: Option<&'static [&'static str]>,
    pub slint_global_override: Option<&'static str>,
    pub slint_skip: bool,
    pub slint_import: Option<&'static str>,
}

pub struct CapabilityMeta {
    pub key: &'static str,
    pub struct_name: &'static str,
}

inventory::collect!(PortStubMeta);
inventory::collect!(BindingStubMeta);
inventory::collect!(CapabilityMeta);

pub fn ports() -> impl Iterator<Item = &'static PortStubMeta> {
    inventory::iter::<PortStubMeta>()
}

pub fn bindings() -> impl Iterator<Item = &'static BindingStubMeta> {
    inventory::iter::<BindingStubMeta>()
}

pub fn capabilities() -> impl Iterator<Item = &'static CapabilityMeta> {
    inventory::iter::<CapabilityMeta>()
}
