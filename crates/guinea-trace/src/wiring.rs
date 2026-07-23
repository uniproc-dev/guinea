use crate::{
    ScopeSpec, TracePolicy, init_subscriber, init_test_subscriber, install_policy, load_policy,
    normalize_policy, register_scopes,
};
use std::path::Path;
use std::sync::{Arc, Mutex};
use tracing_subscriber::filter::Targets;
use tracing_subscriber::fmt::writer::MakeWriter;

/// The pieces a consumer's `trace-scopes.toml` codegen (see
/// `guinea-trace-codegen`) produces: the full scope registry plus the
/// built-in policy defaults baked into that toml.
pub struct GeneratedScopes {
    pub all: &'static [ScopeSpec],
    pub builtin_enable: &'static [&'static str],
    pub builtin_disable_messages: &'static [&'static str],
    pub builtin_disable_targets: &'static [&'static str],
}

pub fn builtin_policy_from(scopes: &GeneratedScopes) -> TracePolicy {
    normalize_policy(TracePolicy {
        enabled_prefixes: scopes.builtin_enable.iter().map(|v| v.to_string()).collect(),
        disabled_prefixes: Vec::new(),
        disabled_message_prefixes: scopes
            .builtin_disable_messages
            .iter()
            .map(|v| v.to_string())
            .collect(),
        disabled_target_prefixes: scopes
            .builtin_disable_targets
            .iter()
            .map(|v| v.to_string())
            .collect(),
        dump_capacity: TracePolicy::default().dump_capacity,
    })
}

/// Wires up scope registration, policy loading/installation, and subscriber
/// init from a consumer's generated scopes and its default `Targets` filter.
pub fn init_traced_subscriber<W>(
    settings_path: &Path,
    writer: W,
    scopes: &GeneratedScopes,
    default_targets: Targets,
) -> anyhow::Result<()>
where
    W: for<'writer> MakeWriter<'writer> + Send + Sync + 'static,
{
    register_scopes(scopes.all);

    let policy = load_policy(settings_path, builtin_policy_from(scopes));
    let dump_capacity = policy.dump_capacity;
    install_policy(policy);

    init_subscriber(writer, dump_capacity, default_targets)
}

pub fn init_traced_test_subscriber<W>(
    writer: W,
    test_storage: Arc<Mutex<Vec<String>>>,
    scopes: &GeneratedScopes,
    default_targets: Targets,
) -> anyhow::Result<()>
where
    W: for<'writer> MakeWriter<'writer> + Send + Sync + 'static,
{
    register_scopes(scopes.all);

    init_test_subscriber(writer, 64, default_targets, test_storage)
}
