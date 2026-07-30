# Resize livelock: table columns update only after the mouse stops

## Symptom

Dragging a column resize handle in `guinea::widgets::table` does not update the
table while the mouse is moving. The column width changes only when the user
stops moving or releases the button. Rapid press-release cycles, however, do
trigger an immediate update.

## What we measured

Instrumentation added to `uniproc` (`tracing::info!(target: "bench")`) shows:

| Stage | Time |
|-------|------|
| `Signal::set` (column width) | 0.1–0.5 ms |
| `ReactiveMap::set_or_create` (persist width) | 0.04–0.25 ms |
| `table()` tree build | 0.1–0.5 ms |
| `Processes::view` total | 4–7 ms (peaks 19–24 ms) |
| `render_complete` reconcile | 5–10 ms (peaks 18–29 ms) |

After removing per-cell `on_tapped` callbacks and switching row selection to
`ListView`'s `selected_index` / `on_selection_changed`, the reconciler skips
most rows during a pure resize (`diffed=47, skipped=67` instead of
`diffed=784, skipped=0`). Reconcile time stays in the 5–10 ms range, which is
acceptable for 60 FPS.

So the bottleneck is **not** tree build, signal propagation, or persistence.

## Root cause (best current hypothesis)

`windows_reactor::RenderHost::request_render` coalesces render requests:

1. First request schedules `render_loop` with `DispatcherQueuePriority::Normal`.
2. While `render_loop` is running, new requests only set `RenderingDirty`.
3. After the render, a dirty loop re-schedules itself with
   `DispatcherQueuePriority::Low`.

The suspicion was that a `Low`-priority re-render starves behind a flood of
`Normal`-priority pointer-move messages. Patching `Low` → `Normal` did **not**
fix the symptom, so the starvation is probably not in our render queue but in
how WinUI dispatches pointer events while the left button is captured.

WinUI appears to deliver `PointerMoved` synchronously and keeps the UI thread
inside the pointer-capture message loop. Our `DispatcherQueue` callbacks (both
`Normal` and `Low`) are not pumped until the pointer stream pauses. Releasing
the button ends the capture and immediately lets the queued render run.

## What did not work

- Commit-only resize (updating only on release) — removes the livelock but
  makes the resize feel dead.
- Re-render priority `Low` → `Normal` — no visible change.
- Removing per-cell `on_tapped` and using `ListView` selection — reduces
  `diffed` from 784 to 47, but the update still waits for the mouse to stop.

## Current state

We do not have a fix. The resize handle still calls `set` on every
`PointerMoved`, and `RenderHost` still coalesces renders. The UI updates as
soon as WinUI releases the pointer stream.

Possible directions for a real fix:

1. **Bypass the dispatcher for resize.** Apply the width change directly to
   the realized `FrameworkElement`s without going through `request_render`.
   This requires `windows_reactor` to expose a way to update an element prop
   outside the reconciler.
2. **Use a XAML `Thumb`/`GridSplitter` instead of a custom pointer handler.**
   The platform controls resize synchronously and do not depend on our
   render queue.
3. **Force a synchronous render.** Make `request_render` detect a resize
   gesture and run `render_once` inline instead of enqueueing. Risky: deep
   recursion if a render triggers another resize.
4. **Throttle resize events to the display refresh.** Coalesce pointer moves
   at ~60 Hz so the render queue is not flooded. Does not address the
   dispatcher starvation, but may make it less visible.

## Related changes kept in the codebase

- `ColumnSpec::min_width` is respected by the resize handle.
- Header cells can be arbitrary `Element`s (`ColumnSpec::new_with_header`).
- CPU/Memory headers show live machine summary (percent + progress bar).
- Row selection uses `ListView` selection instead of per-cell `on_tapped`.
- `ColumnLayoutEntry` no longer writes back to the same signal it syncs from
  (that was causing a stack overflow on resize).
