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

Add to `~/.zshrc` (zsh / bash):

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

## Keybindings

| Key | Action |
|---|---|
| `j`/`↓`, `k`/`↑` | move cursor |
| `g`/`G` | top / bottom |
| `Enter` / `l` / `→` / `e` | open (file → `$EDITOR` / directory → descend into it) |
| `h` / `←` / `Backspace` | go to parent directory |
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

## Development

```sh
cargo run               # run from source
cargo build --release   # build the single binary
```

## License

MIT
