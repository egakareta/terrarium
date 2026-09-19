use super::*;
use crate::HasCamera;

#[test]
fn surface_packing_preserves_normal_xy_and_material_channels() {
    let normal = Texture::linear(1, 1, vec![10, 20, 30, 255]).unwrap();
    let metallic_roughness = Texture::linear(1, 1, vec![40, 50, 60, 255]).unwrap();

    assert_eq!(
        pack_surface_pixels(&normal, &metallic_roughness, 1, 1),
        vec![10, 20, 60, 50]
    );
}

#[test]
fn material_shader_validates_with_explicit_texture_gradients() {
    let module = wgpu::naga::front::wgsl::parse_str(include_str!("shader.wgsl"))
        .expect("material shader should parse");
    let mut validator = wgpu::naga::valid::Validator::new(
        wgpu::naga::valid::ValidationFlags::all(),
        wgpu::naga::valid::Capabilities::empty(),
    );
    validator
        .validate(&module)
        .expect("material shader should validate");
}

#[test]
fn skybox_shader_validates() {
    let module = wgpu::naga::front::wgsl::parse_str(include_str!("skybox.wgsl"))
        .expect("skybox shader should parse");
    let mut validator = wgpu::naga::valid::Validator::new(
        wgpu::naga::valid::ValidationFlags::all(),
        wgpu::naga::valid::Capabilities::empty(),
    );
    validator
        .validate(&module)
        .expect("skybox shader should validate");
}

#[test]
fn eframe_composite_shader_validates() {
    let module = wgpu::naga::front::wgsl::parse_str(include_str!("blit.wgsl"))
        .expect("eframe composite shader should parse");
    let mut validator = wgpu::naga::valid::Validator::new(
        wgpu::naga::valid::ValidationFlags::all(),
        wgpu::naga::valid::Capabilities::empty(),
    );
    validator
        .validate(&module)
        .expect("eframe composite shader should validate");
}

#[test]
fn material_set_keys_resolve_unset_slots_to_the_default_material() {
    use crate::{Face, HasMaterials as _, Instance as _, Part};

    let part = Part::new().named("part");
    let top = Material::default()
        .with_base_color([0.1, 0.4, 0.2, 1.0])
        .with_metallic(0.1)
        .with_roughness(0.9)
        .with_emissive([0.0, 0.0, 0.0]);
    let part = part.with_material_slot(Face::Top, top);

    let vec4s = MaterialSetKey::from_materials(&part.material_slots).vec4s();
    // The Top override carries its own factors.
    assert_eq!(vec4s[0], [0.1, 0.4, 0.2, 1.0]);
    assert_eq!(vec4s[1], [0.0, 0.0, 0.0, 0.9]);
    assert_eq!(vec4s[2][0], 0.1);
    // Unset slots fall back to the default material.
    assert_eq!(vec4s[3], [1.0, 1.0, 1.0, 1.0]);
    assert_eq!(vec4s[4], [0.0, 0.0, 0.0, 0.5]);
    assert_eq!(vec4s[5][0], 0.0);
}

#[test]
fn material_set_keys_distinguish_per_face_factors() {
    use crate::{Face, HasMaterials as _, Instance as _, Part};

    let copper = Part::new()
        .named("copper")
        .with_material(Material::default().with_metallic(0.82));
    let copper_top_metal = Part::new()
        .named("copper-top-metal")
        .with_material(Material::default().with_metallic(0.82))
        .with_material_slot(Face::Top, Material::default().with_metallic(0.1));
    let copper_clone = Part::new()
        .named("copper-clone")
        .with_material(Material::default().with_metallic(0.82));

    assert_eq!(
        MaterialSetKey::from_materials(&copper.material_slots),
        MaterialSetKey::from_materials(&copper_clone.material_slots)
    );
    assert_ne!(
        MaterialSetKey::from_materials(&copper.material_slots),
        MaterialSetKey::from_materials(&copper_top_metal.material_slots)
    );
}

#[test]
fn compact_normal_scales_match_inverse_transpose_for_trs() {
    use crate::glam::{Quat, Vec3};
    let rotation = Quat::from_euler(crate::glam::EulerRot::XYZ, 0.4, -0.7, 0.2);
    let pivot = Mat4::from_rotation_translation(rotation, Vec3::new(1.0, -2.0, 3.0));
    let size = Vec3::new(0.82, 1.1, 0.6);
    let model = pivot * Mat4::from_scale(size);
    let scales = normal_scales_from_model(&model);
    let columns = [
        (model.x_axis.truncate() * scales[0]).to_array(),
        (model.y_axis.truncate() * scales[1]).to_array(),
        (model.z_axis.truncate() * scales[2]).to_array(),
    ];
    let reference = model.inverse().transpose().to_cols_array_2d();
    for (computed, expected) in columns.iter().zip([
        [reference[0][0], reference[0][1], reference[0][2]],
        [reference[1][0], reference[1][1], reference[1][2]],
        [reference[2][0], reference[2][1], reference[2][2]],
    ]) {
        for (a, b) in computed.iter().zip(expected.iter()) {
            assert!((a - b).abs() < 1e-5, "got {computed:?}, want {expected:?}");
        }
    }
}

#[test]
fn frustum_culling_keeps_visible_and_rejects_outside() {
    use crate::{Camera, HasPVInstance as _};
    let camera = Camera::new(Vec3::new(0.0, 2.0, 6.0), Vec3::ZERO, 16.0 / 9.0);
    let planes = frustum_planes(camera.view_projection_matrix());
    assert!(sphere_visible(&planes, Vec3::ZERO, 0.5));
    // Far behind the camera must be culled.
    assert!(!sphere_visible(&planes, Vec3::new(0.0, 2.0, 20.0), 0.5));
    // Far beyond far plane must be culled.
    assert!(!sphere_visible(
        &planes,
        camera.pivot().w_axis.truncate() + camera.forward() * 500.0,
        0.5
    ));
}

#[test]
fn shadow_cascades_cover_their_camera_frustum_slices() {
    let camera = Camera::new(Vec3::new(3.0, 4.0, 8.0), Vec3::ZERO, 16.0 / 9.0);
    let (matrices, splits, texel_sizes) =
        light_view_projections(&camera, Vec3::new(-0.45, 0.85, 0.35));
    let mut near = camera.znear();

    for cascade in 0..SHADOW_CASCADE_COUNT {
        assert!(splits[cascade] > near);
        assert!(texel_sizes[cascade] > 0.0);
        for corner in camera_frustum_slice_corners(&camera, near, splits[cascade]) {
            let clip = matrices[cascade] * corner.extend(1.0);
            let projected = clip.truncate() / clip.w;
            assert!(projected.x.abs() <= 1.0, "cascade {cascade}: {projected:?}");
            assert!(projected.y.abs() <= 1.0, "cascade {cascade}: {projected:?}");
            assert!(
                (0.0..=1.0).contains(&projected.z),
                "cascade {cascade}: {projected:?}"
            );
        }
        near = splits[cascade];
    }
    assert!((splits[SHADOW_CASCADE_COUNT - 1] - SHADOW_DISTANCE).abs() < 1e-4);
}
