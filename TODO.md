# TODO

## Devtools (guinea-plugins)

### The MCP server answers in JSON meant for a window

`tools/devtools/mcp` offers every route of the API as a tool, generated from
the OpenAPI document, and hands back the response body as it came. That body
is shaped for the window: one trace record is an array of words with tones and
links, ten lines of JSON for what reads as
`14:47  push into Metrics  <- MetricsActor handles Tick`. The only consumers
are agents, and they pay for every line of it.

Left:

- A text rendering per operation, chosen by the operation's id, with the raw
  body behind a `json: true` argument for the routes where structure is the
  point (`get_element`, `get_native_tree`, `get_graph`). `words::text` in the
  model is the primitive; the printers the deleted CLI had are worth lifting
  rather than rewriting - `git show HEAD:tools/devtools/cli/src/main.rs`.
- Tool descriptions written for an agent, saying what the answer is good for,
  instead of the `summary` that `--help` prints at a person. Which means the
  description stops being derived from the summary.
- `.mcp.json` in the root of `guinea-plugins`, so a session started there sees
  the tools. A session picks its tools up at startup, so registering one only
  reaches the next session, not the one that registered it.

### The puffin profiler, half done

Done: zones around every page's and layout's `render` (`devtools::Rendering`),
a frame boundary per backend, `Capability::Profiler`, `Command::Profiler`,
`Report::Profiler`, and a `puffin_http` server the plugin starts on request.

Left: devtools' own half - keep the address the application reports, a command
route on the HTTP API, and a page that connects with `puffin_http::Client` and
draws the frames. `puffin_egui` is no use: it is on egui 0.33 and devtools are
on 0.36, so the flame graph is ours to draw.

Also worth doing: let devtools profile themselves, which is the same page
reading the in-process `GlobalProfiler` instead of a socket.

## Deep links

Found by running the generated `link()` / `parse()` over edge cases.

### Bug: an empty `String` capture does not round-trip

`Route::Host { host: "" }.link()` is `"//processes"`, and `parse` answers `None`
for it: the generated walk splits on `/` and drops empty segments, so the
capture disappears. The application hands out an address it does not answer.

Fix: keep empty segments, strip only one leading and one trailing `/`. This
stops `//ubuntu//processes` from matching, which it does today.

### Gap: nothing opens a link

- `parse` takes a path only; `guinea-processes://ubuntu/processes` and
  `guinea-processes:///ubuntu/processes` both give `None`.
- How scheme and path compose is not defined. In `scheme://ubuntu/processes`
  `ubuntu` is the authority, not a path segment, so the form has to be
  `scheme:///…` or `scheme:/…`.
- No adapter or example reads the activation address (argv, Windows protocol
  activation). There is no way from a URL to a screen.

### Smaller

- Numbers and `bool` do not unescape: `/job/%34%32` is `None`, while strings
  are unescaped. Unescape first, then parse.
- Non-canonical numbers are accepted: `/job/+42`, `/job/042` and `/job/42` are
  one route, `/offset/-0` too. Require the segment to equal
  `value.to_string()`.
- Literals are not unescaped: `/ubuntu/%70rocesses` is `None`, though RFC 3986
  makes `%70` and `p` equivalent.
- `.` and `..` captures round-trip here, but any URL normaliser removes them
  first, and WHATWG treats `%2E%2E` as `..` as well. Probably refuse in
  `link()`.
- A path with `?query` or `#fragment` is `None`. Fine if intended; some shells
  append one.

## Table (guinea-widgets, guinea-plugins)

Found after the table moved from `ListView` to `ItemsRepeater` +
`VirtualSource`, measured on uniproc.

### Bug: a click handled inside a cell also selects the row

The row selects itself from `Border::on_pointer_released`. A cell that handles
its own click - uniproc's chevron that folds a group, a `Border` with
`on_pointer_released` - fires, and then the row's handler fires too, selecting
the row. `ListView` did not do this.

Nothing in the table can stop it: reactor pointer callbacks are not routed, so
a handler cannot mark the event `Handled`. Keys can (`RoutedCallback`),
pointers cannot.

Options:

- Ask reactor for routed pointer callbacks, as keys have. The real fix.
- A column that does not select (`ColumnSpec::inert()` or the like). Coarse:
  the chevron shares its column with the name, which should still select.
- A "handled" flag the cell sets and the row checks. Works only if reactor
  runs the inner element's handler before the outer one; not verified.

### Regression: rows are not list items to UI Automation

With `ListView` the body was list / list item with a selection pattern. Now it
is one group with a flat run of text and images: no rows, no selection, for
screen readers and automated tests alike.

Reactor offers `automation_name`, `automation_id` and `automation_heading_level`
only - no control type, no selection pattern. Nothing to fix on the table's
side until reactor has them.

### Smaller

- The selection highlight ends at the last column, not the window's edge.
  Each realized row sits in a pooled `ContentControl` whose
  `HorizontalContentAlignment` stays `Left` from the built-in style. Setting it
  to `Stretch` live through XAML diagnostics made the highlight full width, so
  the fix is in reactor's shell.
- Column resize was not checked after the rewrite; synthetic input did not
  move the handle. Check by hand.

