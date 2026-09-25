#[cfg(not(target_arch = "wasm32"))]
use std::fs;
use std::{
    collections::HashMap,
    path::{Component, Path, PathBuf},
    sync::{Arc, Mutex},
};

use rquickjs::{
    Context, Ctx, Error, Function, Module, Object, Persistent, Runtime,
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
            return Module::declare(ctx.clone(), name, source);
        }
        #[cfg(not(target_arch = "wasm32"))]
        if Path::new(name).is_absolute() {
            let source = fs::read_to_string(name)
                .map_err(|error| Error::new_loading_message(name, error.to_string()))?;
            return Module::declare(ctx.clone(), name, source);
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

pub(super) struct BuiltModule {
    pub(super) instance: Persistent<Object<'static>>,
}

pub(super) fn build_module(
    runtime: &JavaScriptRuntime,
    name: &str,
    source: &str,
) -> JavaScriptResult<BuiltModule> {
    runtime.with_context(|ctx| {
        let module = Module::declare(ctx.clone(), name, source)?;
        let (module, promise) = module.eval()?;
        // Run jobs scheduled by the module before lifecycle hooks commit scene commands.
        while ctx.execute_pending_job() {}
        promise.finish::<()>()?;
        let namespace = module.namespace()?;
        let instance = construct_class(&ctx, &namespace, source, "default", &mut HashMap::new())?;
        Ok(BuiltModule {
            instance: Persistent::save(&ctx, instance),
        })
    })
}

fn construct_class<'js>(
    ctx: &Ctx<'js>,
    namespace: &Object<'js>,
    source: &str,
    class_name: &str,
    cache: &mut HashMap<String, Object<'js>>,
) -> JavaScriptResult<Object<'js>> {
    if let Some(instance) = cache.get(class_name) {
        return Ok(instance.clone());
    }
    let constructor: Function = namespace
        .get(class_name)
        .map_err(|_| Error::new_from_js_message("constructor", "exported class", class_name))?;
    let dependencies = constructor_dependencies(source, class_name);
    let mut values = Vec::with_capacity(dependencies.len());
    for dependency in dependencies {
        values.push(construct_class(ctx, namespace, source, &dependency, cache)?);
    }
    let construct: Function = ctx.globals().get("__internal_construct")?;
    let instance: Object = construct.call((constructor, values))?;
    cache.insert(class_name.to_owned(), instance.clone());
    Ok(instance)
}

fn constructor_dependencies(source: &str, class_name: &str) -> Vec<String> {
    let class_start = if class_name == "default" {
        source.find("export default class")
    } else {
        source.find(&format!("class {class_name}"))
    };
    let Some(class_start) = class_start else {
        return Vec::new();
    };
    let rest = &source[class_start..];
    let Some(constructor_start) = rest.find("constructor(") else {
        return Vec::new();
    };
    let before = &rest[..constructor_start];
    let Some(comment_start) = before.rfind("/**") else {
        return Vec::new();
    };
    let comment = &before[comment_start..];
    comment
        .lines()
        .filter_map(|line| {
            let marker = line.find("@param")?;
            let annotation = line[marker + "@param".len()..].trim();
            let start = annotation.find('{')? + 1;
            let end = annotation[start..].find('}')? + start;
            let dependency = annotation[start..end].trim();
            (!dependency.is_empty() && dependency != "Object" && dependency != "string")
                .then(|| dependency.to_owned())
        })
        .collect()
}
