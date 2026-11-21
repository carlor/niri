use niri_config::Action;
use smithay::backend::input::InputTime;
use wayland_client::protocol::wl_surface::WlSurface;

use super::client::ClientId;
use super::Fixture;
use crate::layout::LayoutElement as _;
use crate::window::mapped::MappedId;

fn open(f: &mut Fixture, client: ClientId) -> (WlSurface, MappedId) {
    let window = f.client(client).create_window();
    let surface = window.surface.clone();
    window.commit();
    f.roundtrip(client);
    let window = f.client(client).window(&surface);
    window.attach_new_buffer();
    window.set_size(300, 200);
    window.ack_last_and_commit();
    f.double_roundtrip(client);
    let id = f
        .niri()
        .layout
        .windows()
        .map(|(_, w)| w.id())
        .max_by_key(|id| id.get())
        .unwrap();
    (surface, id)
}

fn action(f: &mut Fixture, client: ClientId, action: Action) {
    f.niri_state().do_action(action, false, InputTime::now());
    f.double_roundtrip(client);
}

fn hidden(f: &mut Fixture) -> Vec<MappedId> {
    f.niri()
        .minimized_windows
        .iter()
        .map(|r| r.window().id())
        .collect()
}

fn ipc_window(f: &mut Fixture, id: MappedId) -> niri_ipc::Window {
    f.niri()
        .ipc_server
        .as_ref()
        .unwrap()
        .windows()
        .into_iter()
        .find(|w| w.id == id.get())
        .unwrap()
}

#[test]
fn minimize_and_restore_preserve_id_and_update_ipc() {
    let mut f = Fixture::new();
    f.add_output(1, (1920, 1080));
    let client = f.add_client();
    let (surface, id) = open(&mut f, client);

    action(&mut f, client, Action::MinimizeWindow);
    assert_eq!(hidden(&mut f), [id]);
    assert_eq!(f.niri().layout.windows().count(), 0);
    assert!(f.niri().layout.focus().is_none());
    let win = ipc_window(&mut f, id);
    assert!(win.is_minimized);
    assert!(!win.is_focused);
    assert_eq!(win.workspace_id, None);

    // Client commits and title updates must not accidentally remap the window.
    let window = f.client(client).window(&surface);
    window.set_title("still hidden");
    window.attach_new_buffer();
    window.ack_last_and_commit();
    f.double_roundtrip(client);
    assert_eq!(f.niri().layout.windows().count(), 0);
    assert_eq!(hidden(&mut f), [id]);
    assert_eq!(
        ipc_window(&mut f, id).title.as_deref(),
        Some("still hidden")
    );

    action(&mut f, client, Action::RestoreWindow(None));
    assert!(hidden(&mut f).is_empty());
    assert_eq!(f.niri().layout.focus().unwrap().id(), id);
    assert!(!ipc_window(&mut f, id).is_minimized);
    assert!(ipc_window(&mut f, id).is_focused);
}

#[test]
fn restore_by_id_and_most_recent_order() {
    let mut f = Fixture::new();
    f.add_output(1, (1920, 1080));
    let client = f.add_client();
    let (_, first) = open(&mut f, client);
    let (_, second) = open(&mut f, client);
    let (_, third) = open(&mut f, client);

    for id in [first, second, third] {
        action(&mut f, client, Action::MinimizeWindowById(id.get()));
    }
    assert_eq!(hidden(&mut f), [first, second, third]);
    // Duplicate minimize and unknown IDs are harmless and don't reorder the stack.
    action(&mut f, client, Action::MinimizeWindowById(first.get()));
    action(&mut f, client, Action::RestoreWindow(Some(u64::MAX)));
    assert_eq!(hidden(&mut f), [first, second, third]);

    action(&mut f, client, Action::RestoreWindow(Some(second.get())));
    assert_eq!(hidden(&mut f), [first, third]);
    assert_eq!(f.niri().layout.focus().unwrap().id(), second);
    action(&mut f, client, Action::RestoreWindow(None));
    assert_eq!(f.niri().layout.focus().unwrap().id(), third);
    action(&mut f, client, Action::FocusWindow(first.get()));
    assert_eq!(f.niri().layout.focus().unwrap().id(), first);
    action(&mut f, client, Action::RestoreWindow(None));
    assert!(hidden(&mut f).is_empty());
}

#[test]
fn client_minimize_request_and_focus_transfer() {
    let mut f = Fixture::new();
    f.add_output(1, (1920, 1080));
    let client = f.add_client();
    let (_, first) = open(&mut f, client);
    let (surface, second) = open(&mut f, client);
    f.client(client)
        .window(&surface)
        .xdg_toplevel
        .set_minimized();
    f.double_roundtrip(client);
    assert_eq!(hidden(&mut f), [second]);
    assert_eq!(f.niri().layout.focus().unwrap().id(), first);
}

#[test]
fn restore_in_same_cycle_keeps_focus_state_consistent() {
    let mut f = Fixture::new();
    f.add_output(1, (1920, 1080));
    let client = f.add_client();
    let (_, id) = open(&mut f, client);
    let state = f.niri_state();
    state.do_action(Action::MinimizeWindow, false, InputTime::now());
    state.do_action(Action::RestoreWindow(None), false, InputTime::now());
    f.double_roundtrip(client);
    assert_eq!(f.niri().layout.focus().unwrap().id(), id);
    assert!(ipc_window(&mut f, id).is_focused);
}

#[test]
fn destroyed_minimized_window_is_forgotten() {
    let mut f = Fixture::new();
    f.add_output(1, (1920, 1080));
    let client = f.add_client();
    let (surface, id) = open(&mut f, client);
    action(&mut f, client, Action::MinimizeWindow);
    // Closing by ID must also work while hidden.
    action(&mut f, client, Action::CloseWindowById(id.get()));
    let window = f.client(client).window(&surface);
    assert!(window.close_requested);
    window.xdg_toplevel.destroy();
    window.xdg_surface.destroy();
    window.viewport.destroy();
    window.surface.destroy();
    f.double_roundtrip(client);
    assert!(hidden(&mut f).is_empty());
    assert!(f.niri().ipc_server.as_ref().unwrap().windows().is_empty());
    action(&mut f, client, Action::RestoreWindow(None));
    assert_eq!(f.niri().layout.windows().count(), 0);
}

#[test]
fn client_unmap_while_minimized_restarts_initial_configure() {
    let mut f = Fixture::new();
    f.add_output(1, (1920, 1080));
    let client = f.add_client();
    let (surface, _) = open(&mut f, client);
    action(&mut f, client, Action::MinimizeWindow);
    let window = f.client(client).window(&surface);
    window.attach_null();
    window.commit();
    f.double_roundtrip(client);
    assert!(hidden(&mut f).is_empty());
    assert_eq!(f.niri().unmapped_windows.len(), 1);
    f.client(client).window(&surface).commit();
    f.double_roundtrip(client);
    let window = f.client(client).window(&surface);
    window.attach_new_buffer();
    window.ack_last_and_commit();
    f.double_roundtrip(client);
    assert_eq!(f.niri().layout.windows().count(), 1);
}

#[test]
fn restore_on_current_workspace_after_output_removed() {
    let mut f = Fixture::new();
    f.add_output(1, (1920, 1080));
    f.add_output(2, (1280, 720));
    let client = f.add_client();
    let (_, id) = open(&mut f, client);
    let old_output = f.niri().layout.active_output().unwrap().clone();
    action(&mut f, client, Action::MinimizeWindow);
    f.niri().remove_output(&old_output);
    action(&mut f, client, Action::RestoreWindow(None));
    assert_eq!(f.niri().layout.focus().unwrap().id(), id);
    let (_, output) = f.niri().find_mapped_output_by_id(id).unwrap();
    assert_ne!(output, Some(&old_output));
}

#[test]
fn minimize_and_restore_without_outputs() {
    let mut f = Fixture::new();
    let client = f.add_client();
    let (_, id) = open(&mut f, client);
    action(&mut f, client, Action::MinimizeWindowById(id.get()));
    assert_eq!(f.niri().layout.windows().count(), 0);
    action(&mut f, client, Action::RestoreWindow(None));
    assert_eq!(f.niri().layout.windows().next().unwrap().1.id(), id);
    f.add_output(1, (1920, 1080));
    f.double_roundtrip(client);
    assert_eq!(f.niri().layout.focus().unwrap().id(), id);
}

#[test]
fn restore_preserves_floating_and_fullscreen_state() {
    for floating in [false, true] {
        for fullscreen in [false, true] {
            let mut f = Fixture::new();
            f.add_output(1, (1920, 1080));
            let client = f.add_client();
            let (surface, id) = open(&mut f, client);
            if floating {
                action(&mut f, client, Action::ToggleWindowFloating);
            }
            if fullscreen {
                action(&mut f, client, Action::FullscreenWindow);
            }
            f.client(client).window(&surface).ack_last_and_commit();
            f.double_roundtrip(client);
            action(&mut f, client, Action::MinimizeWindow);
            action(&mut f, client, Action::RestoreWindow(None));
            f.client(client).window(&surface).ack_last_and_commit();
            f.double_roundtrip(client);
            let mapped = f.niri().find_mapped_output_by_id(id).unwrap().0;
            assert_eq!(mapped.pending_sizing_mode().is_fullscreen(), fullscreen);
            if !fullscreen {
                assert_eq!(mapped.is_floating(), floating);
            }
            if fullscreen && floating {
                action(&mut f, client, Action::FullscreenWindow);
                f.client(client).window(&surface).ack_last_and_commit();
                f.double_roundtrip(client);
                assert!(f
                    .niri()
                    .find_mapped_output_by_id(id)
                    .unwrap()
                    .0
                    .is_floating());
            }
        }
    }
}

#[test]
fn sizing_requests_do_not_restore_minimized_windows() {
    let mut f = Fixture::new();
    f.add_output(1, (1920, 1080));
    let client = f.add_client();
    let (surface, id) = open(&mut f, client);
    action(&mut f, client, Action::MinimizeWindow);
    for request in 0..4 {
        let window = f.client(client).window(&surface);
        let previous_configures = window.configures_received.len();
        match request {
            0 => window.set_fullscreen(None),
            1 => window.unset_fullscreen(),
            2 => window.set_maximized(),
            _ => window.unset_maximized(),
        }
        f.double_roundtrip(client);
        let window = f.client(client).window(&surface);
        assert!(window.configures_received.len() > previous_configures);
        window.ack_last_and_commit();
        f.double_roundtrip(client);
        assert_eq!(hidden(&mut f), [id]);
        assert_eq!(f.niri().layout.windows().count(), 0);
    }
}

#[test]
fn foreign_toplevel_minimize_and_activate() {
    use crate::protocols::foreign_toplevel::ForeignToplevelHandler as _;

    let mut f = Fixture::new();
    f.add_output(1, (1920, 1080));
    let client = f.add_client();
    let (_, id) = open(&mut f, client);
    let surface = f
        .niri()
        .layout
        .focus()
        .unwrap()
        .toplevel()
        .wl_surface()
        .clone();
    f.niri_state().set_minimized(surface.clone());
    f.double_roundtrip(client);
    assert_eq!(hidden(&mut f), [id]);
    f.niri_state().unset_minimized(surface.clone());
    f.double_roundtrip(client);
    assert_eq!(f.niri().layout.focus().unwrap().id(), id);
    f.niri_state().set_minimized(surface.clone());
    f.double_roundtrip(client);
    f.niri_state().activate(surface);
    f.double_roundtrip(client);
    assert!(hidden(&mut f).is_empty());
    assert_eq!(f.niri().layout.focus().unwrap().id(), id);
}

#[test]
fn minimization_actions_parse() {
    niri_config::Config::parse_mem(
        r#"
        binds {
            Mod+M { minimize-window; }
            Mod+Shift+M { restore-window; }
            Mod+Ctrl+M { restore-window id=42; }
        }
    "#,
    )
    .unwrap();
    assert!(matches!(
        Action::from(niri_ipc::Action::MinimizeWindow { id: None }),
        Action::MinimizeWindow
    ));
    assert!(matches!(
        Action::from(niri_ipc::Action::MinimizeWindow { id: Some(42) }),
        Action::MinimizeWindowById(42)
    ));
    assert!(matches!(
        Action::from(niri_ipc::Action::RestoreWindow { id: None }),
        Action::RestoreWindow(None)
    ));
}
