fn main() -> anyhow::Result<()> {
    // Generates the route tree from `src/routes.rs` and compiles
    // `src/app.slint` against it. Each `.slint` sits next to the `.rs` that
    // binds it; which one draws which page is decided by name.
    guinea_slint_build::compile("src")
}
