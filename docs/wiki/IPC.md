You can communicate with the running niri instance over an IPC socket.
Check `niri msg --help` for available commands.

The `--json` flag prints the response in JSON, rather than formatted.
For example, `niri msg --json outputs`.

> [!TIP]
> If you're getting parsing errors from `niri msg` after upgrading niri, make sure that you've restarted niri itself.
> You might be trying to run a newer `niri msg` against an older `niri` compositor.

### Minimizing Windows

Hide the focused window without closing it:

```sh
niri msg action minimize-window
```

Use `--id ID` to minimize a particular window. Minimized windows retain their IDs and appear in
`niri msg windows` (or `niri msg --json windows`) with `is_minimized: true`, no workspace, and no focus.
They are absent from the layout, overview, and built-in window switcher.

List only hidden windows:

```sh
niri msg hidden-windows
niri msg --json hidden-windows
```

This returns the same window records as `windows`, filtered by `is_minimized`.
The normal `windows` query and window events continue to include this boolean for every window,
whether visible or hidden. At the socket level, use `"HiddenWindows"` to receive a
`HiddenWindows` response containing the filtered window list.
The human-readable lists show an explicit `Is minimized: yes/no` line; JSON uses `is_minimized`.

Restore the most recently minimized window, or a particular window by ID:

```sh
niri msg action restore-window
niri msg action restore-window --id 42
```

Restored windows open on the active workspace and receive focus. Their floating/fullscreen state and
column width are retained, but their original column position is not. Focusing a minimized window
by ID also restores it. Empty restore lists and unknown IDs are harmless no-ops.
The minimized list lasts only for the current compositor session; closed windows are removed from it.

Both actions can be bound in the config:

```kdl
binds {
    Mod+M { minimize-window; }
    Mod+Shift+M { restore-window; }
}
```

Application minimize requests and the wlr foreign-toplevel minimize/restore requests use the same
list. Minimizing and restoring produce `WindowOpenedOrChanged` events, not close/open events with
new IDs.

### Event Stream

<sup>Since: 0.1.9</sup>

While most niri IPC requests return a single response, the event stream request will make niri continuously stream events into the IPC connection until it is closed.
This is useful for implementing various bars and indicators that update as soon as something happens, without continuous polling.

The event stream IPC is designed to give you the complete current state up-front, then follow up with updates to that state.
This way, your state can never "desync" from niri, and you don't need to make any other IPC information requests.

Where reasonable, event stream state updates are atomic, though this is not always the case.
For example, a window may end up with a workspace id for a workspace that had already been removed.
This can happen if the corresponding workspaces-changed event arrives before the corresponding window-changed event.

To get a taste of the events, run `niri msg event-stream`.
Though, this is more of a debug function than anything.
You can get raw events from `niri msg --json event-stream`, or by connecting to the niri socket and requesting an event stream manually.

You can find the full list of events along with documentation [here](https://niri-wm.github.io/niri/niri_ipc/enum.Event.html).

### Programmatic Access

`niri msg --json` is a thin wrapper over writing and reading to a socket.
When implementing more complex scripts and modules, you're encouraged to access the socket directly.

Connect to the UNIX domain socket located at `$NIRI_SOCKET` in the filesystem.
Write your request encoded in JSON on a single line, followed by a newline character, or by flushing and shutting down the write end of the connection.
Read the reply as JSON, also on a single line.

You can use `socat` to test communicating with niri directly:

```sh
$ socat STDIO "$NIRI_SOCKET"
"FocusedWindow"
{"Ok":{"FocusedWindow":{"id":12,"title":"t socat STDIO /run/u ~","app_id":"Alacritty","workspace_id":6,"is_focused":true}}}
```

The reply is an `Ok` or an `Err` wrapping the same JSON object as you get from `niri msg --json`.

<sup>Since: next release</sup>
For more complex requests, you can pass `--print-request` to `niri msg` to see how the request should be formatted:

```sh
$ niri msg --print-request action focus-workspace 2
{"Action":{"FocusWorkspace":{"reference":{"Index":2}}}}
```

This is the format that you should use for communicating with the niri socket directly.
You can also use `niri msg raw-request` to send a raw JSON request without `socat`:

```sh
$ echo '{"Action":{"FocusWorkspace":{"reference":{"Id":8}}}}' | niri msg raw-request
"Handled"
```

You can find all available requests and response types in the [niri-ipc sub-crate documentation](https://niri-wm.github.io/niri/niri_ipc/).

### Backwards Compatibility

The JSON output *should* remain stable, as in:

- existing fields and enum variants should not be renamed
- non-optional existing fields should not be removed

However, new fields and enum variants will be added, so you should handle unknown fields or variants gracefully where reasonable.

The formatted/human-readable output (i.e. without `--json` flag) is **not** considered stable.
Please prefer the JSON output for scripts, since I reserve the right to make any changes to the human-readable output.

The `niri-ipc` sub-crate (like other niri sub-crates) is *not* API-stable in terms of the Rust semver; rather, it follows the version of niri itself.
In particular, new struct fields and enum variants will be added.
