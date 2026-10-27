import html
import json
import os
import shutil
import subprocess
from pathlib import Path
from urllib.parse import quote


DESCRIPTIONS = {
    "app": "the main example",
    "gltf": "a custom 3d model",
    "outlines": "outline a mesh easily",
    "physics": "realistic physics with rapier",
    "simple": "the smallest terrarium scene: one cube",
    "viewport": "render scenes into textures and display them in egui",
}


def example_card(name):
    title = html.escape(name)
    url = quote(name, safe="") + "/"
    description = DESCRIPTIONS.get(name)
    paragraph = f"\n          <p>{html.escape(description)}</p>" if description else ""
    return f"""      <article class="example">
        <div class="example-info">
          <h2>{title}</h2>{paragraph}
          <a href="{url}" data-example="{title}" aria-haspopup="dialog" aria-controls="example-modal"
            >open {title}</a
          >
        </div>
      </article>"""


def main():
    root = Path(__file__).resolve().parent.parent
    metadata = json.loads(
        subprocess.check_output(
            ["cargo", "metadata", "--no-deps", "--format-version", "1"],
            cwd=root,
            text=True,
        )
    )
    package = next(
        package
        for package in metadata["packages"]
        if Path(package["manifest_path"]) == root / "Cargo.toml"
    )
    examples = sorted(
        target["name"]
        for target in package["targets"]
        if "example" in target["kind"] and "bin" in target["crate_types"]
    )
    dist = root / "examples/dist"
    dist.mkdir(parents=True, exist_ok=True)
    base_path = os.environ.get("PAGES_BASE_PATH", "").rstrip("/")
    for example in examples:
        subprocess.run(
            [
                "cargo",
                "build",
                "--release",
                "--locked",
                "--all-features",
                "--package",
                package["name"],
                "--target",
                "wasm32-unknown-unknown",
                "--example",
                example,
                "--timings",
            ],
            cwd=root,
            check=True,
        )
        timing_dir = dist / ".timings" / example
        timing_dir.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(
            Path(metadata["target_directory"]) / "cargo-timings/cargo-timing.html",
            timing_dir / "index.html",
        )
        subprocess.run(
            [
                "trunk",
                "build",
                "--release",
                "--locked",
                "--all-features",
                "--example",
                example,
                "--dist",
                str(dist / example),
                "--public-url",
                f"{base_path}/{quote(example, safe='')}/",
            ],
            cwd=root,
            check=True,
        )

    template = (root / "assets/web/examples.html").read_text(encoding="utf-8")
    marker = "      <!-- examples -->"
    if template.count(marker) != 1:
        raise ValueError("Expected one examples placeholder in the gallery template")
    gallery = template.replace(marker, "\n".join(map(example_card, examples)))
    (dist / "index.html").write_text(gallery, encoding="utf-8")
    shutil.copyfile(root / "assets/favicon.svg", dist / "favicon.svg")


if __name__ == "__main__":
    main()
