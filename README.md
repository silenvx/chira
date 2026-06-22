# chira

A TUI tool for managing your *chirashi no ura* — disposable directories where you scatter and scribble throwaway stuff (Rust + ratatui).

> 🌐 This English README is machine-translated. The authoritative version is [日本語版 / README.ja.md](README.ja.md).

It handles not only note files but also "directories where you try running something," letting you list, create, and delete them in one place — and open a shell right inside a selected directory to experiment or run agents. Editing is delegated to `$EDITOR` (no built-in editor).

*chirashi no ura* (チラシの裏) literally means "the back of a flyer" — in Japan, the throwaway surface you scribble worthless little things on. The name *chira* comes from 散らす (*chirasu*, "to scatter") → チラシ (*chirashi*, a flyer).

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
  local dir; dir="$(cat "$tmp")"
  rm -f "$tmp"
  [ -n "$dir" ] && [ -d "$dir" ] && [ "$dir" != "$PWD" ] && cd "$dir"
}
```

Or `~/.config/fish/functions/chira.fish` (fish):

```fish
function chira
    set -l tmp (mktemp); or return
    command chira --cd-file $tmp $argv
    set -l dir (cat $tmp)
    rm -f $tmp
    test -n "$dir"; and test -d "$dir"; and test "$dir" != "$PWD"; and cd "$dir"
end
```

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
| `chira new <name>` | `n` | Create a file and open `$EDITOR`; `--no-edit` skips the editor |
| `chira mkdir <name>` | `N` | Create a directory |
| `chira edit <name>` | `e` | Open `<name>` in `$EDITOR` |
| `chira shell [<dir>]` | `s` | Open `$SHELL` in `<dir>` (or in `CHIRA_DIR` if omitted) |
| `chira rm <name>` | `d` | Delete; `-r` is required for directories, `-f` skips confirmation |
| `chira mv <old> <new>` | `r` | Rename |
| `chira path [<name>]` | — | Print the full path of an entry (or `CHIRA_DIR` if omitted) |
| `chira find <query> [<path>]` | `/` | List entries whose name matches the substring (`ls`-style output) |

Output is biased toward machine-readability: `ls` / `find` print one name per line, color and the trailing `/` for directories appear only when stdout is a TTY. Errors go to stderr; missing entries exit `1`, argument errors exit `2`. Destructive operations (`rm` / `mv`) verify that the target is under `CHIRA_DIR` (canonicalized; `..` and symlink escapes are rejected). On a symlink, `rm` removes the symlink itself (unix `rm` semantics), not the target. `rm` / `mv` refuse to operate on the scratch root itself (`.` / empty name) to prevent accidentally destroying the whole `CHIRA_DIR`.

`new` and `mkdir` accept a single basename and always create at `CHIRA_DIR` root (slashes in the name are rejected). The other commands (`ls`, `tree`, `edit`, `shell`, `rm`, `mv`, `path`, `find`) accept root-relative paths and can target nested entries (e.g. `chira edit ws/note.md`). To create inside a subdirectory, open a shell in it first (`chira shell ws`).

`chira path` enables shell-side `cd` without launching the TUI:

```sh
cd "$(chira path)"               # cd into CHIRA_DIR
cd "$(chira path my-experiment)" # cd into a specific entry
```

## Development

```sh
cargo run               # run from source
cargo build --release   # build the single binary
```

## License

MIT
