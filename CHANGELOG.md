# Changelog

## 0.2.0 — unreleased

### Picker UX
- Fuzzy subsequence match (`Modal::fuzzy`, ranked best-first) alongside substring search
- `ModalOption::disabled`, `ModalOption::separator`, `ModalOption::shortcut`,
  `ModalOption::keywords`/`keyword` (matched in both search modes)
- `Modal::default_selected`, `Modal::allow_custom_input` (command-palette free text),
  `Modal::empty_message`, `Modal::footer_hint`, `Modal::max_results`
- Extended keys: `Home`/`End`, `PgUp`/`PgDn`, `Ctrl+N`/`Ctrl+P`, `Ctrl+U` clears query
- First-query-occurrence highlight (underline, no layout shift)

### New modal types
- `Confirm::new(...).message(...).run() -> bool`
- `Prompt::new(...).placeholder(...).password(...).run() -> Option<String>`
- `Modal::run_multi()` / `multi::run_multi` — Space-toggle multi-select

### CLI (`quick-modals --dmenu`)
- stdin-lines → picker → stdout, `--prompt/--layout/--theme/--multi/--fuzzy/`
  `--case-sensitive/--allow-custom/--history-file/--empty-message/`
  `--max-results/--print-id`; exit codes 0 pick · 1 cancel · 2 error

### API / robustness
- Typed `ModalError::{NoOptions, Css, Backend}` (`Display + Error`;
  `From<ModalError> for String` keeps legacy `?` working)
- `Modal::spawn(callback)` non-blocking variant
- `Modal::font(name, bytes)` custom fonts; `Modal::history_file(path)`
  most-used-first sorting with pick-count persistence; `Theme::auto()`
  light/dark sniffing
