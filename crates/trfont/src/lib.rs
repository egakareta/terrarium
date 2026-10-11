#![deny(missing_docs)]
#![doc = include_str!("../README.md")]

#[cfg(feature = "default-fonts")]
use std::sync::Arc;

/// Name of the bundled proportional font.
pub const PROPORTIONAL_FONT_NAME: &str = "Outfit";
/// Name of the bundled monospace font.
pub const MONOSPACE_FONT_NAME: &str = "SUSEMono";

/// Returns egui font definitions, using the bundled fonts when `default-fonts` is enabled.
pub fn font_definitions() -> egui::FontDefinitions {
    #[cfg(feature = "default-fonts")]
    let mut fonts = egui::FontDefinitions::default();
    #[cfg(not(feature = "default-fonts"))]
    let fonts = egui::FontDefinitions::default();

    #[cfg(feature = "default-fonts")]
    {
        let mut tweak = egui::FontTweak::default();
        tweak.coords.push(b"wght", 400.0);

        fonts.font_data.insert(
            PROPORTIONAL_FONT_NAME.to_owned(),
            Arc::new(
                egui::FontData::from_static(include_bytes!("bin/Outfit-VariableFont_wght.ttf"))
                    .tweak(tweak.clone()),
            ),
        );
        fonts.font_data.insert(
            MONOSPACE_FONT_NAME.to_owned(),
            Arc::new(
                egui::FontData::from_static(include_bytes!("bin/SUSEMono-VariableFont_wght.ttf"))
                    .tweak(tweak),
            ),
        );
        fonts.families.insert(
            egui::FontFamily::Proportional,
            vec![PROPORTIONAL_FONT_NAME.to_owned()],
        );
        fonts.families.insert(
            egui::FontFamily::Monospace,
            vec![MONOSPACE_FONT_NAME.to_owned()],
        );
    }

    fonts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(feature = "default-fonts")]
    #[test]
    fn definitions_register_the_bundled_font_families() {
        let fonts = font_definitions();

        assert!(fonts.font_data.contains_key(PROPORTIONAL_FONT_NAME));
        assert!(fonts.font_data.contains_key(MONOSPACE_FONT_NAME));
        assert_eq!(
            fonts.families[&egui::FontFamily::Proportional],
            [PROPORTIONAL_FONT_NAME]
        );
        assert_eq!(
            fonts.families[&egui::FontFamily::Monospace],
            [MONOSPACE_FONT_NAME]
        );
    }

    #[cfg(not(feature = "default-fonts"))]
    #[test]
    fn definitions_use_egui_defaults_without_bundled_fonts() {
        let fonts = font_definitions();
        let defaults = egui::FontDefinitions::default();

        assert_eq!(fonts.families, defaults.families);
        assert!(!fonts.font_data.contains_key(PROPORTIONAL_FONT_NAME));
        assert!(!fonts.font_data.contains_key(MONOSPACE_FONT_NAME));
    }
}
