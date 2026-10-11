# tregui

`tregui` centralizes optional egui crates used by Terrarium. It re-exports the
shared egui version and gates each curated crate behind an opt-in feature:

| Feature           | Re-export                      | Purpose                              |
| ----------------- | ------------------------------ | ------------------------------------ |
| `ltreeview`       | `egui_ltreeview`               | Hierarchical, selectable trees       |
| `phosphor`        | `egui_phosphor`                | Phosphor icons and fonts             |
| `phosphor-subset` | `egui_phosphor` subset support | Compile-time icon font subsetting    |
| `text-outline`    | `egui_text_outline`            | Outlined text widgets                |
| `taffy`           | `egui_taffy`                   | Flexbox, grid, and block layouts     |
| `all`             | All curated crates             | Convenience feature for the full set |

Consumers should disable default features and enable only what they use:

```toml
tregui = { version = "0.1", default-features = false, features = ["ltreeview", "phosphor"] }
```

Run an example with its feature, for example `cargo run -p tregui --example
text_outline_basic --features text-outline`. The examples directory contains
runnable demonstrations for each crate and common usage patterns.
