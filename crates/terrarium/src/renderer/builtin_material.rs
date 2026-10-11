use super::material::{Texture, TextureColorSpace, TextureError};

macro_rules! material_map {
    ($name:ident, $map:literal) => {
        include_bytes!(concat!(
            "../../assets/material/",
            stringify!($name),
            "_",
            $map,
            ".basis"
        )) as &'static [u8]
    };
}

macro_rules! material_assets {
    (all_maps, $name:ident) => {
        BuiltinMaterialAssets {
            base_color: material_map!($name, "color"),
            normal: Some(material_map!($name, "normal")),
            roughness: Some(material_map!($name, "rough")),
        }
    };
    (color_and_roughness, $name:ident) => {
        BuiltinMaterialAssets {
            base_color: material_map!($name, "color"),
            normal: None,
            roughness: Some(material_map!($name, "rough")),
        }
    };
    (color_and_normal, $name:ident) => {
        BuiltinMaterialAssets {
            base_color: material_map!($name, "color"),
            normal: Some(material_map!($name, "normal")),
            roughness: None,
        }
    };
}

macro_rules! define_builtin_materials {
    ($( $variant:ident => $name:ident, $maps:ident, $metallic:literal, $roughness:literal, $description:literal; )+) => {
        /// A material texture set shipped with Terrarium.
        ///
        /// Use a material directly with [`Material::builtin`](crate::Material::builtin).
        /// Built-in material maps use world-space triplanar projection.
        #[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
        #[repr(u8)]
        pub enum BuiltinMaterial {
            $(
                #[doc = $description]
                $variant,
            )+
        }

        impl BuiltinMaterial {
            /// Every built-in material in its stable declaration order.
            pub const ALL: [Self; [$(Self::$variant),+].len()] = [$(Self::$variant),+];

            pub(crate) const COUNT: usize = Self::ALL.len();

            /// Returns the lowercase asset name for this material.
            pub const fn name(self) -> &'static str {
                match self {
                    $(Self::$variant => stringify!($name),)+
                }
            }

            pub(crate) const fn metallic(self) -> f32 {
                match self {
                    $(Self::$variant => $metallic,)+
                }
            }

            pub(crate) const fn fallback_roughness(self) -> f32 {
                match self {
                    $(Self::$variant => $roughness,)+
                }
            }

            pub(crate) fn assets(self) -> BuiltinMaterialAssets {
                match self {
                    $(Self::$variant => material_assets!($maps, $name),)+
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

        impl super::material::Material {
            $(
                #[doc = concat!("Creates the built-in PBR material for `", stringify!($name), "`.")]
                pub fn $name() -> Self {
                    Self::builtin(BuiltinMaterial::$variant)
                }
            )+
        }
    };
}

define_builtin_materials! {
    Asphalt => asphalt, all_maps, 0.0, 0.8, "Asphalt surface.";
    Basalt => basalt, all_maps, 0.0, 0.8, "Basalt surface.";
    Brick => brick, all_maps, 0.0, 0.8, "Brick surface.";
    Cobblestone => cobblestone, all_maps, 0.0, 0.8, "Cobblestone surface.";
    Concrete => concrete, all_maps, 0.0, 0.8, "Concrete surface.";
    CorrodedMetal => corrodedmetal, all_maps, 0.8, 0.75, "Corroded metal surface.";
    CrackedLava => crackedlava, all_maps, 0.0, 0.8, "Cracked lava surface.";
    DiamondPlate => diamondplate, all_maps, 0.9, 0.4, "Diamond plate metal surface.";
    Fabric => fabric, all_maps, 0.0, 0.8, "Fabric surface.";
    Foil => foil, all_maps, 0.9, 0.4, "Foil metal surface.";
    Glacier => glacier, all_maps, 0.0, 0.18, "Glacier ice surface.";
    Granite => granite, all_maps, 0.0, 0.8, "Granite surface.";
    Grass => grass, color_and_roughness, 0.0, 0.9, "Grass surface.";
    Ground => ground, all_maps, 0.0, 0.8, "Ground surface.";
    Ice => ice, color_and_normal, 0.0, 0.18, "Ice surface.";
    LeafyGrass => leafygrass, color_and_normal, 0.0, 0.8, "Leafy grass surface.";
    Limestone => limestone, all_maps, 0.0, 0.8, "Limestone surface.";
    Marble => marble, all_maps, 0.0, 0.8, "Marble surface.";
    Metal => metal, all_maps, 1.0, 0.4, "Metal surface.";
    Mud => mud, all_maps, 0.0, 0.8, "Mud surface.";
    Pavement => pavement, all_maps, 0.0, 0.8, "Pavement surface.";
    Pebble => pebble, all_maps, 0.0, 0.8, "Pebble surface.";
    Plastic => plastic, color_and_normal, 0.0, 0.35, "Plastic surface.";
    Rock => rock, all_maps, 0.0, 0.8, "Rock surface.";
    Salt => salt, color_and_normal, 0.0, 0.8, "Salt surface.";
    Sand => sand, color_and_normal, 0.0, 0.9, "Sand surface.";
    Sandstone => sandstone, all_maps, 0.0, 0.8, "Sandstone surface.";
    Slate => slate, color_and_normal, 0.0, 0.8, "Slate surface.";
    Snow => snow, all_maps, 0.0, 0.9, "Snow surface.";
    Wood => wood, all_maps, 0.0, 0.8, "Wood surface.";
    WoodPlanks => woodplanks, all_maps, 0.0, 0.8, "Wood planks surface.";
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
