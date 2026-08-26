fn main() {
    // Both flags reach the `slint!` macro through rustc's environment, and
    // rustc compiles one crate at a time - so these apply to this crate only,
    // tests included. An application that declares components with an outlet
    // must say the same thing in its own build.rs; see the crate docs.
    //
    // `ComponentContainer` and the `component-factory` type are the only seam
    // Slint offers for putting a separately compiled component into a hole
    // another component left, and 1.17 keeps both behind this flag.
    println!("cargo::rustc-env=SLINT_ENABLE_EXPERIMENTAL_FEATURES=1");
    // Only the tests need this one: the element-query API the testing backend
    // exposes reads debug info the compiler otherwise leaves out.
    println!("cargo::rustc-env=SLINT_EMIT_DEBUG_INFO=1");
    println!("cargo::rerun-if-changed=build.rs");
}
