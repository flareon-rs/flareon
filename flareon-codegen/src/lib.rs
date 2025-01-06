extern crate self as flareon_codegen;

pub mod expr;
mod maybe_unknown;
pub mod model;
#[cfg(feature = "symbol-resolver")]
pub mod symbol_resolver;

#[cfg(not(feature = "symbol-resolver"))]
pub mod symbol_resolver {
    /// Dummy SymbolResolver for use in contexts when it's not useful (e.g.
    /// macros which do not have access to the entire source tree to look
    /// for `use` statements anyway).
    ///
    /// This is defined as an empty enum so that it's entirely optimized out by
    /// the compiler, along with all functions that reference it.
    pub enum SymbolResolver {}

    impl SymbolResolver {
        pub fn resolve(&self, _: &mut syn::Type) {}
    }
}
