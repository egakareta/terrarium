#![cfg(not(target_family = "wasm"))]

use terrarium::*;

fn gpu() -> Result<(wgpu::Device, wgpu::Queue), Box<dyn std::error::Error>> {
    // Renderer caches retain device resources, so their native instance must stay alive too.
    static INSTANCE: std::sync::OnceLock<wgpu::Instance> = std::sync::OnceLock::new();
    let instance = INSTANCE.get_or_init(|| {
        wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle().with_env())
    });
    let adapter = pollster::block_on(instance.request_adapter(&Default::default()))?;
    Ok(pollster::block_on(
        adapter.request_device(&wgpu::DeviceDescriptor::default()),
    )?)
}

#[path = "../benches/render_benchmark/workload.rs"]
mod workload;

#[test]
fn caller_encoder_renders_and_resizes_output() -> Result<(), Box<dyn std::error::Error>> {
    let mut workspace = Workspace::new();
    workspace.lighting.clear_skybox();
    let (device, queue) = gpu()?;
    let mut renderer = Renderer::new(&device, &queue, wgpu::TextureFormat::Rgba8Unorm, [8, 8])?;
    renderer.with_clear_color(wgpu::Color::RED);

    for size in [[8, 8], [32, 16]] {
        let mut encoder = device.create_command_encoder(&Default::default());
        renderer.encode(&workspace.render_scene(), size, &mut encoder)?;
        queue.submit(Some(encoder.finish()));
        let pixels = renderer.read_pixels()?;
        assert_eq!(pixels.len(), (size[0] * size[1]) as usize);
        assert!(pixels.iter().all(|&pixel| pixel == [255, 0, 0, 255]));
    }
    Ok(())
}

#[test]
fn custom_shaders_validate_and_render_across_targets() -> Result<(), Box<dyn std::error::Error>> {
    let (device, queue) = gpu()?;
    for format in [
        wgpu::TextureFormat::Rgba8Unorm,
        wgpu::TextureFormat::Rgba8UnormSrgb,
    ] {
        let mut renderer = Renderer::new(&device, &queue, format, [8, 8])?;
        let validation = device.push_error_scope(wgpu::ErrorFilter::Validation);
        for source in [
            "not wgsl",
            "fn unrelated() {}",
            "fn shade_surface() -> f32 { return 0.0; }",
        ] {
            assert!(matches!(
                pollster::block_on(renderer.register_material_shader("invalid material", source)),
                Err(RendererError::InvalidShader { .. })
            ));
            assert!(matches!(
                pollster::block_on(renderer.register_post_process("invalid effect", source)),
                Err(RendererError::InvalidShader { .. })
            ));
        }
        let material_registration = renderer.register_material_shader("procedural surface", r#"
            fn shade_surface(input: SurfaceInput, surface: Surface) -> Surface {
                var result = surface;
                let stripes = smoothstep(0.4, 0.6, fract(input.uv.x * input.parameters.x));
                result.base_color = vec4<f32>(surface.base_color.rgb * stripes, surface.base_color.a);
                result.roughness = 0.8;
                result.normal = normalize(surface.normal + input.world_normal * 0.1);
                result.emissive = vec3<f32>(length(dpdx(input.world_position)) * 0.01);
                result.unlit = 1u;
                return result;
            }
        "#);
        let effect_registration = renderer.register_post_process(
            "color grade",
            r#"
            fn post_process(input: PostProcessInput) -> vec4<f32> {
                let color = sample_scene(input.uv + vec2<f32>(0.5, 0.0) / input.resolution);
                return vec4<f32>(color.rgb * input.parameters.rgb, color.a);
            }
        "#,
        );
        let mut workspace = Workspace::new();
        workspace.lighting.clear_skybox();
        renderer.prepare(&workspace.render_scene(), [8, 8])?;
        let material_shader = pollster::block_on(material_registration)?;
        let effect = pollster::block_on(effect_registration)?;
        drop(renderer.register_material_shader("cancelled", "not wgsl"));
        drop(renderer.register_post_process("cancelled", "not wgsl"));
        renderer.prepare(&workspace.render_scene(), [8, 8])?;
        let identity = pollster::block_on(renderer.register_material_shader(
            "identity",
            r#"
            fn shade_surface(input: SurfaceInput, surface: Surface) -> Surface { return surface; }
        "#,
        ))?;
        let invalid_material = MaterialShaderHandle(usize::MAX);
        let invalid_effect = PostProcessHandle(usize::MAX);
        assert!(matches!(
            renderer.set_post_processing(&[invalid_effect]),
            Err(RendererError::InvalidPostProcess(_))
        ));
        assert!(matches!(
            renderer.set_post_process_parameters(invalid_effect, [1.0; 4]),
            Err(RendererError::InvalidPostProcess(_))
        ));
        let mut other = Renderer::new(&device, &queue, format, [8, 8])?;
        assert!(matches!(
            other.set_post_processing(&[effect]),
            Err(RendererError::InvalidPostProcess(_))
        ));

        let camera = workspace.render_scene().camera;
        let material = Material::default()
            .with_shader(material_shader)
            .with_shader_parameters([8.0, 0.0, 0.0, 0.0]);
        let part = workspace.add_child(
            Part::new()
                .with_position(camera.position() + camera.forward * 4.0)
                .with_material(material)
                .with_material_slot(Face::Top, Material::default().with_shader(identity))
                .with_material_slot(Face::Bottom, Material::default()),
        );
        workspace.add_child(
            Part::new()
                .with_position(camera.position() + camera.forward * 6.0)
                .with_material(material)
                .with_transparency(0.5),
        );
        workspace.add_child(
            Part::new()
                .with_position(camera.position() + camera.forward * 8.0)
                .with_material(material)
                .with_transparency(0.5),
        );
        assert!(matches!(
            other.prepare(&workspace.render_scene(), [8, 8]),
            Err(RendererError::InvalidMaterialShader(_))
        ));
        workspace
            .get_mut::<Part>(part)
            .unwrap()
            .with_material_slot(Face::Top, Material::default().with_shader(invalid_material));
        assert!(matches!(
            renderer.prepare(&workspace.render_scene(), [8, 8]),
            Err(RendererError::InvalidMaterialShader(_))
        ));
        workspace
            .get_mut::<Part>(part)
            .unwrap()
            .with_material_slot(Face::Top, Material::default().with_shader(identity));

        renderer.set_post_process_parameters(effect, [0.8, 1.0, 0.9, 0.0])?;
        for size in [[8, 8], [32, 16]] {
            for effects in [
                vec![effect],
                vec![effect, effect],
                vec![effect, effect, effect],
                vec![],
            ] {
                renderer.set_post_processing(&effects)?;
                renderer.prepare(&workspace.render_scene(), size)?;
                renderer.wait_for_gpu()?;
            }
            workspace
                .get_mut::<Part>(part)
                .unwrap()
                .add_child(Outline::toon());
            renderer.set_post_processing(&[effect])?;
            renderer.prepare(&workspace.render_scene(), size)?;
            renderer.wait_for_gpu()?;
            let target_size = [size[0] * 2, size[1] * 2];
            let target = |format, usage| {
                device
                    .create_texture(&wgpu::TextureDescriptor {
                        label: None,
                        size: wgpu::Extent3d {
                            width: target_size[0],
                            height: target_size[1],
                            depth_or_array_layers: 1,
                        },
                        mip_level_count: 1,
                        sample_count: 1,
                        dimension: wgpu::TextureDimension::D2,
                        format,
                        usage,
                        view_formats: &[],
                    })
                    .create_view(&Default::default())
            };
            let color = target(format, wgpu::TextureUsages::RENDER_ATTACHMENT);
            let depth = target(DEPTH_FORMAT, wgpu::TextureUsages::RENDER_ATTACHMENT);
            renderer.render_to_view(
                &workspace.render_scene(),
                &color,
                &depth,
                target_size,
                wgpu::Color::BLACK,
            )?;
            renderer.wait_for_gpu()?;
            renderer.prepare(&workspace.render_scene(), size)?;
            renderer.wait_for_gpu()?;
        }
        let error = pollster::block_on(validation.pop());
        assert!(
            error.is_none(),
            "unexpected GPU validation error: {error:?}"
        );
    }
    Ok(())
}

#[test]
fn render_workload_animates_textured_shapes_and_replaces_blocks() -> Result<(), RendererError> {
    let mut workload = workload::RenderWorkload::new(64, [256, 144])?;
    let workspace = workload.workspace();
    let original = workspace
        .get_all::<Part>()
        .map(|part| (part.id(), part.pivot()))
        .collect::<Vec<_>>();
    assert!((1_065..=2_065).contains(&original.len()));
    for shape in [
        PartShape::Ball,
        PartShape::Cylinder,
        PartShape::Wedge,
        PartShape::CornerWedge,
    ] {
        assert!(
            workspace
                .get_all::<Part>()
                .any(|part| part.shape() == shape)
        );
    }
    let textures = workspace
        .get_all::<Part>()
        .flat_map(|part| Face::ALL.map(|face| part.material_slot(face).textures().base_color))
        .flatten()
        .collect::<std::collections::HashSet<_>>();
    assert_eq!(textures.len(), 6);
    let camera = workspace.camera().pivot();

    for _ in 0..31 {
        workload.advance();
    }
    let workspace = workload.workspace();
    assert_ne!(workspace.camera().pivot(), camera);
    assert!(original.iter().any(|&(id, pivot)| {
        workspace
            .get::<Part>(id)
            .is_some_and(|part| part.pivot() != pivot)
    }));
    assert!(
        workspace
            .get_all::<Part>()
            .any(|part| part.transparency() > 0.0)
    );
    let removed = original
        .iter()
        .filter(|&&(id, _)| workspace.get::<Part>(id).is_none())
        .count();
    assert!((1_000..=2_000).contains(&removed));
    assert!((1_065..=2_065).contains(&workspace.get_all::<Part>().count()));
    Ok(())
}
