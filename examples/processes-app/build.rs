fn main() {
    // Bundles this machine's exact Windows App SDK runtime + WebView2 loader
    // next to the built binary and embeds the app manifest - no machine-wide
    // WinAppSDK install required. `as_framework_dependent()`'s bootstrap DLL
    // is pinned to runtime 2.1.3, which didn't match what's actually
    // installed here (WindowsAppRuntime.2 2.2.x/2.3.x), so WinRT couldn't
    // activate the reactor's compositor class ("Class not registered",
    // 0x80040154).
    windows_reactor_setup::as_self_contained();
}
