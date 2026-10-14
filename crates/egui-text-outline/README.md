# egui-text-outline

Outlined text labels for egui. The widget uses egui's text layout and draws a
cached outline behind the original text.

```rust
fn show(ui: &mut egui::Ui) {
    ui.add(
        egui_text_outline::OutlinedLabel::new("Outlined text")
            .text_color(egui::Color32::WHITE)
            .outline(egui::Stroke::new(1.5, egui::Color32::BLACK)),
    );
}
```

The outline width is measured outward from the glyphs in egui points. The font
defaults to the body text style; use `.font(...)` to select another `FontId`.
