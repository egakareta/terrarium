# Material Textures

These recreated textures provide tiled color, normal, and roughness maps for
Roblox-inspired materials. The source images were generated with AI assistance.

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

Custom SurfaceAppearance maps should keep their authored mesh UVs and use
the UV derivatives to construct their tangent frame; they do not use triplanar
projection.
