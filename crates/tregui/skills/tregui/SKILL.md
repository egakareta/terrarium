---
name: tregui
description: Guide downstream Rust applications that depend on the tregui crate to select opt-in features and use its egui widget re-exports. Use when adding tregui, integrating egui_ltreeview, egui-phosphor, egui-text-outline, or egui_taffy, configuring icon fonts, or resolving egui feature and version conflicts.
---

# Tregui Consumer Guide

## Scope

Use this guide from the application repository that depends on `tregui`. Treat `tregui` as a normal Cargo dependency; do not assume access to Terrarium's repository, workspace layout, or source files. Enable only the features the application needs and use the public re-exports.

## Add the dependency

```toml
[dependencies]
tregui = { version = "0.1", default-features = false, features = ["ltreeview", "phosphor"] }
```

`tregui::egui` re-exports the egui version used by its widgets. Keep the application's `egui`/`eframe` dependency compatible with that version; duplicate egui versions produce incompatible `Ui`, `Context`, and widget types. Check `cargo tree -d` when a widget reports an unexpected type mismatch.

| Feature           | Public API                       | Use                                                                 |
| ----------------- | -------------------------------- | ------------------------------------------------------------------- |
| `ltreeview`       | `tregui::egui_ltreeview`         | Hierarchical trees, selection, and keyboard navigation              |
| `phosphor`        | `tregui::egui_phosphor`          | Phosphor glyphs and font registration                               |
| `phosphor-subset` | `tregui::egui_phosphor::subset!` | Embed only the icon glyphs the application uses; enables `phosphor` |
| `text-outline`    | `tregui::egui_text_outline`      | Outlined text widgets                                               |
| `taffy`           | `tregui::egui_taffy`             | Flexbox, grid, and block layouts                                    |
| `all`             | All base widget APIs             | Experiments only; add `phosphor-subset` separately when needed      |

## Use the widgets

### Build a selectable tree

Use stable, unique node IDs and close every opened directory after its descendants. Process actions to update application state:

```rust
use tregui::egui_ltreeview::{Action, TreeView};

let (_, actions) = TreeView::new(ui.make_persistent_id("scene-tree"))
    .allow_multi_selection(true)
    .show(ui, |tree| {
        tree.dir(0, "Scene");
        tree.leaf(1, "Camera");
        tree.leaf(2, "Light");
        tree.close_dir();
    });

for action in actions {
    if let Action::SetSelected(ids) = action {
        self.selected_ids = ids;
    }
}
```

Keep `selected_ids` in the application's state rather than recreating it each frame.

For custom node labels or icons, construct nodes with `NodeBuilder`. Use `TreeViewState` with `show_state` when state must be owned or persisted by the application.

### Register and display Phosphor icons

Register the font during application setup, not during each frame. Add it to the same `FontDefinitions` that contains the application's other fonts, then call `set_fonts` once. Do not replace custom fonts with a fresh default definition just to add icons.

```rust
use tregui::egui;

let mut fonts = egui::FontDefinitions::default();
// Add the application's custom fonts to this same definition before setting it.
tregui::egui_phosphor::add_to_fonts(
    &mut fonts,
    tregui::egui_phosphor::Variant::Regular,
);
creation_context.egui_ctx.set_fonts(fonts);

ui.label(format!("{} Settings", tregui::egui_phosphor::regular::GEAR));
```

Use glyph constants from the same variant that was registered. For smaller icon fonts, enable `phosphor-subset` and define a subset with the re-exported `subset!` macro.

### Outline a label

```rust
use tregui::egui;
use tregui::egui_text_outline::OutlinedLabel;

ui.add(
    OutlinedLabel::new("Outlined label")
        .text_color(egui::Color32::WHITE)
        .outline(egui::Stroke::new(2.0, egui::Color32::BLACK)),
);
```

### Create a Taffy layout

Give each layout a stable, unique egui ID. Set the root display mode explicitly and use child styles to express flex or grid behavior:

```rust
use tregui::egui_taffy::{taffy, tui, TuiBuilderLogic};
use taffy::prelude::length;

tui(ui, ui.id().with("main-layout"))
    .reserve_available_space()
    .style(taffy::Style {
        display: taffy::Display::Flex,
        flex_direction: taffy::FlexDirection::Row,
        gap: length(8.0),
        ..Default::default()
    })
    .show(|tui| {
        tui.add_with_border(|tui| {
            tui.label("Sidebar");
        });
        tui.style(taffy::Style {
            flex_grow: 1.0,
            ..Default::default()
        })
        .add(|tui| {
            tui.label("Content");
        });
    });
```

The `egui_taffy` 0.14 release requires Rust 1.95. Enable `taffy` only when the consumer's toolchain supports that requirement.

## Validate in the consuming application

Check the feature set selected by the application with `cargo check --all-targets` and its normal tests and lints. Run an interactive host application to validate input, font registration, and layout behavior. Consult the API documentation for the versions resolved by the application's lockfile when using options not covered here; do not rely on Terrarium workspace-only example commands.
