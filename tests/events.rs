use std::{
    cell::{Cell, RefCell},
    future::Future,
    pin::Pin,
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    task::{Context, Poll, Waker},
};

use terrarium::{
    AncestryChanged, Connection, Instance, InstanceProperty, Part, SignalClosed, SignalWait,
    Workspace,
};

fn poll<T>(wait: &mut SignalWait<T>) -> Poll<Result<T, SignalClosed>> {
    Pin::new(wait).poll(&mut Context::from_waker(Waker::noop()))
}

#[test]
fn signals_are_deferred_broadcasts_without_replay() {
    let mut workspace = Workspace::new();
    let event = workspace.bindable_event::<u32>();
    let seen = Rc::new(RefCell::new(Vec::new()));
    event.fire(0).unwrap();
    for name in ["first", "second"] {
        let seen = seen.clone();
        // Discarding a connection handle intentionally leaves it subscribed.
        event
            .on_event()
            .connect(move |_, value| seen.borrow_mut().push((name, *value)))
            .detach();
    }
    event.fire(1).unwrap();
    let later = seen.clone();
    event
        .on_event()
        .clone()
        .connect(move |_, value| later.borrow_mut().push(("later", *value)))
        .detach();
    assert!(seen.borrow().is_empty());
    workspace.dispatch_events();
    assert_eq!(*seen.borrow(), [("first", 1), ("second", 1)]);
    event.fire(2).unwrap();
    workspace.update(0.0);
    assert_eq!(
        *seen.borrow(),
        [
            ("first", 1),
            ("second", 1),
            ("first", 2),
            ("second", 2),
            ("later", 2)
        ]
    );
}

#[test]
fn cleanup_preserves_live_subscriptions_pending_delivery_and_owner_shutdown() {
    for churn in [0, 1, 8, 64, 1_024] {
        let mut workspace = Workspace::new();
        let event = workspace.bindable_event::<u32>();
        let signal = event.on_event();
        let seen = Rc::new(RefCell::new(Vec::new()));
        let received = seen.clone();
        let connection = signal
            .connect(move |_, value| received.borrow_mut().push(*value))
            .detach();
        let mut delivered = signal.wait();
        event.fire(42).unwrap();

        for _ in 0..(2 * churn + 17) {
            drop(workspace.bindable_event::<()>());
        }
        workspace.dispatch_events();
        assert_eq!(*seen.borrow(), [42]);
        assert_eq!(poll(&mut delivered), Poll::Ready(Ok(42)));
        assert!(connection.is_connected());

        let mut closed = signal.wait();
        drop(workspace);
        assert!(signal.is_closed());
        assert!(!connection.is_connected());
        assert_eq!(poll(&mut closed), Poll::Ready(Err(SignalClosed)));
    }
}

#[test]
fn callbacks_can_own_their_scoped_connection_during_shutdown_or_disconnect() {
    for close_owner in [true, false] {
        let workspace = Workspace::new();
        let event = workspace.bindable_event::<()>();
        let holder = Rc::new(RefCell::new(None));
        let captured = holder.clone();
        let connection = event
            .on_event()
            .connect(move |_, _| {
                let _ = &captured;
            })
            .detach();
        *holder.borrow_mut() = Some(connection.clone().scoped());
        // The callback now owns the last reference to its scoped connection.
        drop(holder);
        assert!(connection.is_connected());
        if close_owner {
            drop(event);
        } else {
            connection.disconnect();
        }
        assert!(!connection.is_connected());
    }
}

#[test]
fn once_disconnects_before_callback_and_explicit_disconnect_cancels_pending_calls() {
    let mut workspace = Workspace::new();
    let event = workspace.bindable_event::<u32>();
    let seen = Rc::new(RefCell::new(Vec::new()));
    let connection = Rc::new(RefCell::new(None::<Connection>));
    let current = connection.clone();
    let received = seen.clone();
    let captured = String::from("one-shot capture");
    *connection.borrow_mut() = Some(
        event
            .on_event()
            .once(move |_, value| {
                drop(captured);
                assert!(!current.borrow().as_ref().unwrap().is_connected());
                received.borrow_mut().push(*value);
            })
            .detach(),
    );
    let cancelled = event
        .on_event()
        .connect(|_, _| panic!("disconnected callback ran"))
        .detach();
    event.fire(1).unwrap();
    event.fire(2).unwrap();
    cancelled.disconnect();
    cancelled.disconnect();
    workspace.dispatch_events();
    assert_eq!(*seen.borrow(), [1]);
    assert!(!cancelled.is_connected());
}

#[test]
fn subscriptions_can_change_during_delivery() {
    let mut workspace = Workspace::new();
    let event = workspace.bindable_event::<u32>();
    let seen = Rc::new(RefCell::new(Vec::new()));
    let cancelled = Rc::new(RefCell::new(None::<Connection>));
    let other = cancelled.clone();
    let signal = event.on_event();
    let received = seen.clone();
    event
        .on_event()
        .once(move |_, value| {
            received.borrow_mut().push(("initial", *value));
            other.borrow().as_ref().unwrap().disconnect();
            let received = received.clone();
            signal
                .connect(move |_, value| received.borrow_mut().push(("new", *value)))
                .detach();
        })
        .detach();
    *cancelled.borrow_mut() = Some(
        event
            .on_event()
            .connect(|_, _| panic!("cancelled during dispatch"))
            .detach(),
    );
    event.fire(1).unwrap();
    event.fire(2).unwrap();
    workspace.dispatch_events();
    assert_eq!(*seen.borrow(), [("initial", 1)]);
    event.fire(3).unwrap();
    workspace.dispatch_events();
    assert_eq!(*seen.borrow(), [("initial", 1), ("new", 3)]);
}

#[test]
fn scoped_connections_and_self_disconnection_cancel_queued_delivery() {
    let mut workspace = Workspace::new();
    let event = workspace.bindable_event::<()>();
    let scoped = event
        .on_event()
        .connect(|_, _| panic!("scoped connection outlived guard"))
        .detach()
        .scoped();
    event.fire(()).unwrap();
    drop(scoped);

    let connection = Rc::new(RefCell::new(None::<Connection>));
    let own = connection.clone();
    let seen = Rc::new(Cell::new(0));
    let received = seen.clone();
    *connection.borrow_mut() = Some(
        event
            .on_event()
            .connect(move |_, _| {
                received.set(received.get() + 1);
                own.borrow().as_ref().unwrap().disconnect();
            })
            .detach(),
    );
    event.fire(()).unwrap();
    event.fire(()).unwrap();
    workspace.dispatch_events();
    assert_eq!(seen.get(), 1);
}

#[test]
fn nested_events_are_queued_and_dispatch_work_is_bounded() {
    let mut workspace = Workspace::new();
    let event = Rc::new(workspace.bindable_event::<u32>());
    let source = Rc::downgrade(&event);
    let seen = Rc::new(RefCell::new(Vec::new()));
    let received = seen.clone();
    event
        .on_event()
        .connect(move |context, value| {
            received.borrow_mut().push(*value);
            source.upgrade().unwrap().fire(value + 1).unwrap();
            assert_eq!(context.workspace_mut().dispatch_events(), 0);
        })
        .detach();
    event.fire(0).unwrap();
    assert_eq!(workspace.dispatch_events_with_limit(0), 0);
    assert_eq!(workspace.dispatch_events_with_limit(3), 3);
    assert_eq!(*seen.borrow(), [0, 1, 2]);
    assert!(workspace.has_pending_events());
    workspace.dispatch_events_with_limit(2);
    assert_eq!(*seen.borrow(), [0, 1, 2, 3, 4]);
}

#[test]
fn hierarchy_signals_include_nested_changes_and_subtrees() {
    let mut workspace = Workspace::new();
    let added = Rc::new(RefCell::new(Vec::new()));
    let removed = Rc::new(RefCell::new(Vec::new()));
    let direct = Rc::new(RefCell::new(Vec::new()));
    let received = added.clone();
    workspace
        .on_descendant_added()
        .connect(move |context, id| {
            assert!(context.workspace().instance(id.id).is_some());
            received.borrow_mut().push(id.id);
        })
        .detach();
    let received = removed.clone();
    workspace
        .on_descendant_removing()
        .connect(move |context, id| {
            assert!(context.workspace().instance(id.id).is_none());
            received.borrow_mut().push(id.id);
        })
        .detach();
    let received = direct.clone();
    workspace
        .on_child_added()
        .connect(move |_, id| received.borrow_mut().push(id.id))
        .detach();
    let parent = workspace.add_child(Part::new());
    let mut subtree = Part::new();
    let leaf = subtree.add_child(Part::new());
    let child = workspace
        .get_mut::<Part>(parent)
        .unwrap()
        .add_child(subtree);
    workspace.dispatch_events();
    assert_eq!(*added.borrow(), [parent, child, leaf]);
    assert_eq!(*direct.borrow(), [parent]);
    assert!(workspace.instance_mut(child).unwrap().destroy());
    workspace.dispatch_events();
    assert_eq!(*removed.borrow(), [child, leaf]);
    assert!(workspace.instance(parent).is_some());
}

#[test]
fn signals_can_be_connected_before_attachment_and_ancestry_reaches_descendants() {
    let mut workspace = Workspace::new();
    let mut parent = Part::new();
    let leaf = Part::new();
    let leaf_id = leaf.id();
    let parent_id = parent.id();
    let ancestry = Rc::new(RefCell::new(Vec::new()));
    let received = ancestry.clone();
    leaf.on_ancestry_changed()
        .connect(move |_, event| received.borrow_mut().push(*event))
        .detach();
    parent.add_child(leaf);
    assert!(!workspace.has_pending_events());
    workspace.add_child(parent);
    workspace.dispatch_events();
    assert_eq!(
        *ancestry.borrow(),
        [AncestryChanged {
            instance: leaf_id,
            parent: Some(parent_id)
        }]
    );
    workspace.remove_child(parent_id);
    workspace.dispatch_events();
    assert_eq!(ancestry.borrow().len(), 2);
    assert_eq!(
        ancestry.borrow()[1],
        AncestryChanged {
            instance: leaf_id,
            parent: Some(parent_id)
        }
    );
}

#[test]
fn name_and_parent_notifications_are_filtered_and_ignore_unchanged_values() {
    let mut workspace = Workspace::new();
    let mut part = Part::new();
    let changes = Rc::new(RefCell::new(Vec::new()));
    let names = Rc::new(RefCell::new(Vec::new()));
    let received = changes.clone();
    part.on_changed()
        .connect(move |_, property| received.borrow_mut().push(*property))
        .detach();
    let received = names.clone();
    let id = part.id();
    part.on_property_changed(InstanceProperty::Name)
        .connect(move |context, _| {
            received
                .borrow_mut()
                .push(context.workspace().instance(id).unwrap().name().to_owned());
        })
        .detach();
    part.set_name("detached".to_owned());
    workspace.add_child(part);
    workspace
        .instance_mut(id)
        .unwrap()
        .set_name("attached".to_owned());
    workspace
        .instance_mut(id)
        .unwrap()
        .set_name("attached".to_owned());
    workspace.dispatch_events();
    assert_eq!(
        *changes.borrow(),
        [InstanceProperty::Parent, InstanceProperty::Name]
    );
    assert_eq!(*names.borrow(), ["attached"]);
}

#[test]
fn attribute_invalidations_coalesce_updates_removal_and_clear() {
    let mut workspace = Workspace::new();
    let id = workspace.add_child(Part::new());
    let changes = Rc::new(RefCell::new(Vec::new()));
    let received = changes.clone();
    workspace
        .instance(id)
        .unwrap()
        .on_attribute_changed()
        .connect(move |_, name| received.borrow_mut().push(name.clone()))
        .detach();
    let health = Rc::new(Cell::new(0));
    let received = health.clone();
    workspace
        .instance(id)
        .unwrap()
        .on_attribute_changed_for("health")
        .connect(move |_, _| received.set(received.get() + 1))
        .detach();
    let part = workspace.get_mut::<Part>(id).unwrap();
    part.set_attribute("health".to_owned(), serde_json::json!(100));
    part.set_attribute("health".to_owned(), serde_json::json!(100));
    part.set_attribute("health".to_owned(), serde_json::json!(50));
    part.remove_attribute("missing");
    part.remove_attribute("health");
    part.set_attribute("z".to_owned(), serde_json::json!(true));
    part.set_attribute("a".to_owned(), serde_json::json!(true));
    part.clear_attributes();
    part.clear_attributes();
    workspace.dispatch_events();
    assert_eq!(*changes.borrow(), ["health", "z", "a"]);
    assert_eq!(health.get(), 1);
}

#[test]
fn callbacks_can_destroy_their_source_without_invalidating_queued_callbacks() {
    let mut workspace = Workspace::new();
    let id = workspace.add_child(Part::new());
    let signal = workspace
        .instance(id)
        .unwrap()
        .on_property_changed(InstanceProperty::Name);
    let seen = Rc::new(RefCell::new(Vec::new()));
    let received = seen.clone();
    let first = signal
        .connect(move |context, _| {
            received
                .borrow_mut()
                .push(context.workspace().instance(id).is_some());
            assert!(context.workspace_mut().instance_mut(id).unwrap().destroy());
        })
        .detach();
    let received = seen.clone();
    let second = signal
        .connect(move |context, _| {
            received
                .borrow_mut()
                .push(context.workspace().instance(id).is_some())
        })
        .detach();
    workspace
        .instance_mut(id)
        .unwrap()
        .set_name("changed".to_owned());
    workspace.dispatch_events();
    assert_eq!(*seen.borrow(), [true, false]);
    assert!(!first.is_connected());
    assert!(!second.is_connected());
    assert!(signal.is_closed());
    assert!(!signal.connect(|_, _| {}).detach().is_connected());
}

#[test]
fn destruction_delivers_final_notifications_but_explicit_disconnect_can_cancel_them() {
    let mut workspace = Workspace::new();
    let mut parent = Part::new();
    let child = parent.add_child(Part::new());
    let parent = workspace.add_child(parent);
    let destroyed = Rc::new(RefCell::new(Vec::new()));
    for id in [parent, child] {
        let received = destroyed.clone();
        workspace
            .instance(id)
            .unwrap()
            .on_destroying()
            .connect(move |context, _| {
                assert!(context.workspace().instance(id).is_none());
                received.borrow_mut().push(id);
            })
            .detach();
    }
    let cancelled = workspace
        .instance(parent)
        .unwrap()
        .on_destroying()
        .connect(|_, _| panic!("explicitly cancelled destruction"))
        .detach();
    let removed = Rc::new(RefCell::new(Vec::new()));
    let received = removed.clone();
    workspace
        .on_child_removed()
        .connect(move |_, id| received.borrow_mut().push(id.id))
        .detach();
    workspace.remove_child(parent);
    assert!(!cancelled.is_connected());
    cancelled.disconnect();
    workspace.dispatch_events();
    assert_eq!(*destroyed.borrow(), [parent, child]);
    assert_eq!(*removed.borrow(), [parent]);
}

#[test]
fn scene_clones_do_not_inherit_listeners_or_pending_events() {
    let mut workspace = Workspace::new();
    let received = Rc::new(RefCell::new(Vec::new()));
    let seen = received.clone();
    workspace
        .on_child_added()
        .connect(move |_, id| seen.borrow_mut().push(id.id))
        .detach();
    let original = workspace.add_child(Part::new());
    let mut cloned = workspace.clone();
    cloned.add_child(Part::new());
    cloned.dispatch_events();
    assert!(received.borrow().is_empty());
    workspace.dispatch_events();
    assert_eq!(*received.borrow(), [original]);
}

#[test]
fn waits_complete_at_dispatch_and_handle_closed_owners() {
    let mut workspace = Workspace::new();
    let event = workspace.bindable_event::<u32>();
    let mut wait = event.on_event().wait();
    assert_eq!(poll(&mut wait), Poll::Pending);
    event.fire(42).unwrap();
    assert_eq!(poll(&mut wait), Poll::Pending);
    drop(event);
    workspace.dispatch_events();
    assert_eq!(poll(&mut wait), Poll::Ready(Ok(42)));

    let part = Part::new();
    let signal = part.on_destroying();
    let mut wait = signal.wait();
    drop(part);
    assert_eq!(poll(&mut wait), Poll::Ready(Err(SignalClosed)));
    assert_eq!(poll(&mut signal.wait()), Poll::Ready(Err(SignalClosed)));
}

#[test]
fn dropping_a_wait_cancels_it_without_affecting_other_subscribers() {
    let mut workspace = Workspace::new();
    let event = workspace.bindable_event::<u32>();
    let cancelled = event.on_event().wait();
    let mut surviving = event.on_event().wait();
    event.fire(7).unwrap();
    drop(cancelled);
    workspace.dispatch_events();
    assert_eq!(poll(&mut surviving), Poll::Ready(Ok(7)));
}

#[test]
fn dropping_the_workspace_closes_external_bindable_events_and_wakes_waits() {
    let workspace = Workspace::new();
    let event = workspace.bindable_event::<u32>();
    let signal = event.on_event();
    let mut wait = signal.wait();
    event.fire(7).unwrap();
    drop(workspace);
    assert!(signal.is_closed());
    assert_eq!(poll(&mut wait), Poll::Ready(Err(SignalClosed)));
}

#[test]
fn waiting_tasks_are_woken_on_delivery_and_owner_shutdown() {
    struct WakeFlag(AtomicBool);

    impl std::task::Wake for WakeFlag {
        fn wake(self: Arc<Self>) {
            self.0.store(true, Ordering::SeqCst);
        }
    }

    let mut workspace = Workspace::new();
    let event = workspace.bindable_event::<u32>();
    let flag = Arc::new(WakeFlag(AtomicBool::new(false)));
    let waker = Waker::from(flag.clone());
    let mut context = Context::from_waker(&waker);
    let mut delivered = event.on_event().wait();
    assert_eq!(Pin::new(&mut delivered).poll(&mut context), Poll::Pending);
    event.fire(9).unwrap();
    assert!(!flag.0.load(Ordering::SeqCst));
    workspace.dispatch_events();
    assert!(flag.0.swap(false, Ordering::SeqCst));
    assert_eq!(poll(&mut delivered), Poll::Ready(Ok(9)));

    let mut closed = event.on_event().wait();
    assert_eq!(Pin::new(&mut closed).poll(&mut context), Poll::Pending);
    drop(workspace);
    assert!(flag.0.load(Ordering::SeqCst));
    assert_eq!(poll(&mut closed), Poll::Ready(Err(SignalClosed)));
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn dispatch_recovers_after_a_callback_panics() {
    let mut workspace = Workspace::new();
    let event = workspace.bindable_event::<u32>();
    let panicked = Rc::new(Cell::new(false));
    let first = panicked.clone();
    event
        .on_event()
        .connect(move |_, _| {
            if !first.replace(true) {
                panic!("intentional callback panic");
            }
        })
        .detach();
    let seen = Rc::new(RefCell::new(Vec::new()));
    let received = seen.clone();
    event
        .on_event()
        .connect(move |_, value| received.borrow_mut().push(*value))
        .detach();
    event.fire(1).unwrap();
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| workspace.dispatch_events()))
            .is_err()
    );
    event.fire(2).unwrap();
    workspace.dispatch_events();
    assert_eq!(*seen.borrow(), [1, 2]);
}

#[test]
fn ordinary_bursts_drain_in_one_update_without_a_growing_backlog() {
    let mut workspace = Workspace::new();
    let event = workspace.bindable_event::<usize>();
    let mut received = event.on_event().receive();
    for tick in 0..3 {
        for value in 0..1_200 {
            event.fire(tick * 1_200 + value).unwrap();
        }
        workspace.update(0.0);
        assert_eq!(
            received.drain().collect::<Vec<_>>(),
            (tick * 1_200..(tick + 1) * 1_200).collect::<Vec<_>>()
        );
        assert!(!workspace.has_pending_events());
    }
    assert_eq!(received.missed_emissions(), 0);
}

#[test]
fn cancelled_delivery_does_not_delay_unrelated_work() {
    let mut workspace = Workspace::new();
    let noise = workspace.bindable_event::<()>();
    let cancelled = noise
        .on_event()
        .connect(|_, _| panic!("cancelled callback ran"));
    for _ in 0..2_048 {
        noise.fire(()).unwrap();
    }
    drop(cancelled);
    let event = workspace.bindable_event::<u32>();
    let mut received = event.on_event().receive();
    event.fire(7).unwrap();
    assert_eq!(workspace.dispatch_events_with_limit(1), 1);
    assert_eq!(received.try_recv(), Some(7));
    assert!(!workspace.has_pending_events());
}

#[test]
fn callback_queue_overflow_rejects_whole_emissions_and_recovers() {
    let mut workspace = Workspace::new();
    let event = workspace.bindable_event::<usize>();
    let signal = event.on_event();
    let mut first = signal.receive();
    let mut second = signal.receive();
    let mut accepted = 0;
    let full = loop {
        match event.fire(accepted) {
            Ok(()) => accepted += 1,
            Err(full) => break full,
        }
    };
    assert_eq!(accepted * 2, full.capacity);
    assert_eq!(signal.missed_emissions(), 1);
    assert_eq!(first.missed_emissions(), 1);
    workspace.dispatch_events();
    let expected: Vec<_> = (0..accepted).collect();
    assert_eq!(first.drain().collect::<Vec<_>>(), expected);
    assert_eq!(second.drain().collect::<Vec<_>>(), expected);
    event.fire(accepted).unwrap();
    workspace.dispatch_events();
    assert_eq!(first.try_recv(), Some(accepted));
    assert_eq!(second.try_recv(), Some(accepted));
}

#[test]
fn receivers_bound_unread_values_and_preserve_final_delivery_after_close() {
    let mut workspace = Workspace::new();
    let event = workspace.bindable_event::<usize>();
    let mut received = event.on_event().receive();
    for start in [0, 2_500] {
        for value in start..start + 2_500 {
            event.fire(value).unwrap();
        }
        workspace.dispatch_events();
    }
    assert_eq!(received.missed_emissions(), 904);
    assert_eq!(
        received.drain().collect::<Vec<_>>(),
        (904..5_000).collect::<Vec<_>>()
    );
    event.fire(5_000).unwrap();
    drop(event);
    assert!(!received.is_closed());
    workspace.dispatch_events();
    assert!(received.is_closed());
    assert_eq!(received.try_recv(), Some(5_000));
    assert_eq!(received.try_recv(), None);
}

#[test]
fn dropping_default_subscription_guards_cancels_queued_callbacks() {
    let mut workspace = Workspace::new();
    let event = workspace.bindable_event::<()>();
    let seen = Rc::new(Cell::new(0));
    let captured = seen.clone();
    let subscription = event
        .on_event()
        .connect(move |_, _| captured.set(captured.get() + 1));
    event.fire(()).unwrap();
    drop(subscription);
    workspace.dispatch_events();
    event.fire(()).unwrap();
    workspace.dispatch_events();
    assert_eq!(seen.get(), 0);

    let captured = seen.clone();
    drop(
        event
            .on_event()
            .once(move |_, _| captured.set(captured.get() + 1)),
    );
    event.fire(()).unwrap();
    workspace.dispatch_events();
    assert_eq!(seen.get(), 0);
}

#[test]
fn lifecycle_snapshots_survive_add_and_remove_before_dispatch() {
    let mut workspace = Workspace::new();
    let mut added = workspace.on_descendant_added().receive();
    let mut removing = workspace.on_descendant_removing().receive();
    let mut removed = workspace.on_child_removed().receive();
    let mut parent = Part::new()
        .with_name("Enemy")
        .with_attribute("score", serde_json::json!(10));
    let child = parent.add_child(Part::new().with_name("Hitbox"));
    let mut destroying = parent.on_destroying().receive();
    let parent = workspace.add_child(parent);
    assert!(workspace.remove_child(parent));
    workspace.dispatch_events();
    assert!(workspace.instance(parent).is_none());
    let added: Vec<_> = added.drain().collect();
    let removing: Vec<_> = removing.drain().collect();
    assert_eq!(added, removing);
    assert_eq!(
        added.iter().map(|snapshot| snapshot.id).collect::<Vec<_>>(),
        [parent, child]
    );
    let snapshot = removed.try_recv().unwrap();
    assert_eq!(snapshot, added[0]);
    assert_eq!(snapshot.class_name, "Part");
    assert_eq!(snapshot.name, "Enemy");
    assert_eq!(snapshot.parent, Some(workspace.id()));
    assert_eq!(snapshot.children, [child]);
    assert_eq!(snapshot.attributes["score"], serde_json::json!(10));
    assert_eq!(destroying.try_recv(), Some(snapshot));
    assert!(destroying.is_closed());
}

#[test]
fn built_in_property_setters_coalesce_invalidations_without_duplicate_changes() {
    use terrarium::{
        Color3, Face, HasBasePart, HasMaterials, HasPVInstance, HasPart, Material, PartShape,
        glam::Vec3,
    };
    let mut workspace = Workspace::new();
    let id = workspace.add_child(Part::new());
    let part = workspace.get::<Part>(id).unwrap();
    let mut changes = part.on_changed().receive();
    let mut transforms = part
        .on_property_changed(InstanceProperty::Transform)
        .receive();
    let part = workspace.get_mut::<Part>(id).unwrap();
    for x in 1..=2_000 {
        part.with_position(Vec3::new(x as f32, 0.0, 0.0));
    }
    part.with_position(Vec3::new(2_000.0, 0.0, 0.0));
    part.with_color(Color3::RED)
        .with_color(Color3::RED)
        .with_transparency(0.5)
        .with_transparency(0.5)
        .with_anchored(false)
        .with_anchored(false)
        .with_can_collide(false)
        .with_can_collide(false)
        .with_shape(PartShape::Ball)
        .with_shape(PartShape::Ball)
        .with_material(Material::default())
        .with_material_slot(Face::Top, Material::default());
    workspace.dispatch_events();
    assert_eq!(
        changes.drain().collect::<Vec<_>>(),
        [
            InstanceProperty::Transform,
            InstanceProperty::Color,
            InstanceProperty::Transparency,
            InstanceProperty::Anchored,
            InstanceProperty::CanCollide,
            InstanceProperty::Shape,
            InstanceProperty::Material
        ]
    );
    assert_eq!(transforms.drain().count(), 1);
    assert_eq!(changes.missed_emissions(), 0);
    assert_eq!(
        workspace.get::<Part>(id).unwrap().position(),
        Vec3::new(2_000.0, 0.0, 0.0)
    );
}

#[test]
fn cloned_transforms_do_not_notify_the_original_instance() {
    use terrarium::{HasPVInstance, glam::Vec3};
    let mut workspace = Workspace::new();
    let id = workspace.add_child(Part::new());
    workspace
        .get_mut::<Part>(id)
        .unwrap()
        .with_position(Vec3::X);
    let part = workspace.get::<Part>(id).unwrap();
    let mut original = part
        .on_property_changed(InstanceProperty::Transform)
        .receive();
    let cloned = part.clone();
    let standalone = part.pv().clone();
    let _cloned = cloned.with_position(Vec3::Y);
    let _standalone = standalone.with_position(Vec3::Z);
    workspace.dispatch_events();
    assert_eq!(original.try_recv(), None);
    assert_eq!(workspace.get::<Part>(id).unwrap().position(), Vec3::X);
}

#[test]
fn coalesced_callbacks_can_change_the_same_property_during_delivery() {
    use terrarium::{HasPVInstance, glam::Vec3};
    let mut workspace = Workspace::new();
    let id = workspace.add_child(Part::new());
    let seen = Rc::new(RefCell::new(Vec::new()));
    let recorded = seen.clone();
    let _subscription = workspace
        .get::<Part>(id)
        .unwrap()
        .on_property_changed(InstanceProperty::Transform)
        .connect(move |context, _| {
            let position = context.workspace().get::<Part>(id).unwrap().position();
            recorded.borrow_mut().push(position);
            if position == Vec3::X {
                context
                    .workspace_mut()
                    .get_mut::<Part>(id)
                    .unwrap()
                    .with_position(Vec3::Y);
            }
        });
    workspace
        .get_mut::<Part>(id)
        .unwrap()
        .with_position(Vec3::X);
    workspace.dispatch_events();
    assert_eq!(*seen.borrow(), [Vec3::X, Vec3::Y]);
    assert!(!workspace.has_pending_events());
}
