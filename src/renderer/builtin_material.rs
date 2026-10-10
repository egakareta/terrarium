use super::material::{Texture, TextureColorSpace, TextureError};

/// A material texture set shipped with Terrarium.
///
/// Use a material directly with [`Material::builtin`](crate::Material::builtin).
/// Built-in material maps use world-space triplanar projection.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[repr(u8)]
pub enum BuiltinMaterial {
    /// Asphalt surface.
    Asphalt,
    /// Basalt surface.
    Basalt,
    /// Brick surface.
    Brick,
    /// Cobblestone surface.
    Cobblestone,
    /// Concrete surface.
    Concrete,
    /// Corroded metal surface.
    CorrodedMetal,
    /// Cracked lava surface.
    CrackedLava,
    /// Diamond plate metal surface.
    DiamondPlate,
    /// Fabric surface.
    Fabric,
    /// Foil metal surface.
    Foil,
    /// Glacier ice surface.
    Glacier,
    /// Granite surface.
    Granite,
    /// Grass surface.
    Grass,
    /// Ground surface.
    Ground,
    /// Ice surface.
    Ice,
    /// Leafy grass surface.
    LeafyGrass,
    /// Limestone surface.
    Limestone,
    /// Marble surface.
    Marble,
    /// Metal surface.
    Metal,
    /// Mud surface.
    Mud,
    /// Pavement surface.
    Pavement,
    /// Pebble surface.
    Pebble,
    /// Plastic surface.
    Plastic,
    /// Rock surface.
    Rock,
    /// Salt surface.
    Salt,
    /// Sand surface.
    Sand,
    /// Sandstone surface.
    Sandstone,
    /// Slate surface.
    Slate,
    /// Snow surface.
    Snow,
    /// Wood surface.
    Wood,
    /// Wood planks surface.
    WoodPlanks,
}

impl BuiltinMaterial {
    /// Every built-in material in its stable declaration order.
    pub const ALL: [Self; 31] = [
        Self::Asphalt,
        Self::Basalt,
        Self::Brick,
        Self::Cobblestone,
        Self::Concrete,
        Self::CorrodedMetal,
        Self::CrackedLava,
        Self::DiamondPlate,
        Self::Fabric,
        Self::Foil,
        Self::Glacier,
        Self::Granite,
        Self::Grass,
        Self::Ground,
        Self::Ice,
        Self::LeafyGrass,
        Self::Limestone,
        Self::Marble,
        Self::Metal,
        Self::Mud,
        Self::Pavement,
        Self::Pebble,
        Self::Plastic,
        Self::Rock,
        Self::Salt,
        Self::Sand,
        Self::Sandstone,
        Self::Slate,
        Self::Snow,
        Self::Wood,
        Self::WoodPlanks,
    ];

    pub(crate) const COUNT: usize = Self::ALL.len();

    /// Returns the lowercase asset name for this material.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Asphalt => "asphalt",
            Self::Basalt => "basalt",
            Self::Brick => "brick",
            Self::Cobblestone => "cobblestone",
            Self::Concrete => "concrete",
            Self::CorrodedMetal => "corrodedmetal",
            Self::CrackedLava => "crackedlava",
            Self::DiamondPlate => "diamondplate",
            Self::Fabric => "fabric",
            Self::Foil => "foil",
            Self::Glacier => "glacier",
            Self::Granite => "granite",
            Self::Grass => "grass",
            Self::Ground => "ground",
            Self::Ice => "ice",
            Self::LeafyGrass => "leafygrass",
            Self::Limestone => "limestone",
            Self::Marble => "marble",
            Self::Metal => "metal",
            Self::Mud => "mud",
            Self::Pavement => "pavement",
            Self::Pebble => "pebble",
            Self::Plastic => "plastic",
            Self::Rock => "rock",
            Self::Salt => "salt",
            Self::Sand => "sand",
            Self::Sandstone => "sandstone",
            Self::Slate => "slate",
            Self::Snow => "snow",
            Self::Wood => "wood",
            Self::WoodPlanks => "woodplanks",
        }
    }

    pub(crate) const fn metallic(self) -> f32 {
        match self {
            Self::CorrodedMetal => 0.8,
            Self::DiamondPlate | Self::Foil => 0.9,
            Self::Metal => 1.0,
            _ => 0.0,
        }
    }

    pub(crate) const fn fallback_roughness(self) -> f32 {
        match self {
            Self::Ice | Self::Glacier => 0.18,
            Self::Plastic => 0.35,
            Self::Metal | Self::DiamondPlate | Self::Foil => 0.4,
            Self::CorrodedMetal => 0.75,
            Self::Grass | Self::Sand | Self::Snow => 0.9,
            _ => 0.8,
        }
    }

    pub(crate) fn assets(self) -> BuiltinMaterialAssets {
        macro_rules! material_map {
            ($name:literal, $map:literal) => {{
                include_bytes!(concat!(
                    "../../assets/material/",
                    $name,
                    "_",
                    $map,
                    ".basis"
                )) as &'static [u8]
            }};
        }
        macro_rules! all_maps {
            ($name:literal) => {
                BuiltinMaterialAssets {
                    base_color: material_map!($name, "color"),
                    normal: Some(material_map!($name, "normal")),
                    roughness: Some(material_map!($name, "rough")),
                }
            };
        }
        macro_rules! color_and_roughness {
            ($name:literal) => {
                BuiltinMaterialAssets {
                    base_color: material_map!($name, "color"),
                    normal: None,
                    roughness: Some(material_map!($name, "rough")),
                }
            };
        }
        macro_rules! color_and_normal {
            ($name:literal) => {
                BuiltinMaterialAssets {
                    base_color: material_map!($name, "color"),
                    normal: Some(material_map!($name, "normal")),
                    roughness: None,
                }
            };
        }

        match self {
            Self::Asphalt => all_maps!("asphalt"),
            Self::Basalt => all_maps!("basalt"),
            Self::Brick => all_maps!("brick"),
            Self::Cobblestone => all_maps!("cobblestone"),
            Self::Concrete => all_maps!("concrete"),
            Self::CorrodedMetal => all_maps!("corrodedmetal"),
            Self::CrackedLava => all_maps!("crackedlava"),
            Self::DiamondPlate => all_maps!("diamondplate"),
            Self::Fabric => all_maps!("fabric"),
            Self::Foil => all_maps!("foil"),
            Self::Glacier => all_maps!("glacier"),
            Self::Granite => all_maps!("granite"),
            Self::Grass => color_and_roughness!("grass"),
            Self::Ground => all_maps!("ground"),
            Self::Ice => color_and_normal!("ice"),
            Self::LeafyGrass => color_and_normal!("leafygrass"),
            Self::Limestone => all_maps!("limestone"),
            Self::Marble => all_maps!("marble"),
            Self::Metal => all_maps!("metal"),
            Self::Mud => all_maps!("mud"),
            Self::Pavement => all_maps!("pavement"),
            Self::Pebble => all_maps!("pebble"),
            Self::Plastic => color_and_normal!("plastic"),
            Self::Rock => all_maps!("rock"),
            Self::Salt => color_and_normal!("salt"),
            Self::Sand => color_and_normal!("sand"),
            Self::Sandstone => all_maps!("sandstone"),
            Self::Slate => color_and_normal!("slate"),
            Self::Snow => all_maps!("snow"),
            Self::Wood => all_maps!("wood"),
            Self::WoodPlanks => all_maps!("woodplanks"),
        }
    }

    pub(crate) fn decode(self) -> Result<DecodedBuiltinMaterial, TextureError> {
        let assets = self.assets();
        let base_color = decode_builtin_material(assets.base_color, TextureColorSpace::Srgb)?;
        let normal = assets
            .normal
            .map(|bytes| decode_builtin_material(bytes, TextureColorSpace::Linear))
            .transpose()?;
        let metallic_roughness = assets
            .roughness
            .map(|bytes| {
                let roughness = decode_builtin_material(bytes, TextureColorSpace::Linear)?;
                let mut pixels = Vec::with_capacity(roughness.pixels.len());
                for pixel in roughness.pixels.chunks_exact(4) {
                    pixels.extend_from_slice(&[255, pixel[0], 255, 255]);
                }
                Texture::linear(roughness.width, roughness.height, pixels)
            })
            .transpose()?;

        Ok(DecodedBuiltinMaterial {
            base_color,
            normal,
            metallic_roughness,
        })
    }
}

fn decode_builtin_material(
    bytes: &[u8],
    color_space: TextureColorSpace,
) -> Result<Texture, TextureError> {
    use basisu::{DecodeFlags, TargetFormat, Transcoder};

    let transcoder = Transcoder::new(bytes).map_err(|_| TextureError::BasisDecode)?;
    if transcoder.layer_count() != 1 {
        return Err(TextureError::BasisDecode);
    }
    let (width, height) = transcoder.base_dimensions();
    let pixels = transcoder.transcode(0, TargetFormat::Rgba32, DecodeFlags::NONE);
    Texture::with_color_space(
        width,
        height,
        pixels.map_err(|_| TextureError::BasisDecode)?,
        color_space,
    )
}

pub(crate) struct DecodedBuiltinMaterial {
    pub(crate) base_color: Texture,
    pub(crate) normal: Option<Texture>,
    pub(crate) metallic_roughness: Option<Texture>,
}

pub(crate) struct BuiltinMaterialAssets {
    pub(crate) base_color: &'static [u8],
    pub(crate) normal: Option<&'static [u8]>,
    pub(crate) roughness: Option<&'static [u8]>,
}
