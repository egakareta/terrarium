use thiserror::Error;

use crate::Color3;

/// Depth format used by the built-in renderer.
pub const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
/// Number of material slots available to a mesh, including the base slot and
/// six directional slots.
pub const MATERIAL_SLOT_COUNT: usize = 7;

/// A handle to a texture stored in a [`crate::Workspace`].
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct TextureHandle(
    /// Index into the CPU texture storage owned by a [`crate::Workspace`].
    pub usize,
);

/// The material slot selected by a mesh vertex.
#[repr(u32)]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum MaterialSlot {
    /// The default material, stored in [`crate::Part::material`].
    Base = 0,
    /// The top-facing material slot.
    Top = 1,
    /// The bottom-facing material slot.
    Bottom = 2,
    /// The front-facing material slot toward `+Z`.
    Front = 3,
    /// The back-facing material slot toward `-Z`.
    Back = 4,
    /// The left-facing material slot toward `-X`.
    Left = 5,
    /// The right-facing material slot toward `+X`.
    Right = 6,
}

impl MaterialSlot {
    /// Returns the zero-based GPU slot index.
    pub const fn index(self) -> usize {
        self as usize
    }
}

/// Texture maps used by a PBR material.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TextureSet {
    /// Optional sRGB base-color texture.
    pub base_color: Option<TextureHandle>,
    /// Optional linear tangent-space normal texture.
    pub normal: Option<TextureHandle>,
    /// Optional linear glTF metallic-roughness texture.
    pub metallic_roughness: Option<TextureHandle>,
}

/// PBR factors and optional texture maps used by a [`crate::Part`].
///
/// The metallic-roughness map follows the glTF convention: metallic is read
/// from the blue channel and roughness from the green channel. All scalar
/// factors are supplied per instance, so parts can remain instanced efficiently.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Material {
    /// RGBA multiplier for the base color. Values are not clamped on assignment.
    pub base_color: [f32; 4],
    /// Metallic factor, clamped to `0.0..=1.0` during rendering.
    pub metallic: f32,
    /// Roughness factor, clamped to `0.04..=1.0` during rendering.
    pub roughness: f32,
    /// RGB emissive contribution added after direct and ambient lighting.
    pub emissive: [f32; 3],
    /// Optional texture maps multiplied by the scalar factors.
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

    /// Returns a copy of this material with a base-color map assigned.
    pub fn with_base_color_texture(mut self, texture: TextureHandle) -> Self {
        self.textures.base_color = Some(texture);
        self
    }

    /// Returns a copy of this material with a normal map assigned.
    pub fn with_normal_texture(mut self, texture: TextureHandle) -> Self {
        self.textures.normal = Some(texture);
        self
    }

    /// Returns a copy of this material with a metallic-roughness map assigned.
    pub fn with_metallic_roughness_texture(mut self, texture: TextureHandle) -> Self {
        self.textures.metallic_roughness = Some(texture);
        self
    }

    /// Creates a material whose base color comes from an RGB tint.
    pub fn from_color(color: Color3) -> Self {
        Self {
            base_color: color.rgba(),
            ..Self::default()
        }
    }
}

/// Materials assigned to the non-default slots of a mesh.
///
/// Slot zero is the [`crate::Part::material`] field. The first element in this list
/// is slot one, the second is slot two, and so on.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MeshMaterialSlots {
    /// Materials for slots after [`MaterialSlot::Base`], stored by slot order.
    pub slots: Vec<Material>,
}

impl MeshMaterialSlots {
    /// Assigns a non-base material slot, growing the slot list as needed.
    ///
    /// Passing [`MaterialSlot::Base`] panics because the base slot belongs in
    /// [`crate::Part::material`].
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
    /// Image width in pixels.
    pub width: u32,
    /// Image height in pixels.
    pub height: u32,
    /// Packed row-major RGBA8 pixels, four bytes per pixel.
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
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct Texture {
    /// Texture width in pixels.
    pub width: u32,
    /// Texture height in pixels.
    pub height: u32,
    /// Packed row-major RGBA8 pixels, four bytes per pixel.
    pub pixels: Vec<u8>,
    /// Whether the bytes should be sampled as color or data.
    pub color_space: TextureColorSpace,
}

/// Errors produced while decoding or validating image and texture data.
#[derive(Debug, Error)]
pub enum TextureError {
    /// The image decoder could not read the supplied bytes.
    #[error("could not decode image data: {0}")]
    ImageDecode(#[from] image::ImageError),
    /// Width or height was zero.
    #[error("texture dimensions must be greater than zero")]
    ZeroDimensions,
    /// The dimensions overflowed while calculating the expected byte count.
    #[error("texture dimensions are too large")]
    DimensionsTooLarge,
    /// The pixel buffer length did not equal `width * height * 4`.
    #[error("RGBA8 texture data has {actual} bytes, expected {expected}")]
    InvalidData {
        /// Number of bytes supplied by the caller.
        actual: usize,
        /// Number of bytes required by the dimensions.
        expected: usize,
    },
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

    /// Returns the number of mip levels in the automatically generated chain.
    pub fn mip_level_count(&self) -> u32 {
        let max_dimension = self.width.max(self.height);
        if max_dimension == 0 {
            0
        } else {
            max_dimension.ilog2() + 1
        }
    }

    /// Generates the complete mip chain, including the original image at level zero.
    ///
    /// RGB channels in sRGB textures are averaged in linear space before being
    /// encoded again. Linear textures, including normal and metallic-roughness
    /// maps, are averaged directly. The last row or column is included when a
    /// dimension is odd.
    pub fn mip_levels(&self) -> Result<Vec<Image>, TextureError> {
        self.validate()?;

        let mut levels = Vec::with_capacity(self.mip_level_count() as usize);
        let mut width = self.width;
        let mut height = self.height;
        let mut pixels = self.pixels.clone();
        levels.push(Image {
            width,
            height,
            pixels: pixels.clone(),
        });

        while width > 1 || height > 1 {
            let next_width = (width / 2).max(1);
            let next_height = (height / 2).max(1);
            pixels = downsample_rgba8(
                width,
                height,
                next_width,
                next_height,
                &pixels,
                self.color_space,
            );
            width = next_width;
            height = next_height;
            levels.push(Image {
                width,
                height,
                pixels: pixels.clone(),
            });
        }

        Ok(levels)
    }

    /// Checks dimensions and verifies that the pixel buffer is tightly packed RGBA8.
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

fn downsample_rgba8(
    source_width: u32,
    source_height: u32,
    width: u32,
    height: u32,
    source: &[u8],
    color_space: TextureColorSpace,
) -> Vec<u8> {
    let mut pixels = vec![0; width as usize * height as usize * 4];
    for y in 0..height {
        let source_y_start = (y as u64 * source_height as u64 / height as u64) as u32;
        let source_y_end = ((y as u64 + 1) * source_height as u64)
            .div_ceil(height as u64)
            .min(source_height as u64) as u32;
        for x in 0..width {
            let source_x_start = (x as u64 * source_width as u64 / width as u64) as u32;
            let source_x_end = ((x as u64 + 1) * source_width as u64)
                .div_ceil(width as u64)
                .min(source_width as u64) as u32;
            let mut sums = [0.0; 4];
            let mut sample_count = 0.0;
            for source_y in source_y_start..source_y_end {
                for source_x in source_x_start..source_x_end {
                    let source_index =
                        (source_y as usize * source_width as usize + source_x as usize) * 4;
                    for channel in 0..4 {
                        let value = source[source_index + channel];
                        sums[channel] += if color_space == TextureColorSpace::Srgb && channel < 3 {
                            srgb_to_linear(value)
                        } else {
                            value as f32 / 255.0
                        };
                    }
                    sample_count += 1.0;
                }
            }

            let destination_index = (y as usize * width as usize + x as usize) * 4;
            for channel in 0..4 {
                let value = sums[channel] / sample_count;
                pixels[destination_index + channel] =
                    if color_space == TextureColorSpace::Srgb && channel < 3 {
                        linear_to_srgb(value)
                    } else {
                        (value * 255.0).round() as u8
                    };
            }
        }
    }
    pixels
}

fn srgb_to_linear(value: u8) -> f32 {
    let value = value as f32 / 255.0;
    if value <= 0.04045 {
        value / 12.92
    } else {
        ((value + 0.055) / 1.055).powf(2.4)
    }
}

fn linear_to_srgb(value: f32) -> u8 {
    let value = if value <= 0.0031308 {
        value * 12.92
    } else {
        1.055 * value.powf(1.0 / 2.4) - 0.055
    };
    (value.clamp(0.0, 1.0) * 255.0).round() as u8
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

    #[test]
    fn texture_mip_levels_downsample_odd_dimensions_and_retain_alpha() {
        let texture = Texture::linear(
            3,
            2,
            vec![
                0, 10, 20, 30, 10, 20, 30, 40, 20, 30, 40, 50, 30, 40, 50, 60, 40, 50, 60, 70, 50,
                60, 70, 80,
            ],
        )
        .unwrap();

        assert_eq!(texture.mip_level_count(), 2);
        let levels = texture.mip_levels().unwrap();
        assert_eq!(levels.len(), 2);
        assert_eq!((levels[1].width, levels[1].height), (1, 1));
        assert_eq!(levels[1].pixels, vec![25, 35, 45, 55]);
    }

    #[test]
    fn srgb_mip_levels_average_rgb_in_linear_space() {
        let texture = Texture::new(2, 1, vec![0, 0, 0, 0, 255, 255, 255, 255]).unwrap();

        let levels = texture.mip_levels().unwrap();
        assert_eq!(levels[1].pixels, vec![188, 188, 188, 128]);
    }
}
