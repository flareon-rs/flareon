/// Debug mode flag
///
/// This enables some expensive operations that are useful for debugging, such
/// as logging additional information, and collecting some extra diagnostics
/// for generating error pages. This hurts the performance, so it should be
/// disabled for production.
///
/// This is `true` when the application is compiled in debug mode, and `false`
/// when it is compiled in release mode.
pub(crate) const DEBUG_MODE: bool = cfg!(debug_assertions);

/// Whether to display a nice, verbose error page when an error, panic, or
/// 404 "Not Found" occurs.
pub(crate) const DISPLAY_ERROR_PAGE: bool = DEBUG_MODE;

pub(crate) const REGISTER_PANIC_HOOK: bool = true;
