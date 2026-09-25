#[cfg(not(target_arch = "wasm32"))]
use std::fs;
use std::{
    collections::{HashMap, HashSet},
    path::{Component, Path, PathBuf},
    sync::{Arc, Mutex},
};

use rquickjs::{
    Array, Context, Ctx, Error, Function, Module, Object, Persistent, Runtime,
    loader::{ImportAttributes, Loader, Resolver},
};

use super::{JavaScriptResult, ScriptOptions};

/// Owns a QuickJS runtime and evaluates JavaScript source in its context.
pub(super) struct JavaScriptRuntime {
    context: Context,
    sources: Arc<Mutex<HashMap<String, String>>>,
    _runtime: Runtime,
}

struct ScriptResolver;

pub(super) fn normalized_module_path(path: &Path) -> String {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::ParentDir => {
                normalized.pop();
            }
            Component::CurDir => {}
            other => normalized.push(other.as_os_str()),
        }
    }
    normalized.to_string_lossy().into_owned()
}

impl Resolver for ScriptResolver {
    fn resolve<'js>(
        &mut self,
        _ctx: &Ctx<'js>,
        base: &str,
        name: &str,
        _attributes: Option<ImportAttributes<'js>>,
    ) -> JavaScriptResult<String> {
        if !name.starts_with("./") && !name.starts_with("../") {
            return Err(Error::new_resolving_message(
                base,
                name,
                "only relative script imports are supported",
            ));
        }
        let path = Path::new(base).parent().unwrap_or_else(|| Path::new(""));
        Ok(normalized_module_path(&path.join(name)))
    }
}

struct ScriptLoader {
    sources: Arc<Mutex<HashMap<String, String>>>,
}

const DI_DEPENDENCIES: &str = "__terrarium_di_dependencies";

impl Loader for ScriptLoader {
    fn load<'js>(
        &mut self,
        ctx: &Ctx<'js>,
        name: &str,
        _attributes: Option<ImportAttributes<'js>>,
    ) -> JavaScriptResult<Module<'js>> {
        let source = self
            .sources
            .lock()
            .expect("script sources lock poisoned")
            .get(name)
            .cloned();
        if let Some(source) = source {
            return Module::declare(ctx.clone(), name, instrument_module_source(&source));
        }
        #[cfg(not(target_arch = "wasm32"))]
        if Path::new(name).is_absolute() {
            let source = fs::read_to_string(name)
                .map_err(|error| Error::new_loading_message(name, error.to_string()))?;
            return Module::declare(ctx.clone(), name, instrument_module_source(&source));
        }
        Err(Error::new_loading_message(name, "script was not preloaded"))
    }
}

impl JavaScriptRuntime {
    /// Creates an isolated JavaScript runtime.
    pub(super) fn new() -> JavaScriptResult<Self> {
        let runtime = Runtime::new()?;
        let sources = Arc::new(Mutex::new(HashMap::new()));
        runtime.set_loader(
            ScriptResolver,
            ScriptLoader {
                sources: Arc::clone(&sources),
            },
        );
        // NOTE this could be like 960kb but maybe we decide this later
        #[cfg(target_arch = "wasm32")]
        runtime.set_max_stack_size(512 * 1024);
        let context = Context::full(&runtime)?;
        Ok(Self {
            context,
            sources,
            _runtime: runtime,
        })
    }

    pub(super) fn register_sources(&self, options: &[ScriptOptions]) {
        for option in options {
            self.register_source(option);
        }
    }

    pub(super) fn register_source(
        &self,
        option: &ScriptOptions,
    ) -> Option<(String, Option<String>)> {
        let source = option.source.as_ref()?;
        let path = normalized_module_path(Path::new(&option.module_path()));
        let previous = self
            .sources
            .lock()
            .expect("script sources lock poisoned")
            .insert(path.clone(), source.clone());
        Some((path, previous))
    }

    pub(super) fn restore_source(&self, previous: Option<(String, Option<String>)>) {
        if let Some((path, previous)) = previous {
            let mut sources = self.sources.lock().expect("script sources lock poisoned");
            if let Some(previous) = previous {
                sources.insert(path, previous);
            } else {
                sources.remove(&path);
            }
        }
    }

    /// Formats a JavaScript error, including the pending exception's message and stack trace.
    pub(super) fn format_error(&self, error: &Error) -> String {
        self.with_context(|ctx| {
            if !error.is_exception() {
                return error.to_string();
            }

            let value = ctx.catch();
            if let Some(exception) = value.as_exception() {
                return exception.to_string();
            }
            if let Some(string) = value.as_string().and_then(|value| value.to_string().ok()) {
                return string;
            }
            format!("threw {}: {value:?}", value.type_name())
        })
    }

    pub(super) fn with_context<F, R>(&self, callback: F) -> R
    where
        F: FnOnce(Ctx<'_>) -> R,
    {
        self.context.with(callback)
    }
}

pub(super) struct BuiltScriptNode {
    pub(super) constructor: Persistent<Function<'static>>,
    pub(super) instance: Persistent<Object<'static>>,
    pub(super) is_new: bool,
}

pub(super) struct BuiltModule {
    pub(super) nodes: Vec<BuiltScriptNode>,
}

pub(super) fn build_module(
    runtime: &JavaScriptRuntime,
    name: &str,
    source: &str,
    known_instances: &HashMap<Persistent<Function<'static>>, Persistent<Object<'static>>>,
) -> JavaScriptResult<BuiltModule> {
    runtime.with_context(|ctx| {
        let source = instrument_module_source(source);
        let module = Module::declare(ctx.clone(), name, source)?;
        let (module, promise) = module.eval()?;
        // Run jobs scheduled by the module before lifecycle hooks commit scene commands.
        while ctx.execute_pending_job() {}
        promise.finish::<()>()?;
        let namespace = module.namespace()?;
        let constructor: Function = namespace.get("default")?;
        let mut nodes = Vec::new();
        construct_class(
            &ctx,
            constructor,
            known_instances,
            &mut HashMap::new(),
            &mut HashSet::new(),
            &mut nodes,
        )?;
        Ok(BuiltModule { nodes })
    })
}

fn construct_class<'js>(
    ctx: &Ctx<'js>,
    constructor: Function<'js>,
    known_instances: &HashMap<Persistent<Function<'static>>, Persistent<Object<'static>>>,
    cache: &mut HashMap<Function<'js>, Object<'js>>,
    active: &mut HashSet<Function<'js>>,
    nodes: &mut Vec<BuiltScriptNode>,
) -> JavaScriptResult<Object<'js>> {
    if let Some(instance) = cache.get(&constructor) {
        return Ok(instance.clone());
    }
    if !active.insert(constructor.clone()) {
        let name = constructor
            .get::<_, String>("name")
            .unwrap_or_else(|_| "anonymous".to_owned());
        return Err(Error::new_from_js_message(
            "script dependency graph",
            "acyclic class dependencies",
            name,
        ));
    }

    let result = (|| {
        let dependencies = constructor
            .get::<_, Option<Array>>(DI_DEPENDENCIES)?
            .map(|dependencies| {
                dependencies
                    .iter::<Function>()
                    .collect::<JavaScriptResult<Vec<_>>>()
            })
            .transpose()?
            .unwrap_or_default();
        let mut values = Vec::with_capacity(dependencies.len());
        for dependency in dependencies {
            values.push(construct_class(
                ctx,
                dependency,
                known_instances,
                cache,
                active,
                nodes,
            )?);
        }
        let key = Persistent::save(ctx, constructor.clone());
        let is_new = !known_instances.contains_key(&key);
        let instance = if let Some(instance) = known_instances.get(&key) {
            instance.clone().restore(ctx)?
        } else {
            let construct: Function = ctx.globals().get("__internal_construct")?;
            construct.call((constructor.clone(), values))?
        };
        cache.insert(constructor.clone(), instance.clone());
        nodes.push(BuiltScriptNode {
            constructor: key,
            instance: Persistent::save(ctx, instance.clone()),
            is_new,
        });
        Ok(instance)
    })();

    active.remove(&constructor);
    result
}

fn instrument_module_source(source: &str) -> String {
    let mut source = source.to_owned();
    if let Some(declaration) = class_declarations(&source)
        .into_iter()
        .find(|declaration| declaration.is_default && declaration.name.is_none())
    {
        let export_start = declaration
            .export_start
            .expect("default class declarations have an export keyword");
        let synthetic_name = "__terrarium_anonymous_default";
        source.replace_range(
            export_start..declaration.keyword_start + "class".len(),
            &format!("class {synthetic_name}"),
        );
        source.push_str(&format!("\nexport default {synthetic_name};\n"));
    }

    let declarations = class_declarations(&source);
    let mut instrumented = source;
    for declaration in declarations {
        let Some(class_name) = declaration.name.as_deref() else {
            continue;
        };
        let dependencies = constructor_dependencies(&instrumented, &declaration);
        let dependencies = dependencies.join(", ");
        instrumented.push_str(&format!(
            "\n{class_name}[{DI_DEPENDENCIES:?}] = [{dependencies}];\n"
        ));
    }
    instrumented
}

#[derive(Clone)]
struct ClassDeclaration {
    keyword_start: usize,
    name: Option<String>,
    is_default: bool,
    export_start: Option<usize>,
}

fn class_declarations(source: &str) -> Vec<ClassDeclaration> {
    let bytes = source.as_bytes();
    let mut declarations = Vec::new();
    let mut index = 0;
    let mut brace_depth = 0usize;

    while index < bytes.len() {
        match bytes[index] {
            b'/' if bytes.get(index + 1) == Some(&b'/') => {
                index = source[index + 2..]
                    .find('\n')
                    .map_or(bytes.len(), |offset| index + 2 + offset + 1);
            }
            b'/' if bytes.get(index + 1) == Some(&b'*') => {
                index = source[index + 2..]
                    .find("*/")
                    .map_or(bytes.len(), |offset| index + 2 + offset + 2);
            }
            b'\'' | b'"' | b'`' => index = skip_quoted(source, index),
            b'{' => {
                brace_depth += 1;
                index += 1;
            }
            b'}' => {
                brace_depth = brace_depth.saturating_sub(1);
                index += 1;
            }
            byte if is_identifier_start(byte) => {
                let start = index;
                index = skip_identifier(bytes, index);
                if brace_depth != 0 || &source[start..index] != "class" {
                    continue;
                }

                let previous = previous_token(source, start);
                let previous_previous = previous
                    .as_ref()
                    .and_then(|token| previous_token(source, token.start));
                let is_default = previous
                    .as_ref()
                    .is_some_and(|token| token.text == "default")
                    && previous_previous
                        .as_ref()
                        .is_some_and(|token| token.text == "export");
                let is_named_export = previous
                    .as_ref()
                    .is_some_and(|token| token.text == "export");
                let declaration_boundary = previous.as_ref().is_none_or(|token| {
                    matches!(token.text.as_str(), ";" | "}")
                        || source[token.end..start].contains(['\n', '\r'])
                            && !matches!(token.text.as_str(), "=" | "=>" | "," | "(" | "[" | ":")
                });
                if !is_default && !is_named_export && !declaration_boundary {
                    continue;
                }

                let name_start = skip_trivia(source, index);
                let name_end = skip_identifier(bytes, name_start);
                let name = (name_end > name_start && &source[name_start..name_end] != "extends")
                    .then(|| source[name_start..name_end].to_owned());
                if name.is_some() || is_default {
                    declarations.push(ClassDeclaration {
                        keyword_start: start,
                        name,
                        is_default,
                        export_start: is_default
                            .then(|| previous_previous.as_ref().map(|token| token.start))
                            .flatten(),
                    });
                }
            }
            _ => index += 1,
        }
    }

    declarations
}

struct TokenBefore {
    start: usize,
    end: usize,
    text: String,
}

fn previous_token(source: &str, before: usize) -> Option<TokenBefore> {
    let bytes = source.as_bytes();
    let mut end = before;
    loop {
        while end > 0 && bytes[end - 1].is_ascii_whitespace() {
            end -= 1;
        }
        if end >= 2 && &bytes[end - 2..end] == b"*/" {
            let comment_start = source[..end - 2].rfind("/*")?;
            end = comment_start;
            continue;
        }
        let line_start = source[..end].rfind('\n').map_or(0, |position| position + 1);
        if let Some(comment_start) = source[line_start..end].rfind("//") {
            end = line_start + comment_start;
            continue;
        }
        break;
    }
    if end == 0 {
        return None;
    }
    let last = bytes[end - 1];
    let start = if is_identifier_continue(last) {
        let mut start = end - 1;
        while start > 0 && is_identifier_continue(bytes[start - 1]) {
            start -= 1;
        }
        start
    } else {
        end - 1
    };
    Some(TokenBefore {
        start,
        end,
        text: source[start..end].to_owned(),
    })
}

fn skip_trivia(source: &str, mut index: usize) -> usize {
    let bytes = source.as_bytes();
    loop {
        while index < bytes.len() && bytes[index].is_ascii_whitespace() {
            index += 1;
        }
        if bytes.get(index..index + 2) == Some(b"//") {
            index = source[index + 2..]
                .find('\n')
                .map_or(bytes.len(), |offset| index + 2 + offset + 1);
        } else if bytes.get(index..index + 2) == Some(b"/*") {
            index = source[index + 2..]
                .find("*/")
                .map_or(bytes.len(), |offset| index + 2 + offset + 2);
        } else {
            return index;
        }
    }
}

fn skip_quoted(source: &str, start: usize) -> usize {
    let bytes = source.as_bytes();
    let quote = bytes[start];
    let mut index = start + 1;
    while index < bytes.len() {
        if bytes[index] == b'\\' {
            index = (index + 2).min(bytes.len());
        } else if bytes[index] == quote {
            return index + 1;
        } else {
            index += 1;
        }
    }
    bytes.len()
}

fn skip_identifier(bytes: &[u8], mut index: usize) -> usize {
    if index < bytes.len() && is_identifier_start(bytes[index]) {
        index += 1;
        while index < bytes.len() && is_identifier_continue(bytes[index]) {
            index += 1;
        }
    }
    index
}

fn is_identifier_start(byte: u8) -> bool {
    byte == b'_' || byte == b'$' || byte.is_ascii_alphabetic()
}

fn is_identifier_continue(byte: u8) -> bool {
    is_identifier_start(byte) || byte.is_ascii_digit()
}

fn constructor_dependencies(source: &str, declaration: &ClassDeclaration) -> Vec<String> {
    let Some((body_start, body_end)) = class_body_range(source, declaration.keyword_start) else {
        return Vec::new();
    };
    let Some(constructor_start) = find_constructor(source, body_start, body_end) else {
        return Vec::new();
    };
    let Some(comment) = doc_comment_before(source, constructor_start) else {
        return Vec::new();
    };

    comment
        .lines()
        .filter_map(|line| {
            let marker = line.find("@param")?;
            let annotation = line[marker + "@param".len()..].trim();
            let start = annotation.find('{')? + 1;
            let end = annotation[start..].find('}')? + start;
            let dependency = annotation[start..end].trim();
            is_injectable_type(dependency).then(|| dependency.to_owned())
        })
        .collect()
}

fn class_body_range(source: &str, class_start: usize) -> Option<(usize, usize)> {
    let bytes = source.as_bytes();
    let mut index = class_start + "class".len();
    let mut parentheses = 0usize;
    let mut brackets = 0usize;
    while index < bytes.len() {
        match bytes[index] {
            b'/' if bytes.get(index + 1) == Some(&b'/') => {
                index = source[index + 2..]
                    .find('\n')
                    .map_or(bytes.len(), |offset| index + 2 + offset + 1);
            }
            b'/' if bytes.get(index + 1) == Some(&b'*') => {
                index = source[index + 2..]
                    .find("*/")
                    .map_or(bytes.len(), |offset| index + 2 + offset + 2);
            }
            b'\'' | b'"' | b'`' => index = skip_quoted(source, index),
            b'(' => {
                parentheses += 1;
                index += 1;
            }
            b')' => {
                parentheses = parentheses.saturating_sub(1);
                index += 1;
            }
            b'[' => {
                brackets += 1;
                index += 1;
            }
            b']' => {
                brackets = brackets.saturating_sub(1);
                index += 1;
            }
            b'{' if parentheses == 0 && brackets == 0 => {
                return matching_brace(source, index).map(|end| (index + 1, end));
            }
            _ => index += 1,
        }
    }
    None
}

fn matching_brace(source: &str, open: usize) -> Option<usize> {
    let bytes = source.as_bytes();
    let mut index = open + 1;
    let mut depth = 1usize;
    while index < bytes.len() {
        match bytes[index] {
            b'/' if bytes.get(index + 1) == Some(&b'/') => {
                index = source[index + 2..]
                    .find('\n')
                    .map_or(bytes.len(), |offset| index + 2 + offset + 1);
            }
            b'/' if bytes.get(index + 1) == Some(&b'*') => {
                index = source[index + 2..]
                    .find("*/")
                    .map_or(bytes.len(), |offset| index + 2 + offset + 2);
            }
            b'\'' | b'"' | b'`' => index = skip_quoted(source, index),
            b'{' => {
                depth += 1;
                index += 1;
            }
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(index);
                }
                index += 1;
            }
            _ => index += 1,
        }
    }
    None
}

fn find_constructor(source: &str, body_start: usize, body_end: usize) -> Option<usize> {
    let bytes = source.as_bytes();
    let mut index = body_start;
    let mut depth = 0usize;
    while index < body_end {
        match bytes[index] {
            b'/' if bytes.get(index + 1) == Some(&b'/') => {
                index = source[index + 2..]
                    .find('\n')
                    .map_or(body_end, |offset| index + 2 + offset + 1);
            }
            b'/' if bytes.get(index + 1) == Some(&b'*') => {
                index = source[index + 2..]
                    .find("*/")
                    .map_or(body_end, |offset| index + 2 + offset + 2);
            }
            b'\'' | b'"' | b'`' => index = skip_quoted(source, index),
            b'{' => {
                depth += 1;
                index += 1;
            }
            b'}' => {
                depth = depth.saturating_sub(1);
                index += 1;
            }
            byte if depth == 0 && is_identifier_start(byte) => {
                let start = index;
                index = skip_identifier(bytes, index);
                if &source[start..index] == "constructor"
                    && bytes.get(skip_trivia(source, index)) == Some(&b'(')
                {
                    return Some(start);
                }
            }
            _ => index += 1,
        }
    }
    None
}

fn doc_comment_before(source: &str, position: usize) -> Option<&str> {
    let before = source[..position].trim_end();
    if !before.ends_with("*/") {
        return None;
    }
    let comment_end = source[..position].rfind("*/")? + 2;
    if !source[comment_end..position].trim().is_empty() {
        return None;
    }
    let comment_start = source[..comment_end].rfind("/**")?;
    Some(&source[comment_start..comment_end])
}

fn is_injectable_type(type_name: &str) -> bool {
    !matches!(
        type_name,
        "Object"
            | "object"
            | "string"
            | "number"
            | "boolean"
            | "bigint"
            | "symbol"
            | "undefined"
            | "null"
            | "void"
            | "any"
            | "unknown"
    ) && type_name.bytes().enumerate().all(|(index, byte)| {
        if index == 0 {
            is_identifier_start(byte)
        } else {
            is_identifier_continue(byte)
        }
    })
}
