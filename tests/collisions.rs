#![cfg(feature = "physics")]

use std::{cell::RefCell, rc::Rc};

use terrarium::{
    BasePart, HasBasePart, HasPVInstance, HasPart, Instance, InstanceId, Part, PartShape, Signal,
    SignalClosed, Workspace, glam::Vec3,
};

const DT: f32 = 1.0 / 60.0;

fn contact_scene() -> (Workspace, InstanceId, InstanceId) {
    let mut workspace = Workspace::new();
    let floor = BasePart::new().with_size(Vec3::new(4.0, 1.0, 4.0));
    let ball = Part::new()
        .with_shape(PartShape::Ball)
        .with_anchored(false)
        .with_position(Vec3::new(0.0, 0.9, 0.0));
    let floor_id = floor.id();
    let ball_id = ball.id();
    workspace.add_child(floor);
    workspace.add_child(ball);
    workspace.update(0.0);
    // Keep the contact fixture stationary while exercising real Rapier contacts.
    workspace
        .physics_mut()
        .body_mut(ball_id)
        .unwrap()
        .lock_translations(true, true);
    (workspace, floor_id, ball_id)
}

fn received(signal: Signal<InstanceId>) -> Rc<RefCell<Vec<InstanceId>>> {
    let seen = Rc::new(RefCell::new(Vec::new()));
    let recorded = seen.clone();
    signal.connect(move |_, other| recorded.borrow_mut().push(*other));
    seen
}

#[test]
fn contacts_notify_both_parts_once_per_transition() {
    let (mut workspace, floor, ball) = contact_scene();
    let floor_started = received(workspace.get::<BasePart>(floor).unwrap().on_touched());
    let floor_ended = received(workspace.get::<BasePart>(floor).unwrap().on_touch_ended());
    let ball_started = received(workspace.get::<Part>(ball).unwrap().on_touched());
    let ball_ended = received(workspace.get::<Part>(ball).unwrap().on_touch_ended());

    for _ in 0..10 {
        workspace.update(DT);
    }
    assert_eq!(*floor_started.borrow(), [ball]);
    assert_eq!(*ball_started.borrow(), [floor]);
    assert!(floor_ended.borrow().is_empty());
    assert!(ball_ended.borrow().is_empty());

    workspace
        .get_mut::<Part>(ball)
        .unwrap()
        .with_position(Vec3::new(0.0, 4.0, 0.0));
    for _ in 0..3 {
        workspace.update(DT);
    }
    assert_eq!(*floor_ended.borrow(), [ball]);
    assert_eq!(*ball_ended.borrow(), [floor]);

    workspace
        .get_mut::<Part>(ball)
        .unwrap()
        .with_position(Vec3::new(0.0, 0.9, 0.0));
    workspace.update(DT);
    assert_eq!(*floor_started.borrow(), [ball, ball]);
    assert_eq!(*ball_started.borrow(), [floor, floor]);
}

#[test]
fn collider_rebuilds_preserve_contact_and_disabling_ends_it() {
    let (mut workspace, floor, ball) = contact_scene();
    let started = received(workspace.get::<Part>(ball).unwrap().on_touched());
    let ended = received(workspace.get::<Part>(ball).unwrap().on_touch_ended());
    workspace.update(DT);

    workspace
        .get_mut::<Part>(ball)
        .unwrap()
        .with_size(Vec3::splat(1.1))
        .with_shape(PartShape::Block);
    workspace.update(DT);
    assert_eq!(*started.borrow(), [floor]);
    assert!(ended.borrow().is_empty());

    workspace
        .get_mut::<Part>(ball)
        .unwrap()
        .with_can_collide(false);
    workspace.update(DT);
    workspace.update(DT);
    assert_eq!(*ended.borrow(), [floor]);
    workspace
        .get_mut::<Part>(ball)
        .unwrap()
        .with_can_collide(true);
    workspace.update(DT);
    assert_eq!(*started.borrow(), [floor, floor]);
}

#[test]
fn removing_a_touching_part_notifies_the_survivor_and_closes_its_signals() {
    use std::{
        future::Future,
        pin::Pin,
        task::{Context, Poll, Waker},
    };

    let (mut workspace, floor, ball) = contact_scene();
    let ended = received(workspace.get::<BasePart>(floor).unwrap().on_touch_ended());
    let signal = workspace.get::<Part>(ball).unwrap().on_touch_ended();
    let connection = signal.connect(|_, _| panic!("destroyed part received a new contact"));
    let mut wait = signal.wait();
    workspace.update(DT);
    assert!(workspace.remove_child(ball));
    assert!(signal.is_closed());
    assert!(!connection.is_connected());
    assert_eq!(
        Pin::new(&mut wait).poll(&mut Context::from_waker(Waker::noop())),
        Poll::Ready(Err(SignalClosed))
    );
    workspace.update(DT);
    workspace.update(DT);
    assert_eq!(*ended.borrow(), [ball]);
    assert!(workspace.instance(ball).is_none());
}

#[test]
fn collision_callbacks_can_destroy_parts_after_transforms_are_synchronized() {
    let (mut workspace, floor, ball) = contact_scene();
    let ball_started = received(workspace.get::<Part>(ball).unwrap().on_touched());
    let floor_ended = received(workspace.get::<BasePart>(floor).unwrap().on_touch_ended());
    workspace
        .get::<BasePart>(floor)
        .unwrap()
        .on_touched()
        .connect(move |context, other| {
            assert_eq!(*other, ball);
            let position = context.workspace().get::<Part>(ball).unwrap().position();
            let body = context.workspace().physics().body(ball).unwrap();
            assert_eq!(position.to_array(), body.translation().to_array());
            assert!(context.workspace_mut().remove_child(ball));
        });

    workspace.update(DT);
    assert!(workspace.instance(ball).is_none());
    // Already queued contact delivery survives destruction, as other instance signals do.
    assert_eq!(*ball_started.borrow(), [floor]);
    workspace.update(DT);
    assert_eq!(*floor_ended.borrow(), [ball]);
}

#[test]
fn contact_delivery_respects_dispatch_budget_and_cancellation() {
    let (mut workspace, floor, ball) = contact_scene();
    let event = workspace.bindable_event::<()>();
    event.on_event().connect(|_, _| {});
    for _ in 0..1_024 {
        event.fire(());
    }
    let signal = workspace.get::<Part>(ball).unwrap().on_touched();
    let seen = received(signal.clone());
    let cancelled = signal.connect(|_, _| panic!("cancelled contact ran"));
    workspace.update(DT);
    assert!(seen.borrow().is_empty());
    assert!(workspace.has_pending_events());
    cancelled.disconnect();
    workspace.dispatch_events();
    assert_eq!(*seen.borrow(), [floor]);
    assert!(!workspace.has_pending_events());
}

#[test]
fn detached_contact_subscriptions_support_once_and_async_waits() {
    use std::{
        future::Future,
        pin::Pin,
        task::{Context, Poll, Waker},
    };

    let (mut workspace, floor, original_ball) = contact_scene();
    workspace.remove_child(original_ball);
    let ball = Part::new()
        .with_shape(PartShape::Ball)
        .with_anchored(false)
        .with_position(Vec3::new(0.0, 0.9, 0.0));
    let ball_id = ball.id();
    let signal = ball.on_touched();
    let mut wait = signal.wait();
    let once_seen = Rc::new(RefCell::new(Vec::new()));
    let once_recorded = once_seen.clone();
    let once = signal.once(move |_, other| once_recorded.borrow_mut().push(*other));
    workspace.add_child(ball);
    workspace.update(DT);
    assert_eq!(*once_seen.borrow(), [floor]);
    assert!(!once.is_connected());
    assert_eq!(
        Pin::new(&mut wait).poll(&mut Context::from_waker(Waker::noop())),
        Poll::Ready(Ok(floor))
    );
    workspace
        .get_mut::<Part>(ball_id)
        .unwrap()
        .with_can_collide(false);
    workspace.update(DT);
    workspace
        .get_mut::<Part>(ball_id)
        .unwrap()
        .with_can_collide(true);
    workspace.update(DT);
    assert_eq!(*once_seen.borrow(), [floor]);
}

#[test]
fn invalid_or_zero_deltas_do_not_produce_contact_transitions() {
    let (mut workspace, floor, ball) = contact_scene();
    let started = received(workspace.get::<Part>(ball).unwrap().on_touched());
    let ended = received(workspace.get::<Part>(ball).unwrap().on_touch_ended());
    for delta in [0.0, -1.0, f32::NAN, f32::INFINITY] {
        workspace.update(delta);
    }
    assert!(started.borrow().is_empty());
    workspace.update(DT);
    assert_eq!(*started.borrow(), [floor]);
    workspace
        .get_mut::<Part>(ball)
        .unwrap()
        .with_can_collide(false);
    workspace.update(0.0);
    assert!(ended.borrow().is_empty());
    workspace.update(DT);
    assert_eq!(*ended.borrow(), [floor]);
}

#[test]
fn anchored_pairs_and_disabled_colliders_do_not_emit_contacts() {
    let (mut workspace, _, ball) = contact_scene();
    let started = received(workspace.get::<Part>(ball).unwrap().on_touched());
    workspace.get_mut::<Part>(ball).unwrap().with_anchored(true);
    workspace.update(DT);
    assert!(started.borrow().is_empty());
    workspace
        .get_mut::<Part>(ball)
        .unwrap()
        .with_anchored(false)
        .with_can_collide(false);
    workspace.update(DT);
    assert!(started.borrow().is_empty());
    workspace
        .get_mut::<Part>(ball)
        .unwrap()
        .with_can_collide(true);
    workspace.update(DT);
    assert_eq!(started.borrow().len(), 1);
}

#[test]
fn scene_clones_have_fresh_contact_signals_and_contact_state() {
    let (mut workspace, _, ball) = contact_scene();
    let original = received(workspace.get::<Part>(ball).unwrap().on_touched());
    workspace.update(DT);
    let mut clone = workspace.clone();
    let cloned_ball = clone.get_all::<Part>().next().unwrap();
    let cloned = received(cloned_ball.on_touched());
    let cloned_floor = clone.get_all::<BasePart>().next().unwrap().id();
    clone.update(DT);
    assert_eq!(*cloned.borrow(), [cloned_floor]);
    assert_eq!(original.borrow().len(), 1);
}

#[cfg(feature = "meshpart")]
#[test]
fn mesh_parts_emit_contacts_using_their_box_colliders() {
    use terrarium::{Mesh, MeshPart};

    let (mut workspace, floor, ball) = contact_scene();
    workspace.remove_child(ball);
    let mesh = MeshPart::new(Mesh::block(1.0, [1.0; 4]))
        .with_anchored(false)
        .with_position(Vec3::new(0.0, 0.9, 0.0));
    let mesh_id = mesh.id();
    let started = received(mesh.on_touched());
    let ended = received(mesh.on_touch_ended());
    workspace.add_child(mesh);
    workspace.update(DT);
    assert_eq!(*started.borrow(), [floor]);
    workspace
        .get_mut::<MeshPart>(mesh_id)
        .unwrap()
        .with_position(Vec3::new(0.0, 4.0, 0.0));
    workspace.update(DT);
    assert_eq!(*ended.borrow(), [floor]);
}
