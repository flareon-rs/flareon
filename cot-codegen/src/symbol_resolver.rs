use std::collections::HashMap;
use std::fmt::Display;
use std::iter::FromIterator;
use std::path::Path;

use syn::UseTree;
use tracing::warn;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SymbolResolver {
    /// List of imports in the format `"HashMap" -> VisibleSymbol`
    symbols: HashMap<String, VisibleSymbol>,
}

impl SymbolResolver {
    #[must_use]
    pub fn new(symbols: Vec<VisibleSymbol>) -> Self {
        let mut symbol_map = HashMap::new();
        for symbol in symbols {
            symbol_map.insert(symbol.alias.clone(), symbol);
        }

        Self {
            symbols: symbol_map,
        }
    }

    #[must_use]
    pub fn from_file(file: &syn::File, module_path: &Path) -> Self {
        let imports = Self::get_imports(file, &ModulePath::from_fs_path(module_path));
        Self::new(imports)
    }

    /// Return the list of top-level `use` statements, structs, and constants as
    /// a list of [`VisibleSymbol`]s from the file.
    fn get_imports(file: &syn::File, module_path: &ModulePath) -> Vec<VisibleSymbol> {
        let mut imports = Vec::new();

        for item in &file.items {
            match item {
                syn::Item::Use(item) => {
                    imports.append(&mut VisibleSymbol::from_item_use(item, module_path));
                }
                syn::Item::Struct(item_struct) => {
                    imports.push(VisibleSymbol::from_item_struct(item_struct, module_path));
                }
                syn::Item::Const(item_const) => {
                    imports.push(VisibleSymbol::from_item_const(item_const, module_path));
                }
                _ => {}
            }
        }

        imports
    }

    pub fn resolve_struct(&self, item: &mut syn::ItemStruct) {
        for field in &mut item.fields {
            self.resolve(&mut field.ty);
        }
    }

    pub fn resolve(&self, ty: &mut syn::Type) {
        if let syn::Type::Path(path) = ty {
            self.resolve_type_path(path);
        }
    }

    /// Checks the provided `TypePath` and resolves the full type path, if
    /// available.
    fn resolve_type_path(&self, path: &mut syn::TypePath) {
        let first_segment = path.path.segments.first();

        if let Some(first_segment) = first_segment {
            if let Some(symbol) = self.symbols.get(&first_segment.ident.to_string()) {
                let mut new_segments: Vec<_> = symbol
                    .full_path_parts()
                    .map(|s| syn::PathSegment {
                        ident: syn::Ident::new(s, first_segment.ident.span()),
                        arguments: syn::PathArguments::None,
                    })
                    .collect();

                let first_arguments = first_segment.arguments.clone();
                new_segments
                    .last_mut()
                    .expect("new_segments must have at least one element")
                    .arguments = first_arguments;

                new_segments.extend(path.path.segments.iter().skip(1).cloned());
                path.path.segments = syn::punctuated::Punctuated::from_iter(new_segments);
            }

            for segment in &mut path.path.segments {
                self.resolve_path_arguments(&mut segment.arguments);
            }
        }
    }

    fn resolve_path_arguments(&self, arguments: &mut syn::PathArguments) {
        if let syn::PathArguments::AngleBracketed(args) = arguments {
            for arg in &mut args.args {
                self.resolve_generic_argument(arg);
            }
        }
    }

    fn resolve_generic_argument(&self, arg: &mut syn::GenericArgument) {
        if let syn::GenericArgument::Type(syn::Type::Path(path)) = arg {
            if let Some(new_arg) = self.try_resolve_generic_const(path) {
                *arg = new_arg;
            } else {
                self.resolve_type_path(path);
            }
        }
    }

    fn try_resolve_generic_const(&self, path: &syn::TypePath) -> Option<syn::GenericArgument> {
        if path.qself.is_none() && path.path.segments.len() == 1 {
            let segment = path
                .path
                .segments
                .first()
                .expect("segments have exactly one element");
            if segment.arguments.is_none() {
                let ident = segment.ident.to_string();
                if let Some(symbol) = self.symbols.get(&ident) {
                    if symbol.kind == VisibleSymbolKind::Const {
                        let path = &symbol.full_path;
                        return Some(syn::GenericArgument::Const(
                            syn::parse_str(path).expect("full_path should be a valid path"),
                        ));
                    }
                }
            }
        }

        None
    }
}

/// Represents a symbol visible in the current module. This might mean there is
/// a `use` statement for a given type, but also, for instance, the type is
/// defined in the current module.
///
/// For instance, for `use std::collections::HashMap;` the `VisibleSymbol `
/// would be:
/// ```
/// use cot_codegen::symbol_resolver::{VisibleSymbol, VisibleSymbolKind};
///
/// let _ = VisibleSymbol {
///     alias: String::from("HashMap"),
///     full_path: String::from("std::collections::HashMap"),
///     kind: VisibleSymbolKind::Use,
/// };
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VisibleSymbol {
    pub alias: String,
    pub full_path: String,
    pub kind: VisibleSymbolKind,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum VisibleSymbolKind {
    Use,
    Struct,
    Const,
}

impl VisibleSymbol {
    #[must_use]
    pub fn new(alias: &str, full_path: &str, kind: VisibleSymbolKind) -> Self {
        assert_ne!(alias, "", "alias must not be empty");
        assert!(!alias.contains("::"), "alias must not contain '::'");
        Self {
            alias: alias.to_string(),
            full_path: full_path.to_string(),
            kind,
        }
    }

    fn full_path_parts(&self) -> impl Iterator<Item = &str> {
        self.full_path.split("::")
    }

    fn new_use(alias: &str, full_path: &str) -> Self {
        Self::new(alias, full_path, VisibleSymbolKind::Use)
    }

    fn from_item_use(item: &syn::ItemUse, module_path: &ModulePath) -> Vec<Self> {
        Self::from_tree(&item.tree, module_path)
    }

    fn from_item_struct(item: &syn::ItemStruct, module_path: &ModulePath) -> Self {
        let ident = item.ident.to_string();
        let full_path = Self::module_path(module_path, &ident);

        Self {
            alias: ident,
            full_path,
            kind: VisibleSymbolKind::Struct,
        }
    }

    fn from_item_const(item: &syn::ItemConst, module_path: &ModulePath) -> Self {
        let ident = item.ident.to_string();
        let full_path = Self::module_path(module_path, &ident);

        Self {
            alias: ident,
            full_path,
            kind: VisibleSymbolKind::Const,
        }
    }

    fn module_path(module_path: &ModulePath, ident: &str) -> String {
        format!("{module_path}::{ident}")
    }

    fn from_tree(tree: &UseTree, current_module: &ModulePath) -> Vec<Self> {
        match tree {
            UseTree::Path(path) => {
                let ident = path.ident.to_string();
                let resolved_path = if ident == "crate" {
                    current_module.crate_name().to_string()
                } else if ident == "self" {
                    current_module.to_string()
                } else if ident == "super" {
                    current_module.parent().to_string()
                } else {
                    ident
                };

                return Self::from_tree(&path.tree, current_module)
                    .into_iter()
                    .map(|import| {
                        Self::new_use(
                            &import.alias,
                            &format!("{}::{}", resolved_path, import.full_path),
                        )
                    })
                    .collect();
            }
            UseTree::Name(name) => {
                let ident = name.ident.to_string();
                return vec![Self::new_use(&ident, &ident)];
            }
            UseTree::Rename(rename) => {
                return vec![Self::new_use(
                    &rename.rename.to_string(),
                    &rename.ident.to_string(),
                )];
            }
            UseTree::Glob(_) => {
                warn!("Glob imports are not supported");
            }
            UseTree::Group(group) => {
                return group
                    .items
                    .iter()
                    .flat_map(|tree| Self::from_tree(tree, current_module))
                    .collect();
            }
        }

        vec![]
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModulePath {
    parts: Vec<String>,
}

impl ModulePath {
    #[must_use]
    pub fn from_fs_path(path: &Path) -> Self {
        let mut parts = vec![String::from("crate")];

        if path == Path::new("lib.rs") || path == Path::new("main.rs") {
            return Self { parts };
        }

        parts.append(
            &mut path
                .components()
                .map(|c| {
                    let component_str = c.as_os_str().to_string_lossy();
                    component_str
                        .strip_suffix(".rs")
                        .unwrap_or(&component_str)
                        .to_string()
                })
                .collect::<Vec<_>>(),
        );

        if parts
            .last()
            .expect("parts must have at least one component")
            == "mod"
        {
            parts.pop();
        }

        Self { parts }
    }

    #[must_use]
    fn parent(&self) -> Self {
        let mut parts = self.parts.clone();
        parts.pop();
        Self { parts }
    }

    #[must_use]
    fn crate_name(&self) -> &str {
        &self.parts[0]
    }
}

impl Display for ModulePath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.parts.join("::"))
    }
}

#[cfg(test)]
mod tests {
    use cot_codegen::symbol_resolver::VisibleSymbolKind;
    use quote::{quote, ToTokens};
    use syn::parse_quote;

    use super::*;

    #[test]
    fn imports() {
        let source = r"
use std::collections::HashMap;
use std::error::Error as StdError;
use std::fmt::{Debug, Display, Formatter};
use std::fs::*;
use rand as r;
use super::MyModel;
use crate::MyOtherModel;
use self::MyThirdModel;

struct MyFourthModel {}

const MY_CONSTANT: u8 = 42;
        ";

        let file = syn::parse_file(source).unwrap();
        let imports =
            SymbolResolver::get_imports(&file, &ModulePath::from_fs_path(Path::new("foo/bar.rs")));

        let expected = vec![
            VisibleSymbol {
                alias: "HashMap".to_string(),
                full_path: "std::collections::HashMap".to_string(),
                kind: VisibleSymbolKind::Use,
            },
            VisibleSymbol {
                alias: "StdError".to_string(),
                full_path: "std::error::Error".to_string(),
                kind: VisibleSymbolKind::Use,
            },
            VisibleSymbol {
                alias: "Debug".to_string(),
                full_path: "std::fmt::Debug".to_string(),
                kind: VisibleSymbolKind::Use,
            },
            VisibleSymbol {
                alias: "Display".to_string(),
                full_path: "std::fmt::Display".to_string(),
                kind: VisibleSymbolKind::Use,
            },
            VisibleSymbol {
                alias: "Formatter".to_string(),
                full_path: "std::fmt::Formatter".to_string(),
                kind: VisibleSymbolKind::Use,
            },
            VisibleSymbol {
                alias: "r".to_string(),
                full_path: "rand".to_string(),
                kind: VisibleSymbolKind::Use,
            },
            VisibleSymbol {
                alias: "MyModel".to_string(),
                full_path: "crate::foo::MyModel".to_string(),
                kind: VisibleSymbolKind::Use,
            },
            VisibleSymbol {
                alias: "MyOtherModel".to_string(),
                full_path: "crate::MyOtherModel".to_string(),
                kind: VisibleSymbolKind::Use,
            },
            VisibleSymbol {
                alias: "MyThirdModel".to_string(),
                full_path: "crate::foo::bar::MyThirdModel".to_string(),
                kind: VisibleSymbolKind::Use,
            },
            VisibleSymbol {
                alias: "MyFourthModel".to_string(),
                full_path: "crate::foo::bar::MyFourthModel".to_string(),
                kind: VisibleSymbolKind::Struct,
            },
            VisibleSymbol {
                alias: "MY_CONSTANT".to_string(),
                full_path: "crate::foo::bar::MY_CONSTANT".to_string(),
                kind: VisibleSymbolKind::Const,
            },
        ];
        assert_eq!(imports, expected);
    }

    #[test]
    fn import_resolver() {
        let resolver = SymbolResolver::new(vec![
            VisibleSymbol::new_use("MyType", "crate::models::MyType"),
            VisibleSymbol::new_use("HashMap", "std::collections::HashMap"),
        ]);

        let path = &mut parse_quote!(MyType);
        resolver.resolve_type_path(path);
        assert_eq!(
            quote!(crate::models::MyType).to_string(),
            path.into_token_stream().to_string()
        );

        let path = &mut parse_quote!(HashMap<String, u8>);
        resolver.resolve_type_path(path);
        assert_eq!(
            quote!(std::collections::HashMap<String, u8>).to_string(),
            path.into_token_stream().to_string()
        );

        let path = &mut parse_quote!(Option<MyType>);
        resolver.resolve_type_path(path);
        assert_eq!(
            quote!(Option<crate::models::MyType>).to_string(),
            path.into_token_stream().to_string()
        );
    }

    #[test]
    fn import_resolver_resolve_struct() {
        let resolver = SymbolResolver::new(vec![
            VisibleSymbol::new_use("MyType", "crate::models::MyType"),
            VisibleSymbol::new_use("HashMap", "std::collections::HashMap"),
            VisibleSymbol::new_use("LimitedString", "cot::db::LimitedString"),
            VisibleSymbol::new(
                "MY_CONSTANT",
                "crate::constants::MY_CONSTANT",
                VisibleSymbolKind::Const,
            ),
        ]);

        let mut actual = parse_quote! {
            struct Example {
                field_1: MyType,
                field_2: HashMap<String, MyType>,
                field_3: Option<String>,
                field_4: LimitedString<MY_CONSTANT>,
            }
        };
        resolver.resolve_struct(&mut actual);
        let expected = quote! {
            struct Example {
                field_1: crate::models::MyType,
                field_2: std::collections::HashMap<String, crate::models::MyType>,
                field_3: Option<String>,
                field_4: cot::db::LimitedString<{ crate::constants::MY_CONSTANT }>,
            }
        };
        assert_eq!(actual.into_token_stream().to_string(), expected.to_string());
    }
}
