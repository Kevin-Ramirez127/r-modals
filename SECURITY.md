# Security

`r-modals` runs untrusted text through a picker and, for prefixes/shortcuts,
turns it into process launches. This page documents the boundaries and the
mitigations in place.

Report vulnerabilities privately via the repository's security advisories (or
email the maintainer listed in `Cargo.toml`) rather than a public issue.

## Trust model

| Input | Trust |
| --- | --- |
| Options streamed on stdin (`--menu`, `Dialog::menu()`) | **Untrusted** |
| Query text typed into the picker | **Untrusted** |
| Prefix/shortcut/config files you point at explicitly | Trusted (your choice) |
| Auto-loaded XDG config files | Trusted only if not writable by other users |
| `IconSource::File`/`Bytes` from your program | Trusted, but still bounded |
| Child process arguments produced by this crate | Untrusted where they echo the query |

## Mitigations

### Shell interpolation

- On Unix, `{query}` and the appended remainder are POSIX-quoted as one word
  before being inserted into a `shell` action. Shell actions run through
  `/bin/sh` by absolute path.
- On **Windows**, dynamic `shell` interpolation is refused with an error.
  `cmd.exe` expands `%VAR%` before parsing, so even a quoted value can inject
  `&`, `|`, `^`, or `"`. Use `command` + `args = ["{query}"]` instead; argv is
  never passed through a shell and behaves identically on every platform.
  Static `shell` strings (no `{query}`, empty remainder) keep working.
- Prefer `command` + `args` for anything portable.

### Dialog argument injection

`Dialog::ask/confirm/error` pass their label/title/message after a `--`
separator, so a title beginning with `-` can never be interpreted as a child
flag. The child parser stops option parsing at `--` as well.

### Option stream DoS and terminal escapes

- Option lines are capped at 64 KiB, the option count at 100 000, and the whole
  stdin stream at 64 MiB. Overlong input fails the dialog instead of
  allocating/hanging without bound.
- C0 control characters (other than tabs inside labels) are replaced with
  spaces as options are read and again when a selection is printed, so a
  hostile label cannot emit ANSI/OSC terminal sequences or forge output lines.
  Use `--format json` when labels must round-trip byte-for-byte.

### State files (config, pid, history)

All state writes go through `fsutil::write_atomic`:

- the destination must be absent or a **regular file**; symlinks, FIFOs and
  directories are refused;
- the temporary sibling is created with `create_new` (`O_EXCL`) under a unique
  name and mode `0600`, so a pre-planted symlink cannot redirect the write;
- data is `fsync`ed and `rename`d into place.

History reads accept only regular files up to 1 MiB / 4096 entries. Newlines
and control characters in ids are neutralised, and ids are capped, so a pick
cannot forge extra history rows.

### Config variables

`ConfigFile::var(name, value)` escapes `value` for a TOML basic string and
substitutes in a single pass. An executable path containing `"`, `\`, newlines,
or `{other}` tokens cannot terminate the string, inject new `[[prefix]]`
entries, or be expanded again.

### Icons

- Encoded images are decoded through `image::ImageReader` with a 4096×4096
  edge limit and a 64 MiB allocation cap, before the decoder can allocate
  unbounded memory.
- `IconSource::Pixels` validates dimensions with checked multiplication, rejects
  zero/oversized dimensions, and requires an exact RGBA length.

### Auto-loaded configuration

The XDG default `shortcuts.toml`/`prefixes.toml` are ignored (with a warning on
stderr) when they, or their parent directory, are group/world-writable.
Explicitly configured paths are never filtered — they are the caller's own
choice.

## Not covered

- The picker renders text in a native window; it does not sanitize Unicode
  bidi/zero-width characters (they can reorder display but cannot execute
  commands).
- Process actions are spawned detached and are not sandboxed. A config file is
  code execution by design; keep it owner-writable.
- On Windows, ACLs are not inspected when auto-loading config files.
