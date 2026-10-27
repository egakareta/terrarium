---
[package]
edition = "2024"

[dependencies]
serde_json = "1"
---

use std::{
    env, fs, io,
    path::Path,
    process::{Command, Stdio},
};

fn run(command: &mut Command) -> io::Result<()> {
    let status = command
        .env("CARGO_UNSTABLE_BUILD_STD", "std,panic_abort")
        .status()?;
    if !status.success() {
        return Err(io::Error::other(format!("{command:?} failed: {status}")));
    }
    Ok(())
}

fn example_card(name: &str) -> String {
    let description = match name {
        "app" => "the main example",
        "gltf" => "a custom 3d model",
        "outlines" => "outline a mesh easily",
        "physics" => "realistic physics with rapier",
        "simple" => "the smallest terrarium scene: one cube",
        "viewport" => "render scenes into textures and display them in egui",
        _ => "",
    };
    let paragraph = if description.is_empty() {
        String::new()
    } else {
        format!("\n          <p>{description}</p>")
    };
    let label = name
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;");
    format!(
        r#"      <article class="example">
        <div class="example-info">
          <h2>{label}</h2>{paragraph}
          <a href="{label}/" data-example="{label}" aria-haspopup="dialog" aria-controls="example-modal"
            >open {label}</a
          >
        </div>
      </article>"#
    )
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    env::set_current_dir(root)?;
    let output = Command::new("cargo")
        .args(["metadata", "--no-deps", "--format-version", "1"])
        .stderr(Stdio::inherit())
        .output()?;
    if !output.status.success() {
        return Err("cargo metadata failed".into());
    }
    let metadata: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    let manifest = root.join("Cargo.toml");
    let package = metadata["packages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|package| package["manifest_path"].as_str() == manifest.to_str())
        .ok_or("root package not found in Cargo metadata")?;
    let mut examples: Vec<_> = package["targets"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|target| {
            target["kind"] == serde_json::json!(["example"])
                && target["crate_types"] == serde_json::json!(["bin"])
        })
        .map(|target| target["name"].as_str().unwrap())
        .collect();
    examples.sort_unstable();

    let template = fs::read_to_string("assets/web/examples.html")?;
    let marker = "      <!-- examples -->";
    if template.matches(marker).count() != 1 {
        return Err("expected one examples placeholder in the gallery template".into());
    }
    let dist = root.join("examples/dist");
    fs::create_dir_all(&dist)?;
    let base_path = env::var("PAGES_BASE_PATH").unwrap_or_default();
    let timing_report = Path::new(metadata["target_directory"].as_str().unwrap())
        .join("cargo-timings/cargo-timing.html");
    let mut cards = Vec::new();
    for example in examples {
        run(Command::new("cargo").args([
            "build",
            "--release",
            "--locked",
            "--all-features",
            "--target",
            "wasm32-unknown-unknown",
            "--example",
            example,
            "--timings",
        ]))?;
        let timing_dir = dist.join(".timings").join(example);
        fs::create_dir_all(&timing_dir)?;
        fs::copy(&timing_report, timing_dir.join("index.html"))?;
        run(Command::new("trunk")
            .args([
                "build",
                "--release",
                "--locked",
                "--all-features",
                "--example",
                example,
            ])
            .arg("--dist")
            .arg(dist.join(example))
            .arg("--public-url")
            .arg(format!("{}/{example}/", base_path.trim_end_matches('/'))))?;
        cards.push(example_card(example));
    }
    fs::write(
        dist.join("index.html"),
        template.replace(marker, &cards.join("\n")),
    )?;
    fs::copy("assets/favicon.svg", dist.join("favicon.svg"))?;
    Ok(())
}
