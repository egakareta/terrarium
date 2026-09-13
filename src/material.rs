use thiserror::Error;

use crate::Color3;

pub const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
pub const MATERIAL_SLOT_COUNT: usize = 4;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct MaterialTextures {
    pub(crate) base_color: [TextureHandle; MATERIAL_SLOT_COUNT],
    pub(crate) normal: [TextureHandle; MATERIAL_SLOT_COUNT],
    pub(crate) metallic_roughness: [TextureHandle; MATERIAL_SLOT_COUNT],
}

/// A handle to a texture stored on the GPU.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct TextureHandle(pub usize);

/// The material slot selected by a mesh vertex.
#[repr(u32)]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum MaterialSlot {
    Base = 0,
    Top = 1,
    Bottom = 2,
    Side = 3,
}

impl MaterialSlot {
    pub const fn index(self) -> usize {
        self as usize
    }
}

/// Texture maps used by a PBR material.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TextureSet {
    pub base_color: Option<TextureHandle>,
    pub normal: Option<TextureHandle>,
    pub metallic_roughness: Option<TextureHandle>,
}

/// PBR factors and optional texture maps used by a [`Part`].
///
/// The metallic-roughness map follows the glTF convention: metallic is read
/// from the blue channel and roughness from the green channel. All scalar
/// factors are supplied per instance, so parts can remain instanced efficiently.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Material {
    pub base_color: [f32; 4],
    pub metallic: f32,
    pub roughness: f32,
    pub emissive: [f32; 3],
    pub textures: TextureSet,
}

impl Default for Material {
    fn default() -> Self {
        Self {
            base_color: [1.0, 1.0, 1.0, 1.0],
            metallic: 0.0,
            roughness: 0.5,
            emissive: [0.0, 0.0, 0.0],
            textures: TextureSet::default(),
        }
    }
}

impl Material {
    /// Creates a material using a texture as its base-color map.
    pub fn textured(texture: TextureHandle) -> Self {
        Self::default().with_base_color_texture(texture)
    }

    pub fn with_base_color_texture(mut self, texture: TextureHandle) -> Self {
        self.textures.base_color = Some(texture);
        self
    }

    pub fn with_normal_texture(mut self, texture: TextureHandle) -> Self {
        self.textures.normal = Some(texture);
        self
    }

    pub fn with_metallic_roughness_texture(mut self, texture: TextureHandle) -> Self {
        self.textures.metallic_roughness = Some(texture);
        self
    }

    pub fn from_color(color: Color3) -> Self {
        Self {
            base_color: color.rgba(),
            ..Self::default()
        }
    }
}

/// Materials assigned to the non-default slots of a mesh.
///
/// Slot zero is the [`Part::material`] field. The first element in this list
/// is slot one, the second is slot two, and so on.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MeshMaterialSlots {
    pub slots: Vec<Material>,
}

impl MeshMaterialSlots {
    pub fn set(&mut self, slot: MaterialSlot, material: Material) {
        assert!(slot != MaterialSlot::Base, "slot zero is Part::material");
        let index = slot.index() - 1;
        self.slots.resize(index + 1, Material::default());
        self.slots[index] = material;
    }
}

/// The color space used when sampling an RGBA8 texture.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum TextureColorSpace {
    /// Color data such as a base-color map. The GPU converts it to linear space.
    Srgb,
    /// Data maps such as normals and metallic-roughness.
    Linear,
}

/// CPU-side image data independent of GPU texture resources.
#[derive(Clone, Debug)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

impl Image {
    /// Decodes any image format supported by the configured image codecs.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, TextureError> {
        let image = image::load_from_memory(bytes)?.to_rgba8();
        let (width, height) = image.dimensions();
        Self::from_rgba8(width, height, image.into_raw())
    }

    /// Creates an image from RGBA8 pixels.
    pub fn from_rgba8(width: u32, height: u32, pixels: Vec<u8>) -> Result<Self, TextureError> {
        let image = Self {
            width,
            height,
            pixels,
        };
        image.validate()?;
        Ok(image)
    }

    fn validate(&self) -> Result<(), TextureError> {
        if self.width == 0 || self.height == 0 {
            return Err(TextureError::ZeroDimensions);
        }
        let expected = (self.width as usize)
            .checked_mul(self.height as usize)
            .and_then(|pixels| pixels.checked_mul(4))
            .ok_or(TextureError::DimensionsTooLarge)?;
        if self.pixels.len() != expected {
            return Err(TextureError::InvalidData {
                actual: self.pixels.len(),
                expected,
            });
        }
        Ok(())
    }
}

/// CPU-side RGBA8 texture data ready to be uploaded to a [`Renderer`].
///
/// Textures are intentionally single-mip resources for now. The renderer does
/// not generate or upload additional mip levels.
#[derive(Clone, Debug)]
pub struct Texture {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
    pub color_space: TextureColorSpace,
}

#[derive(Debug, Error)]
pub enum TextureError {
    #[error("could not decode image data: {0}")]
    ImageDecode(#[from] image::ImageError),
    #[error("texture dimensions must be greater than zero")]
    ZeroDimensions,
    #[error("texture dimensions are too large")]
    DimensionsTooLarge,
    #[error("RGBA8 texture data has {actual} bytes, expected {expected}")]
    InvalidData { actual: usize, expected: usize },
}

impl Texture {
    /// Creates an sRGB RGBA8 texture, suitable for a base-color map.
    pub fn new(width: u32, height: u32, pixels: Vec<u8>) -> Result<Self, TextureError> {
        Self::with_color_space(width, height, pixels, TextureColorSpace::Srgb)
    }

    /// Creates an RGBA8 texture with an explicit color space.
    pub fn with_color_space(
        width: u32,
        height: u32,
        pixels: Vec<u8>,
        color_space: TextureColorSpace,
    ) -> Result<Self, TextureError> {
        let texture = Self {
            width,
            height,
            pixels,
            color_space,
        };
        texture.validate()?;
        Ok(texture)
    }

    /// Creates a texture from decoded image data.
    pub fn from_image(image: Image, color_space: TextureColorSpace) -> Result<Self, TextureError> {
        Self::with_color_space(image.width, image.height, image.pixels, color_space)
    }

    /// Creates a linear RGBA8 texture, suitable for normal or metallic-roughness data.
    pub fn linear(width: u32, height: u32, pixels: Vec<u8>) -> Result<Self, TextureError> {
        Self::with_color_space(width, height, pixels, TextureColorSpace::Linear)
    }

    /// Decodes an image from memory with an explicit texture color space.
    pub fn from_bytes(bytes: &[u8], color_space: TextureColorSpace) -> Result<Self, TextureError> {
        Self::from_image(Image::from_bytes(bytes)?, color_space)
    }

    pub fn validate(&self) -> Result<(), TextureError> {
        if self.width == 0 || self.height == 0 {
            return Err(TextureError::ZeroDimensions);
        }
        let expected = (self.width as usize)
            .checked_mul(self.height as usize)
            .and_then(|pixels| pixels.checked_mul(4))
            .ok_or(TextureError::DimensionsTooLarge)?;
        if self.pixels.len() != expected {
            return Err(TextureError::InvalidData {
                actual: self.pixels.len(),
                expected,
            });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn textures_validate_rgba8_data_and_material_defaults_are_rough_dielectrics() {
        assert!(matches!(
            Texture::new(2, 2, vec![0; 3]),
            Err(TextureError::InvalidData {
                actual: 3,
                expected: 16
            })
        ));
        let texture = Texture::linear(1, 1, vec![128, 128, 255, 255]).unwrap();
        assert_eq!(texture.color_space, TextureColorSpace::Linear);

        let material = Material::default();
        assert_eq!(material.base_color, [1.0, 1.0, 1.0, 1.0]);
        assert_eq!(material.metallic, 0.0);
        assert_eq!(material.roughness, 0.5);
        assert!(material.textures.base_color.is_none());
        assert!(material.textures.normal.is_none());
        assert!(material.textures.metallic_roughness.is_none());
    }

    #[test]
    fn material_helpers_configure_a_texture_set() {
        let texture = TextureHandle(7);
        let material = Material::textured(texture)
            .with_normal_texture(TextureHandle(8))
            .with_metallic_roughness_texture(TextureHandle(9));

        assert_eq!(
            material.textures,
            TextureSet {
                base_color: Some(texture),
                normal: Some(TextureHandle(8)),
                metallic_roughness: Some(TextureHandle(9)),
            }
        );
    }
}
