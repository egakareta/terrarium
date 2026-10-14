#![deny(missing_docs)]
#![doc = include_str!("../README.md")]

/// Name of the bundled proportional font.
pub const PROPORTIONAL_FONT_NAME: &str = "Outfit";
/// Name of the bundled monospace font.
pub const MONOSPACE_FONT_NAME: &str = "SUSEMono";
/// Bundled proportional font bytes for host font registration.
#[cfg(feature = "default-fonts")]
pub const PROPORTIONAL_FONT: &[u8] = include_bytes!("bin/Outfit-VariableFont_wght.ttf");
/// Bundled monospace font bytes for host font registration.
#[cfg(feature = "default-fonts")]
pub const MONOSPACE_FONT: &[u8] = include_bytes!("bin/SUSEMono-VariableFont_wght.ttf");

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
        let mut monospace_tweak = tweak.clone();
        monospace_tweak.y_offset_factor = 0.05;

        fonts.font_data.insert(
            PROPORTIONAL_FONT_NAME.to_owned(),
            std::sync::Arc::new(
                egui::FontData::from_static(include_bytes!("bin/Outfit-VariableFont_wght.ttf"))
                    .tweak(tweak.clone()),
            ),
        );
        fonts.font_data.insert(
            MONOSPACE_FONT_NAME.to_owned(),
            std::sync::Arc::new(
                egui::FontData::from_static(include_bytes!("bin/SUSEMono-VariableFont_wght.ttf"))
                    .tweak(monospace_tweak),
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
