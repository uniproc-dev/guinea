# TODO

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
