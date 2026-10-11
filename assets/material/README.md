# Material Textures

These recreated textures provide tiled color, normal, and roughness maps for
Roblox-inspired materials. The source images were generated with AI assistance.

## Bundled maps

Built-in maps are embedded as ETC1S Basis Universal data at the encoder's
minimum quality, capped at 512 pixels on the longest edge, with mipmaps
omitted. The 86 maps total about 1.62 MiB; full-resolution WebP source maps are
not bundled. Mipmaps are generated after decoding when the renderer prepares a
material. To rebuild from externally stored WebP sources, use the upstream
Basis Universal encoder with ETC1S, minimum quality, 512-pixel maximum
dimensions, and mip generation off.

## Projection

Built-in material maps use world-space triplanar projection. The renderer
samples the X, Y, and Z planes and blends them with normalized fourth-power
weights derived from the absolute geometric normal. Coordinate signs preserve
the tangent-space orientation on opposing faces. Color, roughness, and normal
maps use the same coordinates and weights.

Triplanar blending is required on curved meshes. Selecting one tangent plane
from the smallest normal component creates a discontinuity whenever two
components exchange order: nearly identical sphere normals can then produce
very different texture coordinates and mapped lighting normals. A neutral
normal sample on every projection must blend back to the original geometric
normal, while non-neutral samples must perturb that normal continuously.

Custom SurfaceAppearance maps should keep their authored mesh UVs and use the
UV derivatives to construct their tangent frame; they do not use triplanar
projection.
