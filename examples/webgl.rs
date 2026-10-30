use terrarium::*;

fn main() {
    let launcher = Terrarium::new();

    #[cfg(all(target_family = "wasm", not(target_os = "wasi")))]
    let launcher = launcher.with_wgpu_options(Some(Box::new(|options| {
        let mut setup = egui_wgpu::WgpuSetupCreateNew::without_display_handle();
        setup.instance_descriptor.backends = wgpu::Backends::GL;
        options.wgpu_setup = setup.into();
    })));

    launcher
        .run(|engine| {
            engine
                .workspace_mut()
                .add_child(Part::new().with_size(glam::Vec3::splat(2.0)));
            Ok::<(), AppCreationError>(())
        })
        .unwrap();
}
