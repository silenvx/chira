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
  local status=$?
  local dir; dir="$(cat "$tmp")"
  rm -f "$tmp"
  [ -n "$dir" ] && [ -d "$dir" ] && [ "$dir" != "$PWD" ] && cd "$dir"
  return $status
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

`return $status` を最後に置くことで、`chira gc` 等の CLI サブコマンドの exit code (gc は errors > 0 で 1、引数誤りで 2) がそのまま wrapper の戻り値になる。TUI 経由で `cd` した場合も `cd` の成否ではなく chira 本体の exit code を返す。

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
| `N` | new directory |
| `r` | rename |
| `d` | delete (with confirmation; directories are removed recursively) |
| `/` | filter by name |
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
| `chira new [<name>]` | `n` | Create a file and open `$EDITOR`; `--no-edit` skips the editor. When `<name>` is omitted, defaults to `scratch-YYYYMMDD-HHMMSS.md` |
| `chira mkdir [<name>]` | `N` | Create a directory. When `<name>` is omitted, defaults to `scratch-YYYYMMDD-HHMMSS` |
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
