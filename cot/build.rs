#[cfg(all(
    feature = "db",
    not(any(feature = "sqlite", feature = "postgres", feature = "mysql"))
))]
compile_error!("feature \"db\" requires one of: \"sqlite\", \"postgres\", \"mysql\" to be enabled");

fn main() {
    // do nothing; this only checks the feature flags
}
