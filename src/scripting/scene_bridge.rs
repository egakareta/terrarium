use std::{
    cell::{Cell, RefCell},
    collections::{HashMap, HashSet},
    rc::Rc,
};

use glam::Vec3;
use rquickjs::{Ctx, Error, Function, Object};

use super::{JavaScriptResult, JavaScriptRuntime};
use crate::{
    Color3, HasBasePart, HasPVInstance, HasPart, Instance, InstanceId, Part, PartShape, Workspace,
};

const INSTANCE_API: &str = include_str!("api.js");

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen]
unsafe extern "C" {
    #[wasm_bindgen(js_namespace = console, js_name = log)]
    fn console_log(message: &str);
}

pub(super) fn install_instance_bridge(
    runtime: &JavaScriptRuntime,
    commands: Rc<RefCell<Vec<EngineCommand>>>,
    next_handle: Rc<Cell<u64>>,
    live_handles: Rc<RefCell<HashSet<u64>>>,
) -> JavaScriptResult<()> {
    runtime.with_context(|ctx| {
        let create_commands = Rc::clone(&commands);
        let create_next_handle = Rc::clone(&next_handle);
        let create_live_handles = Rc::clone(&live_handles);
        ctx.globals().set(
            "__internal_create_instance",
            Function::new(
                ctx.clone(),
                move |type_name: String, options: Option<Object<'_>>| {
                    if type_name != "Part" {
                        return Err(Error::new_from_js_message(
                            "Instance type",
                            "Part",
                            format!("unsupported instance type `{type_name}`"),
                        ));
                    }
                    let spec = parse_part_spec(options)?;
                    let handle = allocate_handle(&create_next_handle);
                    create_live_handles.borrow_mut().insert(handle);
                    create_commands
                        .borrow_mut()
                        .push(EngineCommand::AddPart { handle, spec });
                    Ok::<_, Error>(handle)
                },
            )?,
        )?;

        let active_commands = Rc::clone(&commands);
        ctx.globals().set(
            "__internal_is_active",
            Function::new(ctx.clone(), move |handle: u64| {
                Ok::<_, Error>(
                    live_handles.borrow().contains(&handle)
                        && !active_commands.borrow().iter().any(|command| {
                            matches!(command, EngineCommand::RemoveInstance(removed) if *removed == handle)
                        }),
                )
            })?,
        )?;

        install_instance_function(
            &ctx,
            "__internal_destroy_instance",
            Rc::clone(&commands),
            EngineCommand::RemoveInstance,
        )?;
        install_instance_vec3_function(
            &ctx,
            "__internal_set_position",
            Rc::clone(&commands),
            |handle, position| EngineCommand::SetPosition { handle, position },
            "position",
        )?;
        install_instance_vec3_function(
            &ctx,
            "__internal_set_orientation",
            Rc::clone(&commands),
            |handle, orientation| EngineCommand::SetOrientation {
                handle,
                orientation,
            },
            "orientation",
        )?;
        install_instance_vec3_function(
            &ctx,
            "__internal_set_size",
            Rc::clone(&commands),
            |handle, size| EngineCommand::SetSize { handle, size },
            "size",
        )?;
        install_instance_vec3_function(
            &ctx,
            "__internal_set_color",
            Rc::clone(&commands),
            |handle, value| EngineCommand::SetColor {
                handle,
                color: Color3::new(value.x, value.y, value.z),
            },
            "color",
        )?;

        let name_commands = Rc::clone(&commands);
        ctx.globals().set(
            "__internal_set_name",
            Function::new(ctx.clone(), move |handle: u64, name: String| {
                name_commands
                    .borrow_mut()
                    .push(EngineCommand::SetName { handle, name });
                Ok::<_, Error>(())
            })?,
        )?;
        let transparency_commands = Rc::clone(&commands);
        ctx.globals().set(
            "__internal_set_transparency",
            Function::new(ctx.clone(), move |handle: u64, value: f32| {
                transparency_commands
                    .borrow_mut()
                    .push(EngineCommand::SetTransparency {
                        handle,
                        transparency: value,
                    });
                Ok::<_, Error>(())
            })?,
        )?;
        let anchored_commands = Rc::clone(&commands);
        ctx.globals().set(
            "__internal_set_anchored",
            Function::new(ctx.clone(), move |handle: u64, value: bool| {
                anchored_commands
                    .borrow_mut()
                    .push(EngineCommand::SetAnchored {
                        handle,
                        anchored: value,
                    });
                Ok::<_, Error>(())
            })?,
        )?;
        let collision_commands = Rc::clone(&commands);
        ctx.globals().set(
            "__internal_set_can_collide",
            Function::new(ctx.clone(), move |handle: u64, value: bool| {
                collision_commands
                    .borrow_mut()
                    .push(EngineCommand::SetCanCollide {
                        handle,
                        can_collide: value,
                    });
                Ok::<_, Error>(())
            })?,
        )?;
        ctx.globals().set(
            "log",
            Function::new(ctx.clone(), |message: String| {
                #[cfg(not(target_arch = "wasm32"))]
                log::info!("[javascript] {message}");
                #[cfg(target_arch = "wasm32")]
                console_log(&format!("[javascript] {message}"));
                Ok::<_, Error>(())
            })?,
        )?;
        ctx.eval::<(), _>(INSTANCE_API)
    })
}

fn install_instance_function(
    ctx: &Ctx<'_>,
    name: &str,
    commands: Rc<RefCell<Vec<EngineCommand>>>,
    make: impl Fn(u64) -> EngineCommand + 'static,
) -> JavaScriptResult<()> {
    ctx.globals().set(
        name,
        Function::new(ctx.clone(), move |handle: u64| {
            commands.borrow_mut().push(make(handle));
            Ok::<_, Error>(())
        })?,
    )?;
    Ok(())
}

fn install_instance_vec3_function(
    ctx: &Ctx<'_>,
    name: &str,
    commands: Rc<RefCell<Vec<EngineCommand>>>,
    make: impl Fn(u64, Vec3) -> EngineCommand + 'static,
    field: &'static str,
) -> JavaScriptResult<()> {
    ctx.globals().set(
        name,
        Function::new(ctx.clone(), move |handle: u64, values: Vec<f32>| {
            let value = parse_vec3(values, field)?;
            commands.borrow_mut().push(make(handle, value));
            Ok::<_, Error>(())
        })?,
    )?;
    Ok(())
}

fn allocate_handle(next_handle: &Cell<u64>) -> u64 {
    let handle = next_handle.get();
    next_handle.set(handle.wrapping_add(1).max(1));
    handle
}

fn parse_vec3(values: Vec<f32>, field: &'static str) -> JavaScriptResult<Vec3> {
    match values.as_slice() {
        [x, y, z] => Ok(Vec3::new(*x, *y, *z)),
        _ => Err(Error::new_from_js_message(
            field,
            "array",
            "expected three numbers",
        )),
    }
}

#[derive(Clone)]
pub(super) struct PartSpec {
    name: Option<String>,
    shape: PartShape,
    position: Vec3,
    size: Vec3,
    color: Color3,
    transparency: f32,
    anchored: bool,
    can_collide: bool,
}

fn parse_part_spec(options: Option<Object<'_>>) -> JavaScriptResult<PartSpec> {
    let Some(options) = options else {
        return Ok(PartSpec {
            name: None,
            shape: PartShape::Block,
            position: Vec3::ZERO,
            size: Vec3::ONE,
            color: Color3::WHITE,
            transparency: 0.0,
            anchored: true,
            can_collide: true,
        });
    };
    let shape = match options.get::<_, Option<String>>("shape")?.as_deref() {
        Some("ball") => PartShape::Ball,
        Some("cylinder") => PartShape::Cylinder,
        Some("wedge") => PartShape::Wedge,
        Some("cornerWedge") => PartShape::CornerWedge,
        Some("block") | None => PartShape::Block,
        Some(value) => return Err(Error::new_from_js_message("shape", "known shape", value)),
    };
    Ok(PartSpec {
        name: options.get("name")?,
        shape,
        position: parse_optional_vec3(options.get("position")?, Vec3::ZERO, "position")?,
        size: parse_optional_vec3(options.get("size")?, Vec3::ONE, "size")?,
        color: options
            .get::<_, Option<Vec<f32>>>("color")?
            .map(|value| parse_vec3(value, "color"))
            .transpose()?
            .map_or(Color3::WHITE, |value| {
                Color3::new(value.x, value.y, value.z)
            }),
        transparency: options
            .get::<_, Option<f32>>("transparency")?
            .unwrap_or(0.0),
        anchored: options.get::<_, Option<bool>>("anchored")?.unwrap_or(true),
        can_collide: options
            .get::<_, Option<bool>>("canCollide")?
            .unwrap_or(true),
    })
}

fn parse_optional_vec3(
    value: Option<Vec<f32>>,
    default: Vec3,
    field: &'static str,
) -> JavaScriptResult<Vec3> {
    value.map_or(Ok(default), |value| parse_vec3(value, field))
}

pub(super) enum EngineCommand {
    AddPart { handle: u64, spec: PartSpec },
    RemoveInstance(u64),
    SetPosition { handle: u64, position: Vec3 },
    SetOrientation { handle: u64, orientation: Vec3 },
    SetSize { handle: u64, size: Vec3 },
    SetColor { handle: u64, color: Color3 },
    SetName { handle: u64, name: String },
    SetTransparency { handle: u64, transparency: f32 },
    SetAnchored { handle: u64, anchored: bool },
    SetCanCollide { handle: u64, can_collide: bool },
}

pub(super) fn apply_command(
    handles: &mut HashMap<u64, InstanceId>,
    live_handles: &mut HashSet<u64>,
    workspace: &mut Workspace,
    command: EngineCommand,
) {
    match command {
        EngineCommand::AddPart { handle, spec } => {
            let mut part = Part::new()
                .with_shape(spec.shape)
                .with_position(spec.position)
                .with_size(spec.size)
                .with_color(spec.color)
                .with_transparency(spec.transparency)
                .with_anchored(spec.anchored)
                .with_can_collide(spec.can_collide);
            if let Some(name) = spec.name {
                part.set_name(name);
            }
            let id = workspace.add_child(part);
            handles.insert(handle, id);
        }
        EngineCommand::RemoveInstance(handle) => {
            live_handles.remove(&handle);
            if let Some(id) = handles.remove(&handle) {
                workspace.remove_child(id);
            }
        }
        EngineCommand::SetPosition { handle, position } => {
            if let Some(part) = part_for_handle(handles, workspace, handle) {
                part.with_position(position);
            }
        }
        EngineCommand::SetOrientation {
            handle,
            orientation,
        } => {
            if let Some(part) = part_for_handle(handles, workspace, handle) {
                part.with_orientation(orientation);
            }
        }
        EngineCommand::SetSize { handle, size } => {
            if let Some(part) = part_for_handle(handles, workspace, handle) {
                part.with_size(size);
            }
        }
        EngineCommand::SetColor { handle, color } => {
            if let Some(part) = part_for_handle(handles, workspace, handle) {
                part.with_color(color);
            }
        }
        EngineCommand::SetName { handle, name } => {
            if let Some(part) = part_for_handle(handles, workspace, handle) {
                part.set_name(name);
            }
        }
        EngineCommand::SetTransparency {
            handle,
            transparency,
        } => {
            if let Some(part) = part_for_handle(handles, workspace, handle) {
                part.with_transparency(transparency);
            }
        }
        EngineCommand::SetAnchored { handle, anchored } => {
            if let Some(part) = part_for_handle(handles, workspace, handle) {
                part.with_anchored(anchored);
            }
        }
        EngineCommand::SetCanCollide {
            handle,
            can_collide,
        } => {
            if let Some(part) = part_for_handle(handles, workspace, handle) {
                part.with_can_collide(can_collide);
            }
        }
    }
}

fn part_for_handle<'a>(
    handles: &HashMap<u64, InstanceId>,
    workspace: &'a mut Workspace,
    handle: u64,
) -> Option<&'a mut Part> {
    workspace.get_mut::<Part>(*handles.get(&handle)?)
}
