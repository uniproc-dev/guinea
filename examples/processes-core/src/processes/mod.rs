pub mod actor;
pub mod contracts;
pub mod install;

/// The pid a row carries.
///
/// The list is a `Vec<String>` shaped like "name (pid 42)", and every front
/// end reads it back the same way - which is what keeps the reducer's state
/// identical between them instead of each growing its own shape for it.
pub fn pid_at(items: &[String], index: usize) -> Option<u32> {
    let row = items.get(index)?;
    row.rsplit_once("(pid ")
        .and_then(|(_, rest)| rest.trim_end_matches(')').trim().parse().ok())
}
