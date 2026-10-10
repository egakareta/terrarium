/// A material texture set shipped with Terrarium.
///
/// Load a material into a workspace with
/// [`Workspace::load_builtin_texture`](crate::Workspace::load_builtin_texture).
/// Built-in material maps use world-space triplanar projection.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[repr(u8)]
pub enum BuiltinTexture {
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

impl BuiltinTexture {
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

    pub(crate) const fn index(self) -> usize {
        self as usize
    }

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
}

pub(crate) struct BuiltinMaterialAssets {
    pub(crate) base_color: &'static [u8],
    pub(crate) normal: Option<&'static [u8]>,
    pub(crate) roughness: Option<&'static [u8]>,
}
