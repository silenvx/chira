# chira

一時的な scratch ディレクトリを素早く管理する TUI ツール (Rust + ratatui)。

メモ用のファイルだけでなく、「お試しで何かを走らせる作業ディレクトリ」も同じ場所で一覧・作成・削除でき、選んだディレクトリでそのままシェルを開いて実験・agent 実行ができる。ファイルの編集は `$EDITOR` に任せる（内蔵エディタは持たない）。

名前は「散らす」→「チラシ（の裏）」=どうでもいいことを書き散らす場所、から。

## 保存場所

`$CHIRA_DIR` → `$XDG_DATA_HOME/chira` → `~/.local/share/chira` の順で決まる（macOS でも XDG 流。ターミナルから扱いやすいよう Apple の Application Support は使わない）。中身は素のファイル/ディレクトリなので、外部エディタや `grep`・dotfiles 同期とそのまま併用できる。

## 使い方

```sh
cargo run                                          # 開発実行
cargo build --release && cp target/release/chira ~/.local/bin/   # 単一バイナリを PATH へ
CHIRA_DIR=~/scratch chira                           # 場所を変える
```

## キー操作

| キー | 動作 |
|---|---|
| `j`/`↓`, `k`/`↑` | カーソル移動 |
| `g`/`G` | 先頭 / 末尾 |
| `Enter` / `l` / `→` / `e` | 開く（ファイル→`$EDITOR` / ディレクトリ→中へ降りる） |
| `h` / `←` / `Backspace` | 親ディレクトリへ戻る |
| `s` | 選択ディレクトリ（無ければ現在地）で `$SHELL` を開く（実験・agent 実行用） |
| `n` | 新規ファイル（名前入力 → `$EDITOR` で開く） |
| `N` | 新規ディレクトリ |
| `r` | 名前を変更 |
| `d` | 削除（確認あり。ディレクトリは中身ごと） |
| `/` | 名前で絞り込み検索 |
| `?` | ヘルプを画面に表示（何かキーで閉じる） |
| `q` | 終了 |

vim と同じく `h`/`j`/`k`/`l` で移動（`h`=親、`l`=開く）でき、方向キーでも操作可能。ヘルプは ranger/nnn と同じ `?`。ディレクトリを選ぶと右ペインに中身が `tree` 風（深さ 4・最大 100 行）で表示される。

`$EDITOR`（未設定なら `vi`）や `s` で開いたシェル（未設定なら `/bin/sh`）を終了すると TUI に復帰し、その間に作られたファイルも一覧へ自動反映される。作成・改名・削除はディスクへ即反映される。

## 終了したディレクトリへ cd する（シェル連携）

`chira` の中でディレクトリを移動して `q` で終了したとき、**起動元シェルの作業ディレクトリをそのディレクトリに変更**できる（lf / nnn と同じ方式）。子プロセスは親シェルの cwd を直接変えられないため、`--cd-file` で最終ディレクトリを書き出し、シェル関数側で `cd` する。

この連携を入れておくと、`chira` 終了後にシェル標準の **`cd -`** で元の場所へ戻れる（`cd` が `OLDPWD` を設定するため）。

### zsh / bash

`~/.zshrc` などに追加:

```sh
chira() {
  local tmp; tmp="$(mktemp)" || return
  command chira --cd-file "$tmp" "$@"
  local dir; dir="$(cat "$tmp")"
  rm -f "$tmp"
  [ -n "$dir" ] && [ -d "$dir" ] && [ "$dir" != "$PWD" ] && cd "$dir"
}
```

### fish

`~/.config/fish/functions/chira.fish` に:

```fish
function chira
    set -l tmp (mktemp); or return
    command chira --cd-file $tmp $argv
    set -l dir (cat $tmp)
    rm -f $tmp
    test -n "$dir"; and test -d "$dir"; and test "$dir" != "$PWD"; and cd "$dir"
end
```

これで `chira` → ディレクトリへ降りる → `q` 終了、でシェルがそのディレクトリに移動する。直後に `cd -` で元いた場所へ戻れる。

## License

MIT
