# chira

A TUI tool for managing your *chirashi no ura* — disposable directories where you scatter and scribble throwaway stuff (Rust + ratatui).

> 🌐 This English README is machine-translated. The authoritative version is [日本語版 / README.ja.md](README.ja.md).

It handles not only note files but also "directories where you try running something," letting you list, create, and delete them in one place — and open a shell right inside a selected directory to experiment or run agents. Editing is delegated to `$EDITOR` (no built-in editor).

*chirashi no ura* (チラシの裏) literally means "the back of a flyer" — in Japan, the throwaway surface you scribble worthless little things on. The name *chira* comes from 散らす (*chirasu*, "to scatter") → チラシ (*chirashi*, a flyer).

## Screen

The Browse screen right after launch looks like this (list on the left, preview or directory contents of the selection on the right, key hints on the bottom line). The middle header segment (`chira`) is the path relative to CHIRA_DIR root and expands like `chira/foo` once you descend into a subdirectory. The right-pane tree starts at the **children** of the selected directory (per `src/scratch.rs::tree`):

```
 chira  chira  4 items
┌─ List ──────────────────────────┐┌─ Directory contents ──────────────────┐
│› 06/22 00:59  try-agent/        ││ ├── README.md                         │
│  06/21 22:10  sandbox/          ││ ├── run.sh                            │
│  06/21 18:42  memo.md           ││ └── notes/                            │
│  06/20 14:10  TODO.md           ││     ├── 2026-06-21.md                 │
│                                 ││     └── 2026-06-22.md                 │
│                                 ││                                       │
│                                 ││                                       │
└─────────────────────────────────┘└───────────────────────────────────────┘
 j/k:move  l:open  h:parent  s:shell  n:new  /:filter  ?:help  q:quit
```

## Install

```sh
cargo install --locked chira
```

`--locked` makes the build use the published `Cargo.lock`, so you get the exact dependency versions that were tested.

### Shell integration (recommended)

`chira` can move your **calling shell's working directory** to the directory you ended up in when you quit (the lf / nnn approach). A child process cannot change its parent shell's cwd directly, so `chira` writes the final directory via `--cd-file` and a shell function does the `cd`. Without this wrapper, running the bare `chira` binary cannot change your shell's directory.

Add to your shell startup file (`~/.zshrc` for zsh, `~/.bashrc` for bash):

```sh
chira() {
  local tmp; tmp="$(mktemp)" || return
  command chira --cd-file "$tmp" "$@"
  # zsh では $status が read-only な特殊変数 ($? の別名) のため別名を使う
  local rc=$?
  local dir; dir="$(cat "$tmp")"
  rm -f "$tmp"
  [ -n "$dir" ] && [ -d "$dir" ] && [ "$dir" != "$PWD" ] && cd "$dir"
  return $rc
}
```

Or `~/.config/fish/functions/chira.fish` (fish):

```fish
function chira
    set -l tmp (mktemp); or return
    command chira --cd-file $tmp $argv
    set -l cmd_status $status
    set -l dir (cat $tmp)
    rm -f $tmp
    test -n "$dir"; and test -d "$dir"; and test "$dir" != "$PWD"; and cd "$dir"
    return $cmd_status
end
```

`return $rc` を最後に置くことで、`chira gc` 等の CLI サブコマンドの exit code (gc は errors > 0 で 1、引数誤りで 2) がそのまま wrapper の戻り値になる。TUI 経由で `cd` した場合も `cd` の成否ではなく chira 本体の exit code を返す。

Now: launch `chira` → descend into a directory → quit with `q`, and your shell moves there. Right after, the shell's standard `cd -` takes you back to where you were (because `cd` sets `OLDPWD`).

## Storage

The location is resolved in this order: `$CHIRA_DIR` → `$XDG_DATA_HOME/chira` → `~/.local/share/chira` (XDG-style even on macOS; Apple's Application Support is not used, so it's easy to handle from the terminal). The contents are plain files and directories, so they work directly with external editors, `grep`, and dotfiles sync.

```sh
CHIRA_DIR=~/scratch chira   # use a different location
```

## Configuration

For persistent settings without touching your shell startup files, chira reads a TOML config file. The path is resolved in this order: `$CHIRA_CONFIG` (a direct path) → `$XDG_CONFIG_HOME/chira/config.toml` → `~/.config/chira/config.toml`. A missing or empty file is treated as "unset" (no warning); a broken file prints a warning to stderr and starts with defaults.

```toml
# ~/.config/chira/config.toml
dir = "~/scratch"      # storage location (leading ~ is expanded to $HOME)
editor = "nvim"        # external editor (arguments allowed, e.g. "code --wait")
shell = "/bin/zsh"     # shell opened with `s` (arguments allowed, e.g. "zsh -l")
```

Each value falls back independently when omitted. Resolution priority (high → low) is **environment variable → config file → built-in default**, so existing env-based usage keeps working:

- `dir`: `$CHIRA_DIR` → `dir` → `$XDG_DATA_HOME/chira` → `~/.local/share/chira`
- `editor`: `$EDITOR` → `editor` → `vi`
- `shell`: `$SHELL` → `shell` → `/bin/sh`

```sh
CHIRA_DIR=/tmp/other chira   # env wins over config's dir
```

### Auto-name templates for `new` / `mkdir`

`chira new` / `chira mkdir` without an argument auto-generate a name from a `chrono` `strftime` template. The defaults are `scratch-%Y%m%d-%H%M%S.md` (file) and `scratch-%Y%m%d-%H%M%S` (directory). To customize:

```toml
[new]
name_template = "memo-%Y-%m-%d_%H%M%S.md"   # used when `chira new` is called without a name
dir_template  = "memo-%Y-%m-%d_%H%M%S"      # used when `chira mkdir` is called without a name
```

- Any `chrono` format specifier ( https://docs.rs/chrono/latest/chrono/format/strftime/ ) is accepted; the result is the entry name on disk.
- Leading/trailing whitespace is trimmed before use; a whitespace-only value is treated as unset.
- Unset or empty values fall back to the built-in defaults above.
- The TUI placeholders for `n` (new file) and `N` (new directory) use the same templates, so CLI and TUI stay in sync.
- If a template renders to a name that fails `chira`'s safety check (empty, contains `/`, or starts with `.`), or contains a `chrono` specifier that cannot be parsed (e.g. `%Q`), a warning is printed at startup and the default is used instead.

### Editing from the TUI

Inside the TUI press `,` to open the configuration screen. It lists every option with its current value and source — `(env: CHIRA_DIR)` / `(config)` / `(default)` — plus the resolution order and the absolute path the changes will be saved to. `Enter` edits the highlighted entry (`Space` toggles booleans), `s` writes the changes back to the same `config.toml` while preserving formatting and comments, and `Esc` returns to the file list. Items currently overridden by an env var are marked `⚠ env override` — the file is still updated, but the env var continues to take precedence on the next launch. Values written by the TUI take effect on the next start (the current session keeps the snapshot loaded at boot).

## Actions (run a command in a freshly created directory)

To bootstrap a new directory with a command of your choice (rsync a skeleton, `git clone`, scaffold via `cookiecutter`, anything), define `[actions.<name>]` entries in `config.toml`. Pressing `t` opens a picker, asks for a directory name, shows the resolved command in a confirm screen (trust gate — config-derived shell runs as you), then on `y` creates the directory and runs the command via `sh -c` inside it.

```toml
# Bootstrap an isolated dev shell
[actions.nix-sandbox]
description = "Nix flake + direnv sandbox"
run = "rsync -a ~/.config/chira/skel/nix-sandbox/ ./ && direnv allow"

# Clone a sandbox repo
[actions.clone-sandbox]
description = "Clone my sandbox repo"
run = "git clone --depth 1 git@github.com:me/sandbox.git ."

# Delegate interpolation to a dedicated generator
[actions.from-copier]
run = "copier copy --trust ~/.config/chira/templates/app \"$CHIRA_TARGET\""
```

Optional: make `N` (plain new directory) auto-run a specific action. Default-off; with this set, `N` becomes the same flow as `t` → `<name>`. **Place `default_action` before any `[actions.*]` table** so TOML parses it as a root-level key (otherwise it becomes a key inside the preceding `[actions.<name>]` table and is silently ignored):

```toml
default_action = "nix-sandbox"

[actions.nix-sandbox]
# ...
```

- `description` is shown in the picker (optional). `run` is required; entries with missing/empty `run` are silently skipped.
- The `run` command is executed via `/bin/sh -c` so pipelines (`&&`, `|`), `~` expansion, and `$VAR` work as usual.
- chira itself does **not** interpolate file contents or filenames. If you need templating, call a dedicated tool from `run` (`cookiecutter --no-input`, `copier copy --trust`, `envsubst`, `sed`, etc.).
- The new directory is the cwd. `run` also receives env vars:
  - `CHIRA_TARGET` — absolute path of the new directory
  - `CHIRA_TARGET_NAME` — directory name
  - `CHIRA_ROOT` — scratch root (`$CHIRA_DIR`)
- On a non-zero exit, the directory is **kept** (no auto-rollback — diagnostic state may be useful) and `.chira/bootstrap-failed` is written inside it. The list shows `[!]` in front of those directories so you can spot the half-provisioned ones. To retry, delete the directory with `d` and re-run the action (chira always creates a fresh directory and refuses an existing name); alternatively, remove `.chira/bootstrap-failed` manually to clear the marker without re-running.
- `default_action = "<name>"` makes the plain `N` key go through the same picker-less confirm + run flow. With it unset (default), `N` keeps creating an empty directory. An unknown name silently falls back to the plain-`N` behavior.

## Language

UI strings (help overlay, status messages, prompts) follow this resolution order:

1. `CHIRA_LANG` — explicit override (case-insensitive). Accepted values: `ja` / `ja_jp` / `japanese` → Japanese, `en` / `en_us` / `english` → English. Any other value falls through to the locale check
2. POSIX locale: `LC_ALL` → `LC_MESSAGES` → `LANG` (values starting with `ja` select Japanese; everything else, including `C`/`POSIX`, selects English)
3. Default: English

```sh
CHIRA_LANG=ja chira   # force Japanese UI
CHIRA_LANG=en chira   # force English UI
```

## Keybindings

| Key | Action |
|---|---|
| `j`/`↓`, `k`/`↑` | move cursor |
| `g`/`G` | top / bottom |
| `Enter` / `l` / `→` | open (file → `$EDITOR` / directory → descend into it) |
| `h` / `←` / `Backspace` | go to parent directory |
| `e` | open the selected file in `$EDITOR` (files only) |
| `s` | open `$SHELL` in the selected directory (or current if none) — for experiments / running agents |
| `n` | new file (enter a name → open in `$EDITOR`) |
| `N` | new directory (with `default_action`: same as `t` + that action) |
| `t` | new directory from an action — pick → name → confirm command → run (`[actions.*]`) |
| `r` | rename |
| `d` | delete (with confirmation; directories are removed recursively) |
| `/` | filter by name |
| `,` | open the configuration screen (edit `config.toml` in place) |
| `?` | show help on screen (any key closes it) |
| `q` | quit |

Like vim, `h`/`j`/`k`/`l` navigate (`h` = parent, `l` = open), and arrow keys work too. Help is `?` (same as ranger / nnn). Selecting a directory shows its contents in the right pane as a `tree`-style view (depth 4, up to 100 lines).

Quitting `$EDITOR` (default `vi`) or the shell opened with `s` (default `/bin/sh`) returns you to the TUI, and any files created in the meantime show up in the list automatically. Create / rename / delete are reflected on disk immediately.

## CLI subcommands

Running `chira` with no arguments launches the TUI. Pass a subcommand to run a one-shot CLI operation instead — useful for piping (`chira ls | fzf`), scripts, or quick `cd` integration without spinning up the TUI.

| Subcommand | TUI equivalent | Notes |
|---|---|---|
| `chira ls [<path>]` | (list view) | One name per line; `-l` prints `<mtime>\t<size>\t<name>` |
| `chira tree [<path>]` | (right pane) | Tree view (depth 4, up to 100 lines) |
| `chira new [<name>]` | `n` | Create a file and open `$EDITOR`; `--no-edit` skips the editor. When `<name>` is omitted, the name is generated from `[new] name_template` (default `scratch-%Y%m%d-%H%M%S.md`) |
| `chira mkdir [<name>]` | `N` | Create a directory. When `<name>` is omitted, the name is generated from `[new] dir_template` (default `scratch-%Y%m%d-%H%M%S`) |
| `chira edit <name>` | `e` | Open `<name>` in `$EDITOR` |
| `chira shell [<dir>]` | `s` | Open `$SHELL` in `<dir>` (or in `CHIRA_DIR` if omitted) |
| `chira rm <name>` | `d` | Delete; `-r` is required for directories, `-f` skips confirmation |
| `chira mv <old> <new>` | `r` | Rename |
| `chira path [<name>]` | — | Print the full path of an entry (or `CHIRA_DIR` if omitted) |
| `chira find <query> [<path>]` | `/` | List entries whose name matches the substring (`ls`-style output) |
| `chira gc [--ttl <dur>] [--archive-dir <path>] [--dry-run]` | — | Move entries whose `mtime` exceeds TTL to the archive dir (see below) |

Output is biased toward machine-readability: `ls` / `find` print one name per line, color and the trailing `/` for directories appear only when stdout is a TTY. Errors go to stderr; missing entries exit `1`, argument errors exit `2`. Destructive operations (`rm` / `mv`) verify that the target is under `CHIRA_DIR` (canonicalized; `..` and symlink escapes are rejected). On a symlink, `rm` removes the symlink itself (unix `rm` semantics), not the target. In non-interactive (non-TTY stdin) contexts, `rm` requires `-f`; without it the prompt auto-cancels with exit `1`. `rm` / `mv` refuse to operate on the scratch root itself (`.` / empty name) to prevent accidentally destroying the whole `CHIRA_DIR`.

`new` and `mkdir` accept a single basename and always create at `CHIRA_DIR` root (slashes in the name are rejected). The other commands (`ls`, `tree`, `edit`, `shell`, `rm`, `mv`, `path`, `find`) accept root-relative paths and can target nested entries (e.g. `chira edit ws/note.md`). To create inside a subdirectory, open a shell in it first (`chira shell ws`).

`chira path` enables shell-side `cd` without launching the TUI:

```sh
cd "$(chira path)"               # cd into CHIRA_DIR
cd "$(chira path my-experiment)" # cd into a specific entry
```

## Archive (chira gc)

chira treats entries as throwaways, but stale ones pile up unless you delete them by hand. `chira gc` sweeps entries whose `mtime` is older than a TTL and moves them under `<CHIRA_DIR>/.archive/` (a hidden directory, so it disappears from the main listing). The archived files stay as plain files/directories, so `find` / `grep` still work on them.

```sh
chira gc --ttl 30d              # archive entries older than 30 days
chira gc --ttl 12h --dry-run    # preview without moving
chira gc --archive-dir ~/old    # move to a custom location
```

Time units: `s` / `m` / `h` / `d` / `w` (no unit defaults to seconds). The TTL is required — `chira gc` exits with an error if neither `--ttl` nor `[archive] ttl_days` is set, so an unconfigured invocation never erases anything by surprise.

The mtime is read from the entry itself (`symlink_metadata`, so intact symlinks are aged by the link itself, not the target). atime is not used because relatime / noatime mounts do not update it. Dangling symlinks (target missing) are treated as "mtime cannot be read" — they are tracked under `errors`, not aged by the link.

### Exclusions

Even past the TTL, the following are kept (counted as `kept` in the summary):

- The `.archive/` directory itself (so re-sweeps stay safe)
- Directories that contain a `.keep` marker file (lf / nnn convention)
- Names that match any pattern in `[archive] keep` (see below)

Entries whose mtime cannot be read (broken symlinks etc.) are tracked separately as **errors** — they are skipped with a warning on stderr and counted in `errors`, not `kept`. The summary line shows `archived / kept / errors` as independent categories, and a non-zero `errors` count exits with code 1 so cron can detect the condition.

### Config (`[archive]`)

```toml
[archive]
# TTL in days (0 / unset means archive is off; CLI --ttl can still drive a one-shot run)
ttl_days = 30

# Archive destination. ~ is expanded. Defaults to <CHIRA_DIR>/.archive
dir = "~/scratch-archive"

# Sweep on TUI startup. Default false (a surprise sweep on every run is too aggressive)
on_startup = false

# Names matching any of these globs are kept. A trailing `/` restricts the match to directories
keep = ["pinned-*", "longterm/"]
```

Globs support `*` (any run) and `?` (any single character). A trailing `/` makes the pattern match directories only.

Name collisions in the archive directory get a `.<unix_ts>` suffix appended (e.g. `old.md.1742278300`); a second collision at the same second appends `_1`, `_2`, etc. Cross-filesystem moves fail with the underlying `EXDEV` error reported to stderr — point `dir` at a location on the same filesystem as `CHIRA_DIR` if you need that case to succeed.

## Development

```sh
cargo run               # run from source
cargo build --release   # build the single binary
```

## License

MIT
