use heck::ToSnakeCase;
use proc_macro::TokenStream;
use proc_macro_crate::{FoundCrate, crate_name};
use proc_macro2::{TokenStream as Tokens, TokenTree};
use quote::{format_ident, quote, quote_spanned};
use syn::{
    Attribute, Expr, Fields, Ident, ImplItem, ImplItemFn, ItemImpl, ItemStruct, Path, Signature,
    Type, Visibility, braced, bracketed,
    parse::{Parse, ParseStream},
    parse_macro_input, parse_quote,
    visit_mut::{self, VisitMut},
};

pub(crate) fn instance(input: TokenStream) -> TokenStream {
    let input = Tokens::from(input);
    finish((|| {
        let definition = syn::parse2::<Definition>(input.clone())?;
        collect(State {
            runtime: runtime()?,
            input,
            index: 0,
            entries: Vec::new(),
            inherited: Vec::new(),
            definition,
        })
    })())
}

pub(crate) fn __instance_expand(input: TokenStream) -> TokenStream {
    let state = parse_macro_input!(input as State);
    finish(collect(state))
}

fn finish(result: syn::Result<Tokens>) -> TokenStream {
    result.unwrap_or_else(syn::Error::into_compile_error).into()
}

fn runtime() -> syn::Result<Tokens> {
    for package in ["trscene", "terrarium", "trinternal", "trengine"] {
        if let Ok(found) = crate_name(package) {
            return Ok(match found {
                FoundCrate::Itself => {
                    let name = format_ident!("{package}");
                    quote!(::#name)
                }
                FoundCrate::Name(name) => {
                    let name = format_ident!("{name}");
                    quote!(::#name)
                }
            });
        }
    }
    Err(syn::Error::new(
        proc_macro2::Span::call_site(),
        "instance! requires trscene or terrarium",
    ))
}

struct Parent {
    field: Ident,
    ty: Path,
    identity: bool,
}

struct Definition {
    item: ItemStruct,
    implementations: Vec<ItemImpl>,
    parents: Vec<Parent>,
    data: Option<Ident>,
    path: Path,
    inspect: bool,
    default: bool,
}

impl Parse for Definition {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let content;
        braced!(content in input);
        let input = &content;
        let mut item: ItemStruct = input.parse()?;
        if !item.generics.params.is_empty() || item.generics.where_clause.is_some() {
            return Err(syn::Error::new_spanned(
                &item.generics,
                "instance types must be concrete",
            ));
        }
        let mut path: Path = parse_quote!(crate);
        let mut inspect = true;
        let mut default = true;
        take_attrs(&mut item.attrs, "instance", |attr| {
            attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("path") {
                    path = meta.value()?.parse()?;
                } else if meta.path.is_ident("inspect") {
                    inspect = meta.value()?.parse::<syn::LitBool>()?.value;
                } else if meta.path.is_ident("default") {
                    default = meta.value()?.parse::<syn::LitBool>()?.value;
                } else {
                    return Err(meta.error("expected path, inspect, or default"));
                }
                Ok(())
            })
        })?;
        if path
            .segments
            .first()
            .is_none_or(|segment| segment.ident != "crate")
        {
            return Err(syn::Error::new_spanned(
                path,
                "the public instance path must start with crate",
            ));
        }
        let Fields::Named(fields) = &item.fields else {
            return Err(syn::Error::new_spanned(
                &item,
                "instance! requires named fields",
            ));
        };
        let mut parents = Vec::new();
        let mut data = None;
        let mut extends = false;
        for field in &fields.named {
            let field_name = field.ident.clone().unwrap();
            let roles: Vec<_> = field
                .attrs
                .iter()
                .filter(|attr| {
                    ["extends", "compose", "data"]
                        .iter()
                        .any(|role| attr.path().is_ident(role))
                })
                .collect();
            if !roles.is_empty()
                && field
                    .attrs
                    .iter()
                    .any(|attr| attr.path().is_ident("property"))
            {
                return Err(syn::Error::new_spanned(
                    field,
                    "composition providers cannot be replaceable properties",
                ));
            }
            if roles.len() > 1 {
                return Err(syn::Error::new_spanned(
                    field,
                    "a field may have only one composition role",
                ));
            }
            if let Some(role) = roles.first() {
                if field.attrs.iter().any(|attr| attr.path().is_ident("cfg")) {
                    return Err(syn::Error::new_spanned(
                        field,
                        "composition fields cannot be conditional",
                    ));
                }
                if role.path().is_ident("data") {
                    if data.replace(field_name).is_some() || extends {
                        return Err(syn::Error::new_spanned(
                            field,
                            "an instance has one identity provider",
                        ));
                    }
                } else {
                    if role.path().is_ident("extends") {
                        if extends || data.is_some() {
                            return Err(syn::Error::new_spanned(
                                field,
                                "an instance has one identity provider",
                            ));
                        }
                        extends = true;
                    }
                    let Type::Path(ty) = &field.ty else {
                        return Err(syn::Error::new_spanned(
                            &field.ty,
                            "a composed field requires a named instance type",
                        ));
                    };
                    if ty.qself.is_some()
                        || ty
                            .path
                            .segments
                            .iter()
                            .any(|segment| !segment.arguments.is_empty())
                    {
                        return Err(syn::Error::new_spanned(
                            ty,
                            "composed types must be concrete paths",
                        ));
                    }
                    parents.push(Parent {
                        field: field_name,
                        ty: ty.path.clone(),
                        identity: role.path().is_ident("extends"),
                    });
                }
            }
        }
        let mut implementations = Vec::new();
        while !input.is_empty() {
            let implementation: ItemImpl = input.parse()?;
            if implementation.trait_.is_some() || !implementation.generics.params.is_empty() {
                return Err(syn::Error::new_spanned(
                    implementation,
                    "expected an inherent impl for this instance",
                ));
            }
            let Type::Path(ty) = &*implementation.self_ty else {
                return Err(syn::Error::new_spanned(
                    implementation.self_ty,
                    "expected this instance type",
                ));
            };
            if !ty.path.is_ident(&item.ident) {
                return Err(syn::Error::new_spanned(
                    ty,
                    "the impl must belong to the declared instance",
                ));
            }
            implementations.push(implementation);
        }
        Ok(Self {
            item,
            implementations,
            parents,
            data,
            path,
            inspect,
            default,
        })
    }
}

#[derive(Clone)]
struct Override {
    name: Ident,
    dispatch: Tokens,
    signature: Signature,
}

#[derive(Clone)]
struct Entry {
    name: Ident,
    target: Tokens,
    api: Tokens,
    view: Tokens,
    instance: bool,
    inspection: Tokens,
    methods: Vec<Ident>,
    overrides: Vec<Override>,
    via: Option<(Ident, Path)>,
}

fn export_crate_paths(tokens: Tokens) -> Tokens {
    tokens
        .into_iter()
        .flat_map(|token| match token {
            TokenTree::Ident(ref ident) if ident == "crate" => {
                quote_spanned!(ident.span()=> $crate)
            }
            TokenTree::Group(group) => {
                let mut replacement =
                    proc_macro2::Group::new(group.delimiter(), export_crate_paths(group.stream()));
                replacement.set_span(group.span());
                Tokens::from(TokenTree::Group(replacement))
            }
            token => Tokens::from(token),
        })
        .collect()
}

struct PropertyApi {
    getter: Ident,
    setter: Ident,
    ty: Type,
    cfg: Vec<Attribute>,
    flatten: bool,
    validate: Option<Ident>,
}

fn group(input: ParseStream) -> syn::Result<Tokens> {
    let content;
    bracketed!(content in input);
    content.parse()
}

impl Parse for Entry {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let content;
        braced!(content in input);
        let name = content.parse()?;
        let target = group(&content)?;
        let api = group(&content)?;
        let view = group(&content)?;
        let inspection = group(&content)?;
        let names;
        bracketed!(names in content);
        let mut methods = Vec::new();
        while !names.is_empty() {
            methods.push(names.parse()?);
        }
        let instance = content.parse::<syn::LitBool>()?.value;
        let overrides_content;
        bracketed!(overrides_content in content);
        let mut overrides = Vec::new();
        while !overrides_content.is_empty() {
            let entry;
            braced!(entry in overrides_content);
            overrides.push(Override {
                name: entry.parse()?,
                dispatch: group(&entry)?,
                signature: syn::parse2(group(&entry)?)?,
            });
        }
        Ok(Self {
            name,
            target,
            api,
            view,
            inspection,
            methods,
            instance,
            overrides,
            via: None,
        })
    }
}

impl Entry {
    fn tokens(&self) -> Tokens {
        let Self {
            name,
            target,
            api,
            view,
            inspection,
            methods,
            instance,
            overrides,
            ..
        } = self;
        let overrides = overrides.iter().map(|entry| {
            let Override {
                name,
                dispatch,
                signature,
            } = entry;
            {
                let signature = export_crate_paths(quote!(#signature));
                quote!({ #name [#dispatch] [#signature] })
            }
        });
        quote!({ #name [#target] [#api] [#view] [#inspection] [#(#methods)*] #instance [#(#overrides)*] })
    }
}

struct State {
    runtime: Tokens,
    input: Tokens,
    index: usize,
    entries: Vec<Entry>,
    inherited: Vec<Entry>,
    definition: Definition,
}

impl Parse for State {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let runtime = group(input)?;
        let original = group(input)?;
        let index = input.parse::<syn::LitInt>()?.base10_parse()?;
        let entries_content;
        bracketed!(entries_content in input);
        let mut entries = Vec::new();
        while !entries_content.is_empty() {
            let field = entries_content.parse()?;
            let ty = syn::parse2(group(&entries_content)?)?;
            let mut entry: Entry = entries_content.parse()?;
            entry.via = Some((field, ty));
            entries.push(entry);
        }
        let inherited_content;
        bracketed!(inherited_content in input);
        let mut inherited = Vec::new();
        while !inherited_content.is_empty() {
            inherited.push(inherited_content.parse()?);
        }
        Ok(Self {
            definition: syn::parse2(original.clone())?,
            runtime,
            input: original,
            index,
            entries,
            inherited,
        })
    }
}

fn collect(mut state: State) -> syn::Result<Tokens> {
    if state.index > 0 {
        let parent = &state.definition.parents[state.index - 1];
        let provides_identity = state.inherited.iter().any(|entry| entry.instance);
        if provides_identity != parent.identity {
            let message = if parent.identity {
                "extends requires an instance identity; use compose for a capability"
            } else {
                "compose requires a capability without an identity; use extends for an instance"
            };
            return Err(syn::Error::new_spanned(&parent.field, message));
        }
        // Projection coherence checks provider uniqueness after Rust resolves aliases.
        for mut entry in state.inherited {
            entry.via = Some((parent.field.clone(), parent.ty.clone()));
            state.entries.push(entry);
        }
    }
    if let Some(parent) = state.definition.parents.get(state.index) {
        let runtime = &state.runtime;
        let original = &state.input;
        let index = syn::LitInt::new(
            &(state.index + 1).to_string(),
            proc_macro2::Span::call_site(),
        );
        let entries = state.entries.iter().map(|entry| {
            let (field, ty) = entry.via.as_ref().unwrap();
            let tokens = entry.tokens();
            quote!(#field [#ty] #tokens)
        });
        let parent = &parent.ty;
        return Ok(quote! {
            #parent! { #runtime::__instance_expand, [
                [#runtime] [#original] #index [#(#entries)*]
            ] }
        });
    }
    expand(state.definition, state.runtime, state.entries)
}

fn take_attrs(
    attrs: &mut Vec<Attribute>,
    name: &str,
    mut apply: impl FnMut(&Attribute) -> syn::Result<()>,
) -> syn::Result<()> {
    let mut error = None;
    attrs.retain(|attr| {
        if !attr.path().is_ident(name) && !attr.path().is_ident(&format!("r#{name}")) {
            return true;
        }
        if error.is_none() {
            error = apply(attr).err();
        }
        false
    });
    error.map_or(Ok(()), Err)
}

struct ReceiverFields<'a> {
    runtime: &'a Tokens,
    class: &'a Ident,
    receiver: Ident,
    mutable: bool,
}

impl VisitMut for ReceiverFields<'_> {
    fn visit_expr_mut(&mut self, expression: &mut Expr) {
        if let Expr::Field(field) = expression
            && matches!(&*field.base, Expr::Path(path) if path.path.is_ident("self"))
        {
            let runtime = self.runtime;
            let class = self.class;
            let receiver = &self.receiver;
            let projection = if self.mutable {
                quote!(project_mut)
            } else {
                quote!(project)
            };
            *field.base = parse_quote!(#runtime::Project::<#class>::#projection(#receiver));
            return;
        }
        if let Expr::Path(path) = expression
            && path.path.is_ident("self")
        {
            path.path = Path::from(self.receiver.clone());
            return;
        }
        visit_mut::visit_expr_mut(self, expression);
    }
}

struct BorrowedType {
    borrowed: bool,
}
impl VisitMut for BorrowedType {
    fn visit_type_reference_mut(&mut self, reference: &mut syn::TypeReference) {
        if reference.lifetime.is_none() {
            reference.lifetime = Some(parse_quote!('__instance));
            self.borrowed = true;
        }
        visit_mut::visit_type_reference_mut(self, reference);
    }
}

fn signature_aliases(
    signature: &mut Signature,
    class: &Ident,
    module: &Path,
    aliases: &mut Vec<Tokens>,
) {
    let method = signature
        .ident
        .to_string()
        .split('_')
        .map(|part| {
            let mut chars = part.chars();
            chars.next().map_or_else(String::new, |first| {
                first.to_uppercase().chain(chars).collect()
            })
        })
        .collect::<String>();
    let mut module = module.clone();
    module
        .segments
        .first_mut()
        .unwrap()
        .ident
        .set_span(class.span());
    let mut export = |ty: &mut Type, label: String| {
        let alias = format_ident!("__{class}{method}{label}");
        let mut definition = ty.clone();
        let mut visitor = BorrowedType { borrowed: false };
        visitor.visit_type_mut(&mut definition);
        let generics = visitor.borrowed.then(|| quote!(<'__instance>));
        aliases.push(quote!(#[doc(hidden)] pub type #alias #generics = #definition;));
        let arguments = visitor.borrowed.then(|| quote!(<'_>));
        *ty = syn::parse2(quote_spanned!(class.span()=> #module::#alias #arguments))
            .expect("exported override type");
    };
    for (index, argument) in signature.inputs.iter_mut().enumerate() {
        if let syn::FnArg::Typed(argument) = argument {
            export(&mut argument.ty, format!("Argument{index}"));
        }
    }
    if let syn::ReturnType::Type(_, ty) = &mut signature.output {
        export(ty, "Output".into());
    }
}

fn receiver_is_borrowed(receiver: &syn::Receiver) -> bool {
    matches!(receiver.kind, syn::ReceiverKind::Reference(..))
}

fn receiver_is_mutable(receiver: &syn::Receiver) -> bool {
    match &receiver.kind {
        syn::ReceiverKind::Reference(_, _, mutability) => mutability.is_some(),
        _ => receiver.mutability.is_some(),
    }
}

fn strip_argument_mutability(signature: &mut Signature) {
    for argument in &mut signature.inputs {
        match argument {
            syn::FnArg::Receiver(receiver) if !receiver_is_borrowed(receiver) => {
                receiver.mutability = None
            }
            syn::FnArg::Typed(argument) => {
                if let syn::Pat::Ident(pattern) = &mut *argument.pat {
                    pattern.mutability = None;
                }
            }
            _ => {}
        }
    }
}

fn arguments(signature: &Signature) -> syn::Result<Vec<Ident>> {
    signature
        .inputs
        .iter()
        .filter_map(|arg| match arg {
            syn::FnArg::Receiver(_) => None,
            syn::FnArg::Typed(arg) => Some(match &*arg.pat {
                syn::Pat::Ident(pattern) if pattern.subpat.is_none() => Ok(pattern.ident.clone()),
                _ => Err(syn::Error::new_spanned(
                    arg,
                    "instance API arguments must have names",
                )),
            }),
        })
        .collect()
}

fn expand(
    mut definition: Definition,
    runtime: Tokens,
    mut entries: Vec<Entry>,
) -> syn::Result<Tokens> {
    let class = definition.item.ident.clone();
    let api = format_ident!("{class}Ext");
    let view = format_ident!("{class}View");
    let visibility = definition.item.vis.clone();
    let is_instance = definition.data.is_some() || entries.iter().any(|entry| entry.instance);
    let mut methods: Vec<ImplItemFn> = Vec::new();
    let mut inherent = Vec::new();
    let mut helpers = Vec::new();
    let mut aliases = Vec::new();
    let mut defaults = Vec::new();
    let mut property_inspect = Vec::new();
    let Fields::Named(fields) = &mut definition.item.fields else {
        unreachable!()
    };
    for field in &mut fields.named {
        let field_name = field.ident.clone().unwrap();
        let ty = &field.ty;
        field.attrs.retain(|attr| {
            !["data", "extends", "compose"]
                .iter()
                .any(|role| attr.path().is_ident(role))
        });
        let mut property = false;
        let mut default: Option<Expr> = None;
        take_attrs(&mut field.attrs, "default", |attr| {
            default = Some(attr.parse_args()?);
            Ok(())
        })?;
        let mut sanitize: Option<Expr> = None;
        let mut borrow = false;
        let mut into = false;
        take_attrs(&mut field.attrs, "property", |attr| {
            property = true;
            if matches!(attr.meta, syn::Meta::Path(_)) {
                return Ok(());
            }
            attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("default") {
                    default = Some(meta.value()?.parse()?);
                } else if meta.path.is_ident("sanitize") {
                    sanitize = Some(meta.value()?.parse()?);
                } else if meta.path.is_ident("into") {
                    into = true;
                } else if meta.path.is_ident("borrow") {
                    borrow = true;
                } else {
                    return Err(meta.error("expected default, sanitize, borrow, or into"));
                }
                Ok(())
            })
        })?;
        let cfg: Vec<_> = field
            .attrs
            .iter()
            .filter(|attr| attr.path().is_ident("cfg"))
            .cloned()
            .collect();
        let default = default.map_or_else(
            || quote!(::core::default::Default::default()),
            |value| quote!(#value),
        );
        defaults.push(quote!(#(#cfg)* #field_name: #default));
        if property {
            let setter = format_ident!("set_{field_name}");
            let builder = format_ident!("with_{field_name}");
            let docs: Vec<_> = field
                .attrs
                .iter()
                .filter(|attr| attr.path().is_ident("doc"))
                .cloned()
                .collect();
            let (return_type, read) = if borrow {
                (
                    quote!(&#ty),
                    quote!(&#runtime::Project::<#class>::project(self).#field_name),
                )
            } else {
                (
                    quote!(#ty),
                    quote!(::core::clone::Clone::clone(&#runtime::Project::<#class>::project(self).#field_name)),
                )
            };
            let value =
                sanitize.map_or_else(|| quote!(value), |sanitize| quote!((#sanitize)(value)));
            methods.push(parse_quote! {
                #(#cfg)* #(#docs)*
                #[doc = concat!("Returns `", stringify!(#field_name), "`.")]
                fn #field_name(&self) -> #return_type { #read }
            });
            if borrow {
                let getter_mut = format_ident!("{field_name}_mut");
                methods.push(parse_quote! {
                    #(#cfg)* #(#docs)*
                    #[doc = concat!("Returns mutable access to `", stringify!(#field_name), "`.")]
                    fn #getter_mut(&mut self) -> &mut #ty {
                        &mut #runtime::Project::<#class>::project_mut(self).#field_name
                    }
                });
            }
            methods.push(parse_quote! {
                #(#cfg)* #[doc = concat!("Sets `", stringify!(#field_name), "`.")] #(#docs)*
                fn #setter(&mut self, value: #ty) { #runtime::Project::<#class>::project_mut(self).#field_name = #value; }
            });
            let builder_ty = if into {
                quote!(impl Into<#ty>)
            } else {
                quote!(#ty)
            };
            let builder_value = if into {
                quote!(value.into())
            } else {
                quote!(value)
            };
            methods.push(parse_quote! {
                #(#cfg)* #[doc = concat!("Returns this instance with `", stringify!(#field_name), "` set.")] #(#docs)*
                fn #builder(mut self, value: #builder_ty) -> Self where Self: Sized { self.#setter(#builder_value); self }
            });
            if definition.inspect {
                property_inspect.push(PropertyApi {
                    getter: field_name.clone(),
                    setter,
                    ty: ty.clone(),
                    cfg,
                    flatten: false,
                    validate: None,
                });
            }
        }
    }
    let public_prefix: Tokens = {
        let segments: Vec<_> = definition.path.segments.iter().skip(1).collect();
        quote_spanned!(class.span()=> $crate::#(#segments::)*)
    };
    let mut overrides = Vec::new();
    for implementation in definition.implementations {
        for item in implementation.items {
            match item {
                ImplItem::Fn(mut method) => {
                    let mut inspected = false;
                    let mut flatten = false;
                    let mut validate: Option<Ident> = None;
                    take_attrs(&mut method.attrs, "property", |attr| {
                        inspected = true;
                        if matches!(attr.meta, syn::Meta::Path(_)) {
                            return Ok(());
                        }
                        attr.parse_nested_meta(|meta| {
                            if meta.path.is_ident("flatten") {
                                flatten = true;
                            } else if meta.path.is_ident("validate") {
                                validate = Some(meta.value()?.parse()?);
                            } else {
                                return Err(meta.error("expected flatten or validate"));
                            }
                            Ok(())
                        })
                    })?;
                    if inspected {
                        if method.sig.inputs.len() != 1
                            || method.sig.receiver().is_none_or(|receiver| {
                                !receiver_is_borrowed(receiver) || receiver_is_mutable(receiver)
                            })
                        {
                            return Err(syn::Error::new_spanned(
                                &method.sig,
                                "a property getter requires only &self",
                            ));
                        }
                        let syn::ReturnType::Type(_, returned) = &method.sig.output else {
                            return Err(syn::Error::new_spanned(
                                &method.sig,
                                "a property getter must return a value",
                            ));
                        };
                        let ty = if let Type::Reference(reference) = &**returned {
                            (*reference.elem).clone()
                        } else {
                            (**returned).clone()
                        };
                        let getter = method.sig.ident.clone();
                        let setter = format_ident!("set_{getter}");
                        let cfg = method
                            .attrs
                            .iter()
                            .filter(|attr| attr.path().is_ident("cfg"))
                            .cloned()
                            .collect();
                        property_inspect.push(PropertyApi {
                            getter,
                            setter,
                            ty,
                            cfg,
                            flatten,
                            validate,
                        });
                    }
                    let mut override_path: Option<Path> = None;
                    take_attrs(&mut method.attrs, "override", |attr| {
                        override_path = Some(attr.parse_args()?);
                        Ok(())
                    })?;
                    if let Some(path) = override_path {
                        if path.segments.len() != 2 {
                            return Err(syn::Error::new_spanned(
                                path,
                                "expected #[r#override(Base::method)]",
                            ));
                        }
                        let target = &path.segments[0].ident;
                        let name = &path.segments[1].ident;
                        if name != &method.sig.ident {
                            return Err(syn::Error::new_spanned(
                                &method.sig.ident,
                                "the override must keep the inherited method name",
                            ));
                        }
                        let Some(entry) = entries
                            .iter_mut()
                            .find(|entry| &entry.name == target && entry.methods.contains(name))
                        else {
                            return Err(syn::Error::new_spanned(
                                name,
                                "the named composed API has no such method",
                            ));
                        };
                        let dispatch = format_ident!("__instance_override_{name}");
                        let mut original_signature = method.sig.clone();
                        signature_aliases(
                            &mut original_signature,
                            &class,
                            &definition.path,
                            &mut aliases,
                        );
                        let Some(receiver) = method.sig.receiver().cloned() else {
                            return Err(syn::Error::new_spanned(
                                method.sig,
                                "an override requires a receiver",
                            ));
                        };
                        if !receiver_is_borrowed(&receiver)
                            || !method.sig.generics.params.is_empty()
                        {
                            return Err(syn::Error::new_spanned(
                                method.sig,
                                "overrides require a borrowed receiver and concrete arguments",
                            ));
                        }
                        arguments(&method.sig)?;
                        let mutable = receiver_is_mutable(&receiver);
                        let receiver_name = format_ident!("receiver");
                        ReceiverFields {
                            runtime: &runtime,
                            class: &class,
                            receiver: receiver_name.clone(),
                            mutable,
                        }
                        .visit_block_mut(&mut method.block);
                        let mut helper_signature = method.sig.clone();
                        helper_signature.ident = dispatch.clone();
                        helper_signature
                            .generics
                            .params
                            .push(parse_quote!(__Receiver: #api + ?Sized));
                        helper_signature.inputs[0] = if mutable {
                            parse_quote!(receiver: &mut __Receiver)
                        } else {
                            parse_quote!(receiver: &__Receiver)
                        };
                        let body = method.block;
                        helpers.push(quote!(#[doc(hidden)] pub #helper_signature #body));
                        entry.overrides.retain(|entry| entry.name != *name);
                        entry.overrides.push(Override {
                            name: name.clone(),
                            dispatch: quote!(#class::#dispatch),
                            signature: original_signature,
                        });
                        overrides.push(name.clone());
                    } else if matches!(method.vis, Visibility::Public(_))
                        && method.sig.receiver().is_some()
                    {
                        method.vis = Visibility::Inherited;
                        if !method.sig.generics.params.is_empty() {
                            method
                                .sig
                                .generics
                                .make_where_clause()
                                .predicates
                                .push(parse_quote!(Self: Sized));
                        }
                        let mutable = receiver_is_mutable(method.sig.receiver().unwrap());
                        ReceiverFields {
                            runtime: &runtime,
                            class: &class,
                            receiver: Ident::new(
                                "self",
                                method.sig.receiver().unwrap().self_token.span,
                            ),
                            mutable,
                        }
                        .visit_block_mut(&mut method.block);
                        methods.push(method);
                    } else {
                        inherent.push(quote!(#method));
                    }
                }
                item => inherent.push(quote!(#item)),
            }
        }
    }
    let storage = format_ident!("{}", class.to_string().to_snake_case());
    let storage_mut = format_ident!("{storage}_mut");
    methods.push(parse_quote! {
        #[doc = concat!("Returns the composed `", stringify!(#class), "` storage.")]
        fn #storage(&self) -> &#class { #runtime::Project::<#class>::project(self) }
    });
    methods.push(parse_quote! {
        #[doc = concat!("Returns mutable composed `", stringify!(#class), "` storage.")]
        fn #storage_mut(&mut self) -> &mut #class { #runtime::Project::<#class>::project_mut(self) }
    });
    let mut builders = Vec::new();
    for method in &methods {
        let signature = &method.sig;
        let Some(property) = signature
            .ident
            .to_string()
            .strip_prefix("set_")
            .map(str::to_owned)
        else {
            continue;
        };
        if !matches!(signature.output, syn::ReturnType::Default)
            || signature.receiver().is_none_or(|receiver| {
                !receiver_is_borrowed(receiver) || !receiver_is_mutable(receiver)
            })
        {
            continue;
        }
        let builder = format_ident!("with_{property}");
        if methods.iter().any(|method| method.sig.ident == builder) {
            continue;
        }
        let setter = &signature.ident;
        let args = arguments(signature)?;
        let mut signature = signature.clone();
        signature.ident = builder;
        signature.inputs[0] = parse_quote!(mut self);
        signature.output = parse_quote!(-> Self);
        signature
            .generics
            .make_where_clause()
            .predicates
            .push(parse_quote!(Self: Sized));
        let cfg: Vec<_> = method
            .attrs
            .iter()
            .filter(|attr| attr.path().is_ident("cfg"))
            .collect();
        let getter = format_ident!("{property}");
        let docs: Vec<_> = methods
            .iter()
            .find(|method| method.sig.ident == getter)
            .into_iter()
            .flat_map(|method| &method.attrs)
            .filter(|attr| attr.path().is_ident("doc"))
            .cloned()
            .collect();
        builders.push(parse_quote! {
            #(#cfg)*
            #[doc = concat!("Returns this instance with `", #property, "` set.")]
            #(#docs)*
            #signature { self.#setter(#(#args),*); self }
        });
    }
    methods.extend(builders);
    let mut promoted = std::collections::HashSet::new();
    for entry in &entries {
        for method in &entry.methods {
            if !promoted.insert(method.to_string()) {
                return Err(syn::Error::new_spanned(
                    &class,
                    format!("ambiguous composed method `{method}`"),
                ));
            }
        }
    }
    for method in &methods {
        if !promoted.insert(method.sig.ident.to_string()) {
            return Err(syn::Error::new_spanned(
                &method.sig.ident,
                "this method already exists; use an explicit override",
            ));
        }
    }
    let item = definition.item;
    let owner_mut = is_instance.then(
        || quote!(fn owner_mut(&mut self) -> Option<&mut dyn #runtime::Instance> { Some(self) }),
    );
    let own_project = quote! {
        impl #runtime::Project<#class> for #class {
            fn project(&self) -> &#class { self }
            fn project_mut(&mut self) -> &mut #class { self }
            #owner_mut
        }
    };
    let data_project = definition.data.as_ref().map(|field| {
        quote! {
            unsafe impl #runtime::__InstanceDataAccess for #class {
                fn __instance_data(&self) -> &#runtime::InstanceData { &self.#field }
                unsafe fn __instance_data_mut(&mut self) -> &mut #runtime::InstanceData { &mut self.#field }
            }
        }
    });
    let inherited_projects = entries.iter().map(|entry| {
        let target = &entry.target;
        let api = &entry.api;
        let (field, parent) = entry.via.as_ref().unwrap();
        let overrides = entry.overrides.iter().map(|entry| {
            let Override { signature, dispatch, .. } = entry;
            let mut signature = signature.clone();
            strip_argument_mutability(&mut signature);
            let args = arguments(&signature).unwrap();
            let receiver = Ident::new("self", signature.receiver().unwrap().self_token.span);
            quote!(#signature { #dispatch(#receiver, #(#args),*) })
        });
        quote! {
            impl #runtime::Project<#target> for #class {
                fn project(&self) -> &#target { <#parent as #runtime::Project<#target>>::project(&self.#field) }
                fn project_mut(&mut self) -> &mut #target { <#parent as #runtime::Project<#target>>::project_mut(&mut self.#field) }
                #owner_mut
            }
            impl #api for #class { #(#overrides)* }
        }
    });
    let inherited_data = if is_instance && definition.data.is_none() {
        let (field, parent) = entries
            .iter()
            .find(|entry| entry.instance)
            .unwrap()
            .via
            .as_ref()
            .unwrap();
        Some(quote! {
            unsafe impl #runtime::__InstanceDataAccess for #class {
                fn __instance_data(&self) -> &#runtime::InstanceData { <#parent as #runtime::__InstanceDataAccess>::__instance_data(&self.#field) }
                unsafe fn __instance_data_mut(&mut self) -> &mut #runtime::InstanceData {
                    unsafe { <#parent as #runtime::__InstanceDataAccess>::__instance_data_mut(&mut self.#field) }
                }
            }
        })
    } else {
        None
    };
    let supers = entries.iter().map(|entry| &entry.api);
    let receiver_super = is_instance.then(|| quote!(+ #runtime::InstanceReceiver));
    let method_tokens = methods.iter().map(|method| {
        let ImplItemFn {
            attrs, sig, block, ..
        } = method;
        quote!(#(#attrs)* #sig #block)
    });
    let forwarded = methods
        .iter()
        .filter(|method| {
            method.sig.receiver().is_some_and(receiver_is_borrowed)
                && method.sig.generics.where_clause.is_none()
        })
        .map(|method| {
            let signature = &method.sig;
            let name = &signature.ident;
            let args = arguments(signature)?;
            let attrs = &method.attrs;
            let self_ident = Ident::new("self", signature.receiver().unwrap().self_token.span);
            let receiver = if receiver_is_mutable(signature.receiver().unwrap()) {
                quote!(&mut **#self_ident)
            } else {
                quote!(&**#self_ident)
            };
            Ok(quote!(#(#attrs)* #signature { <__Receiver as #api>::#name(#receiver, #(#args),*) }))
        })
        .collect::<syn::Result<Vec<_>>>()?;
    let default_impl = definition.default.then(|| {
        let set_name = is_instance
            .then(|| quote!(let mut value = value; #runtime::Instance::set_name(&mut value, stringify!(#class).into());));
        quote! {
            impl ::core::default::Default for #class {
                fn default() -> Self {
                    let value = Self { #(#defaults),* };
                    #set_name
                    value
                }
            }
        }
    });
    let type_queries = std::iter::once((quote!(#class), quote!(dyn #view))).chain(entries.iter().map(|entry| {
        let target = &entry.target;
        let view = &entry.view;
        (quote!(#target), quote!(dyn #view))
    })).map(|(target, target_view)| quote! {
        if target == ::core::any::TypeId::of::<#target>() {
            return Some(&#runtime::InstanceQuery::<#target_view> {
                shared: |value| value.downcast_ref::<Self>().expect("instance query receiver"),
                mutable: |value| value.downcast_mut::<Self>().expect("instance query receiver"),
            });
        }
    });
    let spatial = quote! {
        fn spatial(&self) -> Option<&dyn #runtime::Spatial> {
            use #runtime::__DetectSpatial as _;
            (&&#runtime::__SpatialProbe::<Self>(::core::marker::PhantomData)).get(self)
        }
        fn spatial_mut(&mut self) -> Option<&mut dyn #runtime::Spatial> {
            use #runtime::__DetectSpatial as _;
            (&&#runtime::__SpatialProbe::<Self>(::core::marker::PhantomData)).get_mut(self)
        }
    };
    let instance_impl = is_instance.then(|| quote! {
        impl #runtime::InstanceReceiver for #class {
            fn as_instance(&self) -> &dyn #runtime::Instance { self }
            fn as_instance_mut(&mut self) -> &mut dyn #runtime::Instance { self }
        }
        impl #runtime::Instance for #class {
            fn query(&self, target: ::core::any::TypeId) -> Option<&'static dyn ::core::any::Any> {
                #(#type_queries)*
                None
            }
            fn instance_class(&self) -> Option<&'static #runtime::InstanceClass> {
                Some(<Self as #runtime::InstanceType>::descriptor())
            }
            #runtime::__impl_instance_inspection!(#class);

            #spatial

            fn class_name(&self) -> &'static str {
                stringify!(#class)
            }

        }
    });
    let parents = definition.parents.iter().map(|parent| &parent.ty);
    let instance_base = definition
        .data
        .as_ref()
        .map(|_| quote!(<dyn #runtime::Instance as #runtime::InstanceType>::descriptor,));
    let mut schema_entries = vec![Entry {
        name: class.clone(),
        target: quote!(#public_prefix #class),
        api: quote!(#public_prefix #api),
        view: quote!(#public_prefix #view),
        inspection: quote!(#public_prefix #class),
        methods: methods
            .iter()
            .map(|method| method.sig.ident.clone())
            .collect(),
        instance: is_instance,
        overrides: Vec::new(),
        via: None,
    }];
    schema_entries.extend(entries.iter().cloned().map(|mut entry| {
        for replacement in &mut entry.overrides {
            if overrides.iter().any(|name| name == &replacement.name) {
                let dispatch = format_ident!("__instance_override_{}", replacement.name);
                replacement.dispatch = quote!(#public_prefix #class::#dispatch);
            }
        }
        entry
    }));
    let schema_entries = schema_entries.iter().map(Entry::tokens);
    let schema = format_ident!(
        "__terrarium_instance_{}_{}",
        definition
            .path
            .segments
            .iter()
            .map(|segment| segment.ident.to_string())
            .collect::<Vec<_>>()
            .join("_"),
        class
    );
    let inspection = {
        let reads = property_inspect.iter().map(|property| {
            let PropertyApi {
                getter,
                ty,
                cfg,
                flatten,
                ..
            } = property;
            let path = getter.to_string();
            let prefix =
                (!flatten).then(|| quote!(.into_iter().map(|property| property.prefixed(#path))));
            quote! {
                #(#cfg)* {
                    let value: #ty = #api::#getter(receiver).clone();
                    use #runtime::__InspectInstanceProperty as _;
                    let probe = &&#runtime::__InstancePropertyProbe::<#ty>(::core::marker::PhantomData);
                    properties.extend(probe.properties(&value) #prefix);
                }
            }
        });
        let writes = property_inspect.iter().map(|property| {
            let PropertyApi { getter, setter, ty, cfg, flatten, validate } = property;
            let path = getter.to_string();
            let tail = if *flatten { quote!(Some(path)) } else { quote!(#runtime::devtools::property_tail(path, #path)) };
            let validation = validate.as_ref().map(|validate| quote! {
                if !#api::#validate(receiver, updated.clone()) { return Err(#runtime::devtools::PropertyError::InvalidValue); }
            });
            quote! {
                #(#cfg)*
                if let Some(tail) = #tail {
                    let mut updated: #ty = #api::#getter(receiver).clone();
                    use #runtime::__InspectInstanceProperty as _;
                    let probe = &&#runtime::__InstancePropertyProbe::<#ty>(::core::marker::PhantomData);
                    match probe.set_property(&mut updated, tail, value.clone()) {
                        Err(#runtime::devtools::PropertyError::NotFound) => {},
                        Err(error) => return Err(error),
                        Ok(()) => {
                            #validation
                            #api::#setter(receiver, updated);
                            return Ok(());
                        }
                    }
                }
            }
        });
        let inspect_impl = definition.inspect.then(|| {
            let inherited = entries.iter().map(|entry| &entry.inspection).collect::<Vec<_>>();
            let read_data = is_instance.then(|| quote!(properties.extend(#runtime::devtools::Inspect::properties(#runtime::__InstanceDataAccess::__instance_data(self)));));
            // InstanceData inspection edits name and attributes; tree fields are readonly or skipped.
            let write_data = is_instance.then(|| quote!(return #runtime::devtools::Inspect::set_property(unsafe { #runtime::__InstanceDataAccess::__instance_data_mut(self) }, path, value);));
            let no_data = (!is_instance).then(|| quote!(Err(#runtime::devtools::PropertyError::NotFound)));
            quote! {
                impl #runtime::devtools::Inspect for #class {
                    fn properties(&self) -> Vec<#runtime::devtools::Property> {
                        let mut properties = Self::__instance_properties(self);
                        #read_data
                        #(properties.extend(#inherited::__instance_properties(self));)*
                        properties
                    }
                    fn validate_property(&self, path: &str, value: &#runtime::devtools::PropertyValue) -> Result<(), #runtime::devtools::PropertyError> {
                        let mut copy = self.clone();
                        #runtime::devtools::Inspect::set_property(&mut copy, path, value.clone())
                    }
                    fn set_property(&mut self, path: &str, value: #runtime::devtools::PropertyValue) -> Result<(), #runtime::devtools::PropertyError> {
                        match Self::__instance_set_property(self, path, &value) {
                            Err(#runtime::devtools::PropertyError::NotFound) => {}, result => return result,
                        }
                        #(
                            match #inherited::__instance_set_property(self, path, &value) {
                                Err(#runtime::devtools::PropertyError::NotFound) => {}, result => return result,
                            }
                        )*
                        #write_data
                        #no_data
                    }
                }
            }
        });
        quote! {
            impl #class {
                #[doc(hidden)]
                pub fn __instance_properties<__Receiver: #api + ?Sized>(receiver: &__Receiver) -> Vec<#runtime::devtools::Property> {
                    let _ = receiver;
                    let mut properties = Vec::new();
                    #(#reads)*
                    properties
                }
                #[doc(hidden)]
                pub fn __instance_set_property<__Receiver: #api + ?Sized>(receiver: &mut __Receiver, path: &str, value: &#runtime::devtools::PropertyValue) -> Result<(), #runtime::devtools::PropertyError> {
                    let _ = (&receiver, path, value);
                    #(#writes)*
                    Err(#runtime::devtools::PropertyError::NotFound)
                }
            }
            #inspect_impl
        }
    };
    Ok(quote! {
        #item
        #(#aliases)*
        #own_project
        #data_project
        #inherited_data
        #(#inherited_projects)*
        #[doc = concat!("Methods provided by `", stringify!(#class), "` and its composed bases.")]
        #visibility trait #api: #runtime::Project<#class> #receiver_super #( + #supers)* {
            #(#method_tokens)*
        }
        impl #api for #class {}
        impl<__Receiver: #api + ?Sized> #api for &mut __Receiver { #(#forwarded)* }
        #[doc = concat!("An instance exposing the `", stringify!(#class), "` API.")]
        #visibility trait #view: #api + #runtime::Instance {}
        impl<__Receiver: #api + #runtime::Instance + ?Sized> #view for __Receiver {}
        impl #class { #(#inherent)* #(#helpers)* }
        #default_impl
        #instance_impl
        #runtime::__instance_inspection_items! { #inspection }
        impl #runtime::InstanceType for #class {
            type Api = dyn #view;
            fn descriptor() -> &'static #runtime::InstanceClass {
                &#runtime::InstanceClass {
                    name: stringify!(#class),
                    type_id: ::core::any::TypeId::of::<Self>,
                    bases: &[#instance_base #(<#parents as #runtime::InstanceType>::descriptor),*],
                }
            }
        }
        #[doc(hidden)]
        #[macro_export]
        macro_rules! #schema {
            ($callback:path, [$($state:tt)*]) => {
                $callback! { $($state)* [#(#schema_entries)*] }
            };
        }
        #[doc(hidden)]
        #[allow(unused_imports)]
        #visibility use #schema as #class;
    })
}
