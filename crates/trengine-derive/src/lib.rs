#![deny(missing_docs)]
//! Runtime context macros for Terrarium.

use proc_macro::TokenStream;
use proc_macro2::Ident;
use quote::quote;
use syn::{
    Expr, FnArg, ItemFn, Pat, Type,
    ext::IdentExt,
    parse::Parser,
    parse_quote_spanned,
    punctuated::Punctuated,
    spanned::Spanned,
    visit_mut::{self, VisitMut},
};

/// Makes `workspace` shorthand for per-use access to an engine's mutable workspace.
///
/// The function must have one `&mut Engine` parameter. Use `#[with_workspace(name)]`
/// to select a parameter explicitly, including when its type is an alias. Methods
/// can also select `self` explicitly.
///
/// ```no_run
/// use trengine::{Engine, with_workspace};
/// use trengine::trscene::{Instance, Part};
///
/// #[with_workspace]
/// fn initialize(engine: &mut Engine) {
///     workspace.add_child(Part::new());
///     engine.set_clear_color([0.0, 0.0, 0.0, 1.0]);
///     workspace.lighting.with_clock_time(18.0);
/// }
/// ```
///
/// Local `workspace` bindings shadow the shorthand. Nested items are not rewritten.
/// Standard assertion, formatting, and `vec!` macro expressions are supported;
/// custom macro syntax and implicit format-string captures are not rewritten.
/// Normal Rust borrowing rules still apply to references retained across uses.
///
/// ```compile_fail
/// use trengine_derive::with_workspace;
///
/// #[with_workspace]
/// fn missing_engine() {}
/// ```
///
/// ```compile_fail
/// use trengine_derive::with_workspace;
/// struct Engine;
///
/// #[with_workspace]
/// fn ambiguous(first: &mut Engine, second: &mut Engine) {}
/// ```
#[proc_macro_attribute]
pub fn with_workspace(attribute: TokenStream, item: TokenStream) -> TokenStream {
    expand(attribute.into(), item.into())
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

fn expand(
    attribute: proc_macro2::TokenStream,
    item: proc_macro2::TokenStream,
) -> syn::Result<proc_macro2::TokenStream> {
    let mut function: ItemFn = syn::parse2(item)?;
    let source = if attribute.is_empty() {
        let mut candidates = function.sig.inputs.iter().filter_map(|argument| {
            let FnArg::Typed(argument) = argument else {
                return None;
            };
            let Type::Reference(reference) = &*argument.ty else {
                return None;
            };
            let Type::Path(ty) = &*reference.elem else {
                return None;
            };
            let Pat::Ident(pattern) = &*argument.pat else {
                return None;
            };
            (reference.mutability.is_some() && ty.path.segments.last()?.ident == "Engine")
                .then(|| pattern.ident.clone())
        });
        let source = candidates.next().ok_or_else(|| syn::Error::new_spanned(
            &function.sig,
            "with_workspace requires a &mut Engine parameter; select aliases with #[with_workspace(name)]",
        ))?;
        if candidates.next().is_some() {
            return Err(syn::Error::new_spanned(
                &function.sig,
                "multiple Engine parameters; select one with #[with_workspace(name)]",
            ));
        }
        source
    } else {
        let source = Ident::parse_any.parse2(attribute)?;
        let found = function.sig.inputs.iter().any(|argument| match argument {
            FnArg::Typed(argument) => {
                matches!(&*argument.pat, Pat::Ident(pattern) if pattern.ident == source)
            }
            FnArg::Receiver(_) => source == "self",
        });
        if !found {
            return Err(syn::Error::new_spanned(
                source,
                "workspace source must name a function parameter",
            ));
        }
        source
    };
    if source == "workspace" {
        return Err(syn::Error::new_spanned(
            source,
            "the engine parameter cannot also be named workspace",
        ));
    }
    let mut rewrite = Rewrite {
        source,
        scope: Scope::default(),
        error: None,
    };
    for argument in &mut function.sig.inputs {
        if let FnArg::Typed(argument) = argument {
            rewrite.bind(&mut argument.pat);
        }
    }
    rewrite.scope.source_shadowed = false;
    rewrite.visit_block_mut(&mut function.block);
    if let Some(error) = rewrite.error {
        return Err(error);
    }
    Ok(quote!(#function))
}

#[derive(Clone, Copy, Default)]
struct Scope {
    workspace_bound: bool,
    source_shadowed: bool,
}

struct Rewrite {
    source: Ident,
    scope: Scope,
    error: Option<syn::Error>,
}

impl Rewrite {
    fn bind(&mut self, pattern: &mut Pat) {
        Bindings {
            source: &self.source,
            scope: &mut self.scope,
        }
        .visit_pat_mut(pattern);
    }

    fn bind_name(&mut self, name: &Ident) {
        self.scope.workspace_bound |= name == "workspace";
        self.scope.source_shadowed |= name == &self.source;
    }

    fn bind_import(&mut self, tree: &syn::UseTree) {
        match tree {
            syn::UseTree::Name(name) => self.bind_name(&name.ident),
            syn::UseTree::Rename(rename) => self.bind_name(&rename.rename),
            syn::UseTree::Path(path) => self.bind_import(&path.tree),
            syn::UseTree::Group(group) => {
                for tree in &group.items {
                    self.bind_import(tree);
                }
            }
            syn::UseTree::Glob(_) => {}
        }
    }
}

impl VisitMut for Rewrite {
    fn visit_expr_mut(&mut self, expression: &mut Expr) {
        if let Expr::Path(path) = expression
            && path.qself.is_none()
            && path.path.is_ident("workspace")
            && !self.scope.workspace_bound
        {
            if self.scope.source_shadowed {
                if self.error.is_none() {
                    self.error = Some(syn::Error::new_spanned(
                        path,
                        format!(
                            "implicit workspace cannot be used while {} is shadowed",
                            self.source
                        ),
                    ));
                }
                return;
            }
            let source = &self.source;
            let attributes = &path.attrs;
            *expression =
                parse_quote_spanned!(path.span()=> #(#attributes)* #source.workspace_mut());
            return;
        }
        visit_mut::visit_expr_mut(self, expression);
    }

    fn visit_block_mut(&mut self, block: &mut syn::Block) {
        let scope = self.scope;
        for statement in &block.stmts {
            if let syn::Stmt::Item(item) = statement {
                match item {
                    syn::Item::Fn(item) => self.bind_name(&item.sig.ident),
                    syn::Item::Const(item) => self.bind_name(&item.ident),
                    syn::Item::Static(item) => self.bind_name(&item.ident),
                    syn::Item::Use(item) => self.bind_import(&item.tree),
                    _ => {}
                }
            }
        }
        for statement in &mut block.stmts {
            match statement {
                syn::Stmt::Local(local) => {
                    if let Some(initializer) = &mut local.init {
                        self.visit_expr_mut(&mut initializer.expr);
                        if let Some((_, diverge)) = &mut initializer.diverge {
                            self.visit_expr_mut(diverge);
                        }
                    }
                    self.bind(&mut local.pat);
                }
                syn::Stmt::Expr(expression, _) => self.visit_expr_mut(expression),
                syn::Stmt::Macro(statement) => self.visit_macro_mut(&mut statement.mac),
                syn::Stmt::Item(_) => {}
            }
        }
        self.scope = scope;
    }

    fn visit_expr_closure_mut(&mut self, expression: &mut syn::ExprClosure) {
        let scope = self.scope;
        for pattern in &mut expression.inputs {
            self.bind(pattern);
        }
        self.visit_expr_mut(&mut expression.body);
        self.scope = scope;
    }

    fn visit_expr_for_loop_mut(&mut self, expression: &mut syn::ExprForLoop) {
        self.visit_expr_mut(&mut expression.expr);
        let scope = self.scope;
        self.bind(&mut expression.pat);
        self.visit_block_mut(&mut expression.body);
        self.scope = scope;
    }

    fn visit_arm_mut(&mut self, arm: &mut syn::Arm) {
        let scope = self.scope;
        self.bind(&mut arm.pat);
        self.visit_pat_mut(&mut arm.pat);
        self.visit_expr_mut(&mut arm.body);
        self.scope = scope;
    }

    fn visit_expr_let_mut(&mut self, expression: &mut syn::ExprLet) {
        self.visit_expr_mut(&mut expression.expr);
        self.bind(&mut expression.pat);
    }

    fn visit_expr_if_mut(&mut self, expression: &mut syn::ExprIf) {
        let scope = self.scope;
        self.visit_expr_mut(&mut expression.cond);
        self.visit_block_mut(&mut expression.then_branch);
        self.scope = scope;
        if let Some((_, branch)) = &mut expression.else_branch {
            self.visit_expr_mut(branch);
        }
        self.scope = scope;
    }

    fn visit_expr_while_mut(&mut self, expression: &mut syn::ExprWhile) {
        let scope = self.scope;
        self.visit_expr_mut(&mut expression.cond);
        self.visit_block_mut(&mut expression.body);
        self.scope = scope;
    }

    fn visit_field_value_mut(&mut self, field: &mut syn::FieldValue) {
        if field.colon_token.is_none()
            && matches!(&field.expr, Expr::Path(path) if path.path.is_ident("workspace"))
            && !self.scope.workspace_bound
        {
            field.colon_token = Some(Default::default());
        }
        self.visit_expr_mut(&mut field.expr);
    }

    fn visit_item_mut(&mut self, _: &mut syn::Item) {}

    fn visit_macro_mut(&mut self, mac: &mut syn::Macro) {
        let Some(name) = mac.path.segments.last() else {
            return;
        };
        let name = name.ident.to_string();
        if name == "vec" {
            let tokens = &mac.tokens;
            if let Ok(mut array) = syn::parse2::<Expr>(quote!([#tokens])) {
                self.visit_expr_mut(&mut array);
                mac.tokens = match array {
                    Expr::Array(array) => {
                        let elements = array.elems;
                        quote!(#elements)
                    }
                    Expr::Repeat(repeat) => {
                        let syn::ExprRepeat { expr, len, .. } = repeat;
                        quote!(#expr; #len)
                    }
                    _ => return,
                };
            }
            return;
        }
        if !matches!(
            name.as_str(),
            "assert"
                | "assert_eq"
                | "assert_ne"
                | "debug_assert"
                | "debug_assert_eq"
                | "debug_assert_ne"
                | "dbg"
                | "format"
                | "format_args"
                | "print"
                | "println"
                | "eprint"
                | "eprintln"
                | "write"
                | "writeln"
                | "panic"
        ) {
            return;
        }
        let parser = Punctuated::<Expr, syn::Token![,]>::parse_terminated;
        if let Ok(mut arguments) = parser.parse2(mac.tokens.clone()) {
            for argument in &mut arguments {
                if let Expr::Assign(assignment) = argument {
                    self.visit_expr_mut(&mut assignment.right);
                } else {
                    self.visit_expr_mut(argument);
                }
            }
            mac.tokens = quote!(#arguments);
        }
    }
}

struct Bindings<'a> {
    source: &'a Ident,
    scope: &'a mut Scope,
}

impl VisitMut for Bindings<'_> {
    fn visit_pat_ident_mut(&mut self, pattern: &mut syn::PatIdent) {
        self.scope.workspace_bound |= pattern.ident == "workspace";
        self.scope.source_shadowed |= pattern.ident == *self.source;
        visit_mut::visit_pat_ident_mut(self, pattern);
    }

    fn visit_expr_mut(&mut self, _: &mut Expr) {}
}
