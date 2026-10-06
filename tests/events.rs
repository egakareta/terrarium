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
    event.fire(0);
    for name in ["first", "second"] {
        let seen = seen.clone();
        // Discarding a connection handle intentionally leaves it subscribed.
        event
            .on_event()
            .connect(move |_, value| seen.borrow_mut().push((name, *value)));
    }
    event.fire(1);
    let later = seen.clone();
    event
        .on_event()
        .clone()
        .connect(move |_, value| later.borrow_mut().push(("later", *value)));
    assert!(seen.borrow().is_empty());
    workspace.dispatch_events();
    assert_eq!(*seen.borrow(), [("first", 1), ("second", 1)]);
    event.fire(2);
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
    for threshold in [0, 1, 8, 64, 1_024] {
        let mut workspace = Workspace::new();
        assert_eq!(workspace.signal_cleanup_threshold(), 64);
        workspace.set_signal_cleanup_threshold(threshold);
        assert_eq!(workspace.signal_cleanup_threshold(), threshold.max(1));
        let event = workspace.bindable_event::<u32>();
        let signal = event.on_event();
        let seen = Rc::new(RefCell::new(Vec::new()));
        let received = seen.clone();
        let connection = signal.connect(move |_, value| received.borrow_mut().push(*value));
        let mut delivered = signal.wait();
        event.fire(42);

        for _ in 0..(2 * threshold + 17) {
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
        let connection = event.on_event().connect(move |_, _| {
            let _ = &captured;
        });
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
    *connection.borrow_mut() = Some(event.on_event().once(move |_, value| {
        drop(captured);
        assert!(!current.borrow().as_ref().unwrap().is_connected());
        received.borrow_mut().push(*value);
    }));
    let cancelled = event
        .on_event()
        .connect(|_, _| panic!("disconnected callback ran"));
    event.fire(1);
    event.fire(2);
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
    event.on_event().once(move |_, value| {
        received.borrow_mut().push(("initial", *value));
        other.borrow().as_ref().unwrap().disconnect();
        let received = received.clone();
        signal.connect(move |_, value| received.borrow_mut().push(("new", *value)));
    });
    *cancelled.borrow_mut() = Some(
        event
            .on_event()
            .connect(|_, _| panic!("cancelled during dispatch")),
    );
    event.fire(1);
    event.fire(2);
    workspace.dispatch_events();
    assert_eq!(*seen.borrow(), [("initial", 1)]);
    event.fire(3);
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
        .scoped();
    event.fire(());
    drop(scoped);

    let connection = Rc::new(RefCell::new(None::<Connection>));
    let own = connection.clone();
    let seen = Rc::new(Cell::new(0));
    let received = seen.clone();
    *connection.borrow_mut() = Some(event.on_event().connect(move |_, _| {
        received.set(received.get() + 1);
        own.borrow().as_ref().unwrap().disconnect();
    }));
    event.fire(());
    event.fire(());
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
    event.on_event().connect(move |context, value| {
        received.borrow_mut().push(*value);
        source.upgrade().unwrap().fire(value + 1);
        assert_eq!(context.workspace_mut().dispatch_events(), 0);
    });
    event.fire(0);
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
    workspace.on_descendant_added().connect(move |context, id| {
        assert!(context.workspace().instance(*id).is_some());
        received.borrow_mut().push(*id);
    });
    let received = removed.clone();
    workspace
        .on_descendant_removing()
        .connect(move |context, id| {
            assert!(context.workspace().instance(*id).is_none());
            received.borrow_mut().push(*id);
        });
    let received = direct.clone();
    workspace
        .on_child_added()
        .connect(move |_, id| received.borrow_mut().push(*id));
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
        .connect(move |_, event| received.borrow_mut().push(*event));
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
        .connect(move |_, property| received.borrow_mut().push(*property));
    let received = names.clone();
    let id = part.id();
    part.on_property_changed(InstanceProperty::Name)
        .connect(move |context, _| {
            received
                .borrow_mut()
                .push(context.workspace().instance(id).unwrap().name().to_owned());
        });
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
fn attribute_notifications_cover_updates_removal_and_clear_without_false_changes() {
    let mut workspace = Workspace::new();
    let id = workspace.add_child(Part::new());
    let changes = Rc::new(RefCell::new(Vec::new()));
    let received = changes.clone();
    workspace
        .instance(id)
        .unwrap()
        .on_attribute_changed()
        .connect(move |_, name| received.borrow_mut().push(name.clone()));
    let health = Rc::new(Cell::new(0));
    let received = health.clone();
    workspace
        .instance(id)
        .unwrap()
        .on_attribute_changed_for("health")
        .connect(move |_, _| received.set(received.get() + 1));
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
    assert_eq!(
        *changes.borrow(),
        ["health", "health", "health", "z", "a", "a", "z"]
    );
    assert_eq!(health.get(), 3);
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
    let first = signal.connect(move |context, _| {
        received
            .borrow_mut()
            .push(context.workspace().instance(id).is_some());
        assert!(context.workspace_mut().instance_mut(id).unwrap().destroy());
    });
    let received = seen.clone();
    let second = signal.connect(move |context, _| {
        received
            .borrow_mut()
            .push(context.workspace().instance(id).is_some())
    });
    workspace
        .instance_mut(id)
        .unwrap()
        .set_name("changed".to_owned());
    workspace.dispatch_events();
    assert_eq!(*seen.borrow(), [true, false]);
    assert!(!first.is_connected());
    assert!(!second.is_connected());
    assert!(signal.is_closed());
    assert!(!signal.connect(|_, _| {}).is_connected());
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
            });
    }
    let cancelled = workspace
        .instance(parent)
        .unwrap()
        .on_destroying()
        .connect(|_, _| panic!("explicitly cancelled destruction"));
    let removed = Rc::new(RefCell::new(Vec::new()));
    let received = removed.clone();
    workspace
        .on_child_removed()
        .connect(move |_, id| received.borrow_mut().push(*id));
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
    workspace.set_signal_cleanup_threshold(8);
    let received = Rc::new(RefCell::new(Vec::new()));
    let seen = received.clone();
    workspace
        .on_child_added()
        .connect(move |_, id| seen.borrow_mut().push(*id));
    let original = workspace.add_child(Part::new());
    let mut cloned = workspace.clone();
    assert_eq!(cloned.signal_cleanup_threshold(), 8);
    cloned.set_signal_cleanup_threshold(128);
    assert_eq!(workspace.signal_cleanup_threshold(), 8);
    assert_eq!(cloned.signal_cleanup_threshold(), 128);
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
    event.fire(42);
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
    event.fire(7);
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
    event.fire(7);
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
    event.fire(9);
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
    event.on_event().connect(move |_, _| {
        if !first.replace(true) {
            panic!("intentional callback panic");
        }
    });
    let seen = Rc::new(RefCell::new(Vec::new()));
    let received = seen.clone();
    event
        .on_event()
        .connect(move |_, value| received.borrow_mut().push(*value));
    event.fire(1);
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| workspace.dispatch_events()))
            .is_err()
    );
    event.fire(2);
    workspace.dispatch_events();
    assert_eq!(*seen.borrow(), [1, 2]);
}
