use std::{
    fs, io,
    path::{Path, PathBuf},
};

use rquickjs::Error;
use serde::{Deserialize, Serialize};
use web_time::Duration;

use super::JavaScriptResult;
#[cfg(not(target_arch = "wasm32"))]
use super::runtime::normalized_module_path;

pub(super) const SCRIPT_MANIFEST_FILE: &str = "manifest.json";

const SCRIPTING_JSCONFIG: &str = r#"{
  "compilerOptions": {
    "checkJs": false,
    "module": "ES2022",
    "moduleResolution": "Bundler",
    "target": "ES2022"
  },
  "include": ["**/*.js", "terrarium.d.ts"]
}
"#;

/// The editor declaration file installed by [`setup_script_project`].
pub const SCRIPTING_TYPES: &str = include_str!("api.d.ts");

/// Installs Terrarium JavaScript editor support in a consumer script directory.
///
/// The generated `terrarium.d.ts` supplies global declarations for `Instance`,
/// `time`, and `log`. The generated `jsconfig.json` makes those declarations
/// available to every JavaScript file below `directory`, without requiring
/// per-file reference directives. A sorted `scripts/manifest.json` is also
/// generated from JavaScript files below `directory/scripts`. Top-level files
/// declaring `export default class` are startup scripts; other `.js` files,
/// including files in subdirectories, are importable modules and are not
/// initialized independently.
pub fn setup_script_project(directory: impl AsRef<Path>) -> io::Result<()> {
    let directory = directory.as_ref();
    let declarations = directory.join("terrarium.d.ts");
    let config = directory.join("jsconfig.json");
    let scripts_directory = directory.join("scripts");
    fs::create_dir_all(directory)?;
    fs::create_dir_all(&scripts_directory)?;
    write_if_changed(&declarations, SCRIPTING_TYPES.as_bytes())?;
    write_if_changed(&config, SCRIPTING_JSCONFIG.as_bytes())?;
    write_script_manifest(&scripts_directory)?;
    Ok(())
}

#[derive(Debug, Deserialize, Serialize)]
pub(super) struct ScriptManifest {
    version: u32,
    pub(super) scripts: Vec<ScriptManifestEntry>,
    #[serde(default)]
    pub(super) modules: Vec<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub(super) struct ScriptManifestEntry {
    pub(super) name: String,
    pub(super) path: String,
}

fn write_script_manifest(directory: &Path) -> io::Result<()> {
    let manifest = script_entries(directory)?;
    let contents = serde_json::to_vec_pretty(&manifest).map_err(io::Error::other)?;
    write_if_changed(&directory.join(SCRIPT_MANIFEST_FILE), &contents)
}

fn write_if_changed(path: &Path, contents: &[u8]) -> io::Result<()> {
    match fs::read(path) {
        Ok(existing) if existing == contents => Ok(()),
        Ok(_) => fs::write(path, contents),
        Err(error) if error.kind() == io::ErrorKind::NotFound => fs::write(path, contents),
        Err(error) => Err(error),
    }
}

fn script_entries(directory: &Path) -> io::Result<ScriptManifest> {
    let mut scripts = Vec::new();
    let mut modules = Vec::new();
    collect_script_entries(directory, directory, &mut scripts, &mut modules)?;
    scripts.sort_by(|left, right| left.path.cmp(&right.path));
    modules.sort();
    Ok(ScriptManifest {
        version: 1,
        scripts,
        modules,
    })
}

fn collect_script_entries(
    root: &Path,
    directory: &Path,
    scripts: &mut Vec<ScriptManifestEntry>,
    modules: &mut Vec<String>,
) -> io::Result<()> {
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let path = entry.path();
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            collect_script_entries(root, &path, scripts, modules)?;
            continue;
        }
        if !file_type.is_file() || path.extension().and_then(|value| value.to_str()) != Some("js") {
            continue;
        }
        let relative_path = path
            .strip_prefix(root)
            .expect("scanned script belongs to its root")
            .to_str()
            .ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidData, "script filename is not UTF-8")
            })?
            .replace('\\', "/");
        let source = fs::read_to_string(&path)?;
        if directory == root && declares_default_class(&source) {
            let name = path
                .file_stem()
                .and_then(|value| value.to_str())
                .ok_or_else(|| {
                    io::Error::new(io::ErrorKind::InvalidData, "script filename is not UTF-8")
                })?;
            scripts.push(ScriptManifestEntry {
                name: name.to_owned(),
                path: relative_path,
            });
        } else {
            modules.push(relative_path);
        }
    }
    Ok(())
}

fn declares_default_class(source: &str) -> bool {
    let bytes = source.as_bytes();
    let mut position = 0;
    let mut matched = 0;
    while position < bytes.len() {
        let byte = bytes[position];
        if byte.is_ascii_whitespace() {
            position += 1;
            continue;
        }
        if bytes[position..].starts_with(b"//") {
            position += 2;
            while position < bytes.len() && bytes[position] != b'\n' {
                position += 1;
            }
            continue;
        }
        if bytes[position..].starts_with(b"/*") {
            position += 2;
            while position + 1 < bytes.len() && !bytes[position..].starts_with(b"*/") {
                position += 1;
            }
            position = (position + 2).min(bytes.len());
            continue;
        }
        if matches!(byte, b'\'' | b'"' | b'`') {
            matched = 0;
            position += 1;
            while position < bytes.len() {
                if bytes[position] == b'\\' {
                    position = (position + 2).min(bytes.len());
                } else if bytes[position] == byte {
                    position += 1;
                    break;
                } else {
                    position += 1;
                }
            }
            continue;
        }
        if byte.is_ascii_alphabetic() || matches!(byte, b'_' | b'$') {
            let start = position;
            position += 1;
            while position < bytes.len()
                && (bytes[position].is_ascii_alphanumeric()
                    || matches!(bytes[position], b'_' | b'$'))
            {
                position += 1;
            }
            matched = match (matched, &bytes[start..position]) {
                (0, b"export") => 1,
                (1, b"default") => 2,
                (2, b"class") => return true,
                _ => 0,
            };
        } else {
            matched = 0;
            position += 1;
        }
    }
    false
}

pub(super) fn parse_script_manifest(source: &str) -> JavaScriptResult<ScriptManifest> {
    let manifest: ScriptManifest = serde_json::from_str(source).map_err(|error| {
        Error::new_from_js_message("script manifest", "valid JSON", error.to_string())
    })?;
    if manifest.version != 1 {
        return Err(Error::new_from_js_message(
            "script manifest version",
            "version 1",
            manifest.version.to_string(),
        ));
    }
    Ok(manifest)
}

/// Resolves a script name to native and web locations.
#[derive(Clone, Debug, Default)]
pub struct ScriptDirectories {
    native_directory: Option<PathBuf>,
    web_directory: Option<String>,
}

pub(crate) struct DiscoveredScripts {
    pub(crate) entries: Vec<ScriptOptions>,
    pub(crate) sources: Vec<ScriptOptions>,
}

impl ScriptDirectories {
    /// Creates an empty set of script directories.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the directory used to load scripts on native targets.
    pub fn with_native_directory(mut self, directory: impl Into<PathBuf>) -> Self {
        self.native_directory = Some(directory.into());
        self
    }

    /// Sets the URL prefix used to load scripts on web targets.
    pub fn with_web_directory(mut self, directory: impl Into<String>) -> Self {
        self.web_directory = Some(directory.into());
        self
    }

    /// Creates options for a file relative to these directories.
    pub fn options(&self, file_name: impl AsRef<Path>) -> ScriptOptions {
        let file_name = file_name.as_ref();
        let name = file_name
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or("script")
            .to_owned();
        let native_file = self
            .native_directory
            .as_ref()
            .map(|directory| directory.join(file_name));
        let web_url = self.web_directory.as_ref().map(|directory| {
            let directory = directory.trim_end_matches('/');
            let file_name = file_name.to_string_lossy().replace('\\', "/");
            format!("{directory}/{file_name}")
        });

        ScriptOptions {
            name: Some(name),
            source: None,
            native_file,
            web_url,
            watch: true,
            poll_interval: Duration::from_millis(250),
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn native_options(&self) -> JavaScriptResult<DiscoveredScripts> {
        let directory = self.native_directory.as_ref().ok_or_else(|| {
            Error::new_from_js_message(
                "script directory",
                "a native directory",
                "none was configured",
            )
        })?;
        let manifest_path = directory.join(SCRIPT_MANIFEST_FILE);
        let entries = match fs::read_to_string(&manifest_path) {
            Ok(source) => parse_script_manifest(&source)?.scripts,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                script_entries(directory)
                    .map_err(|error| {
                        Error::new_from_js_message(
                            "script directory",
                            "readable directory",
                            format!("{}: {error}", directory.display()),
                        )
                    })?
                    .scripts
            }
            Err(error) => {
                return Err(Error::new_from_js_message(
                    "script manifest",
                    "readable file",
                    format!("{}: {error}", manifest_path.display()),
                ));
            }
        };

        Ok(DiscoveredScripts {
            entries: entries
                .into_iter()
                .map(|entry| self.options(entry.path).with_name(entry.name))
                .collect(),
            sources: Vec::new(),
        })
    }

    #[cfg(target_arch = "wasm32")]
    pub(crate) async fn web_options(&self) -> JavaScriptResult<DiscoveredScripts> {
        let manifest_url = self.web_url(SCRIPT_MANIFEST_FILE)?;
        let source = fetch_text(&manifest_url, "script manifest").await?;
        let manifest = parse_script_manifest(&source)?;
        let mut entries = Vec::with_capacity(manifest.scripts.len());
        let mut sources = Vec::with_capacity(manifest.scripts.len() + manifest.modules.len());

        for path in manifest.modules {
            let url = self.web_url(&path)?;
            let source = fetch_text(&url, &format!("script module `{path}`")).await?;
            sources.push(self.options(&path).with_source(source).with_web_url(url));
        }

        for entry in manifest.scripts {
            let url = self.web_url(&entry.path)?;
            let source = fetch_text(&url, &format!("script `{}`", entry.name)).await?;
            let option = self
                .options(&entry.path)
                .with_name(entry.name)
                .with_source(source)
                .with_web_url(url);
            sources.push(option.clone());
            entries.push(option);
        }

        Ok(DiscoveredScripts { entries, sources })
    }

    #[cfg(target_arch = "wasm32")]
    fn web_url(&self, file_name: impl AsRef<Path>) -> JavaScriptResult<String> {
        let directory = self.web_directory.as_ref().ok_or_else(|| {
            Error::new_from_js_message("script directory", "a web directory", "none was configured")
        })?;
        let directory = directory.trim_end_matches('/');
        let file_name = file_name.as_ref().to_string_lossy().replace('\\', "/");
        if directory.is_empty() {
            Ok(file_name)
        } else {
            Ok(format!("{directory}/{file_name}"))
        }
    }
}

#[cfg(target_arch = "wasm32")]
pub(super) async fn fetch_text(url: &str, resource: &str) -> JavaScriptResult<String> {
    use wasm_bindgen::JsCast as _;
    use wasm_bindgen_futures::JsFuture;

    let window = web_sys::window().ok_or_else(|| {
        Error::new_from_js_message(
            "script resource",
            "browser window",
            format!("{resource}: window is unavailable"),
        )
    })?;
    let init = web_sys::RequestInit::new();
    init.set_cache(web_sys::RequestCache::NoStore);
    let request = web_sys::Request::new_with_str_and_init(url, &init).map_err(|error| {
        Error::new_from_js_message(
            "script resource",
            "valid request",
            format!("{resource}: {error:?}"),
        )
    })?;
    let response = JsFuture::from(window.fetch_with_request(&request))
        .await
        .map_err(|error| {
            Error::new_from_js_message(
                "script resource",
                "successful request",
                format!("{resource}: {error:?}"),
            )
        })?
        .dyn_into::<web_sys::Response>()
        .map_err(|error| {
            Error::new_from_js_message(
                "script resource",
                "HTTP response",
                format!("{resource}: {error:?}"),
            )
        })?;
    if !response.ok() {
        return Err(Error::new_from_js_message(
            "script resource",
            "successful HTTP response",
            format!("{resource}: {url} returned HTTP {}", response.status()),
        ));
    }
    let text = JsFuture::from(response.text().map_err(|error| {
        Error::new_from_js_message(
            "script resource",
            "readable response",
            format!("{resource}: {error:?}"),
        )
    })?)
    .await
    .map_err(|error| {
        Error::new_from_js_message(
            "script resource",
            "text response",
            format!("{resource}: {error:?}"),
        )
    })?;
    text.as_string().ok_or_else(|| {
        Error::new_from_js_message(
            "script resource",
            "text response",
            format!("{resource}: response was not a string"),
        )
    })
}

/// Describes one JavaScript module source and its optional reload locations.
#[derive(Clone, Debug)]
pub struct ScriptOptions {
    pub(super) name: Option<String>,
    pub(super) source: Option<String>,
    pub(super) native_file: Option<PathBuf>,
    pub(super) web_url: Option<String>,
    pub(super) watch: bool,
    pub(super) poll_interval: Duration,
}

impl ScriptOptions {
    /// Creates options for a named inline module.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: Some(name.into()),
            source: None,
            native_file: None,
            web_url: None,
            watch: false,
            poll_interval: Duration::from_millis(250),
        }
    }

    /// Creates options for an inline module.
    pub fn inline(name: impl Into<String>, source: impl Into<String>) -> Self {
        Self::new(name).with_source(source)
    }

    /// Overrides the module identity.
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    /// Sets inline module source.
    pub fn with_source(mut self, source: impl Into<String>) -> Self {
        self.source = Some(source.into());
        self
    }

    /// Sets the native file used as the module source.
    pub fn with_native_file(mut self, path: impl Into<PathBuf>) -> Self {
        self.native_file = Some(path.into());
        self
    }

    /// Sets the web URL used to resolve relative imports and poll watched source.
    ///
    /// The synchronous module loader does not fetch URLs. On web, supply source
    /// with [`Self::with_source`] (the directory-based startup loader fetches it).
    /// Watched modules are fetched again asynchronously during updates.
    pub fn with_web_url(mut self, url: impl Into<String>) -> Self {
        self.web_url = Some(url.into());
        self
    }

    /// Enables or disables source polling for reloads (native files or web URLs).
    pub fn with_watch(mut self, watch: bool) -> Self {
        self.watch = watch;
        self
    }

    /// Sets the source polling interval.
    pub fn with_poll_interval(mut self, interval: Duration) -> Self {
        self.poll_interval = interval;
        self
    }

    pub(crate) fn identity(&self) -> String {
        self.name
            .clone()
            .or_else(|| {
                self.native_file
                    .as_ref()
                    .map(|path| path.to_string_lossy().into_owned())
            })
            .or_else(|| self.web_url.clone())
            .unwrap_or_else(|| "script".to_owned())
    }

    pub(super) fn module_path(&self) -> String {
        #[cfg(target_arch = "wasm32")]
        let path = self.web_url.clone();
        #[cfg(not(target_arch = "wasm32"))]
        let path = self
            .native_file
            .as_ref()
            .map(|path| path.to_string_lossy().into_owned());
        let path = path.unwrap_or_else(|| self.identity());
        #[cfg(not(target_arch = "wasm32"))]
        if self.native_file.is_some() && !Path::new(&path).is_absolute() {
            return std::env::current_dir()
                .map(|directory| normalized_module_path(&directory.join(&path)))
                .unwrap_or(path);
        }
        path
    }

    pub(super) fn load_source(&self) -> JavaScriptResult<String> {
        if let Some(source) = &self.source {
            return Ok(source.clone());
        }
        if let Some(path) = &self.native_file {
            return fs::read_to_string(path).map_err(|error| {
                Error::new_from_js_message("script file", "string", error.to_string())
            });
        }
        Err(Error::new_from_js_message(
            "script source",
            "string",
            "no inline source or native file was provided",
        ))
    }
}
