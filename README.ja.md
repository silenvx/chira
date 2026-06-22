# chira

チラシの裏のように、使い捨てで何でも書き散らせる作業ディレクトリを素早く管理する TUI ツール (Rust + ratatui)。

> 🌐 こちらが原本です。英語版 [README.md](README.md) は機械翻訳です。

メモ用のファイルだけでなく、「お試しで何かを走らせる作業ディレクトリ」も同じ場所で一覧・作成・削除でき、選んだディレクトリでそのままシェルを開いて実験・agent 実行ができる。ファイルの編集は `$EDITOR` に任せる（内蔵エディタは持たない）。

名前は「散らす」→「チラシ（の裏）」=どうでもいいことを書き散らす場所、から。

## インストール

```sh
cargo install --locked chira
```

`--locked` を付けると公開時の `Cargo.lock` をそのまま使うため、テスト済みと同じ依存バージョンで入る。

### シェル連携（推奨）

`chira` の中でディレクトリを移動して `q` で終了したとき、**起動元シェルの作業ディレクトリをそのディレクトリに変更**できる（lf / nnn と同じ方式）。子プロセスは親シェルの cwd を直接変えられないため、`--cd-file` で最終ディレクトリを書き出し、シェル関数側で `cd` する。このラッパーを入れないと、素の `chira` バイナリではシェルのディレクトリは変わらない。

シェルの起動ファイルに追加（zsh は `~/.zshrc`、bash は `~/.bashrc`）:

```sh
chira() {
  local tmp; tmp="$(mktemp)" || return
  command chira --cd-file "$tmp" "$@"
  local dir; dir="$(cat "$tmp")"
  rm -f "$tmp"
  [ -n "$dir" ] && [ -d "$dir" ] && [ "$dir" != "$PWD" ] && cd "$dir"
}
```

`~/.config/fish/functions/chira.fish`（fish）:

```fish
function chira
    set -l tmp (mktemp); or return
    command chira --cd-file $tmp $argv
    set -l dir (cat $tmp)
    rm -f $tmp
    test -n "$dir"; and test -d "$dir"; and test "$dir" != "$PWD"; and cd "$dir"
end
```

これで `chira` → ディレクトリへ降りる → `q` 終了、でシェルがそのディレクトリに移動する。直後にシェル標準の `cd -` で元いた場所へ戻れる（`cd` が `OLDPWD` を設定するため）。

## 保存場所

`$CHIRA_DIR` → `$XDG_DATA_HOME/chira` → `~/.local/share/chira` の順で決まる（macOS でも XDG 流。ターミナルから扱いやすいよう Apple の Application Support は使わない）。中身は素のファイル/ディレクトリなので、外部エディタや `grep`・dotfiles 同期とそのまま併用できる。

```sh
CHIRA_DIR=~/scratch chira   # 場所を変える
```

## 設定ファイル

シェルの起動ファイルを汚さずに永続的な設定を持てるよう、chira は TOML の設定ファイルを読む。パスは `$CHIRA_CONFIG`（直接指定）→ `$XDG_CONFIG_HOME/chira/config.toml` → `~/.config/chira/config.toml` の順で解決する。ファイル不在・空は「未設定」扱い（warning なし）、壊れたファイルは stderr に warning を出してデフォルト設定で起動する。

```toml
# ~/.config/chira/config.toml
dir = "~/scratch"      # 保存場所（先頭の ~ は $HOME に展開される）
editor = "nvim"        # 外部エディタ（引数も可。例: "code --wait"）
shell = "/bin/zsh"     # `s` で開くシェル（引数も可。例: "zsh -l"）
```

各項目は省略すると個別にフォールバックする。優先順位（高 → 低）は **環境変数 → 設定ファイル → ハードコードのデフォルト** で、既存の環境変数ベースの使い方はそのまま動き続ける:

- `dir`: `$CHIRA_DIR` → `dir` → `$XDG_DATA_HOME/chira` → `~/.local/share/chira`
- `editor`: `$EDITOR` → `editor` → `vi`
- `shell`: `$SHELL` → `shell` → `/bin/sh`

```sh
CHIRA_DIR=/tmp/other chira   # 環境変数が config の dir より優先される
```

## 表示言語

UI 文字列（ヘルプ・ステータス・確認ダイアログ）は以下の順序で決まる:

1. `CHIRA_LANG` — 明示的な override（大文字小文字を無視）。受理する値: `ja` / `ja_jp` / `japanese` → 日本語、`en` / `en_us` / `english` → 英語。それ以外の値は無視して locale 判定へフォールバック
2. POSIX locale: `LC_ALL` → `LC_MESSAGES` → `LANG`（`ja` で始まる値は日本語、それ以外（`C` / `POSIX` 含む）は英語）
3. デフォルト: 英語

```sh
CHIRA_LANG=ja chira   # 日本語 UI を強制
CHIRA_LANG=en chira   # 英語 UI を強制
```

## キー操作

| キー | 動作 |
|---|---|
| `j`/`↓`, `k`/`↑` | カーソル移動 |
| `g`/`G` | 先頭 / 末尾 |
| `Enter` / `l` / `→` | 開く（ファイル→`$EDITOR` / ディレクトリ→中へ降りる） |
| `h` / `←` / `Backspace` | 親ディレクトリへ戻る |
| `e` | 選択ファイルを `$EDITOR` で開く（ファイルのみ） |
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

## 開発

```sh
cargo run               # ソースから実行
cargo build --release   # 単一バイナリをビルド
```

## License

MIT
