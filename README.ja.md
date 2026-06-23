# chira

チラシの裏のように、使い捨てで何でも書き散らせる作業ディレクトリを素早く管理する TUI ツール (Rust + ratatui)。

> 🌐 こちらが原本です。英語版 [README.md](README.md) は機械翻訳です。

メモ用のファイルだけでなく、「お試しで何かを走らせる作業ディレクトリ」も同じ場所で一覧・作成・削除でき、選んだディレクトリでそのままシェルを開いて実験・agent 実行ができる。ファイルの編集は `$EDITOR` に任せる（内蔵エディタは持たない）。

名前は「散らす」→「チラシ（の裏）」=どうでもいいことを書き散らす場所、から。

## 画面イメージ

起動直後の Browse 画面はこんな感じ（左ペインに一覧、右ペインに選択中のファイルプレビューまたはディレクトリ内容、最下行にキーヒント）。ヘッダの中央セグメント (`chira`) は CHIRA_DIR ルートからの相対パスで、サブディレクトリへ降りると `chira/foo` のように展開される。右ペインのツリーは選択ディレクトリの**中身**から始まる（実装の `src/scratch.rs::tree` に準拠）:

```
 chira  chira  4 件
┌─ 一覧 ──────────────────────────┐┌─ ディレクトリ内容 ────────────────────┐
│› 06/22 00:59  try-agent/        ││ ├── README.md                         │
│  06/21 22:10  sandbox/          ││ ├── run.sh                            │
│  06/21 18:42  memo.md           ││ └── notes/                            │
│  06/20 14:10  TODO.md           ││     ├── 2026-06-21.md                 │
│                                 ││     └── 2026-06-22.md                 │
│                                 ││                                       │
│                                 ││                                       │
└─────────────────────────────────┘└───────────────────────────────────────┘
 j/k:移動  l:開く  h:親  s:シェル  n:新規  /:検索  ?:ヘルプ  q:終了
```

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
  local status=$?
  local dir; dir="$(cat "$tmp")"
  rm -f "$tmp"
  [ -n "$dir" ] && [ -d "$dir" ] && [ "$dir" != "$PWD" ] && cd "$dir"
  return $status
}
```

`~/.config/fish/functions/chira.fish`（fish）:

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

最後の `return $status` で `chira gc` 等の CLI サブコマンド exit code (errors > 0 で 1、引数誤りで 2) が wrapper の戻り値にそのまま伝播する。TUI 経由で `cd` した場合も、`cd` の成否ではなく chira 本体の exit code が返る。

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

### `new` / `mkdir` 引数省略時の自動命名テンプレ

`chira new` / `chira mkdir` を引数なしで実行すると `chrono` の `strftime` テンプレに従って自動採番する。デフォルトは `scratch-%Y%m%d-%H%M%S.md` (ファイル) / `scratch-%Y%m%d-%H%M%S` (ディレクトリ)。任意のテンプレに差し替えたい場合は:

```toml
[new]
name_template = "memo-%Y-%m-%d_%H%M%S.md"   # `chira new` の name 省略時
dir_template  = "memo-%Y-%m-%d_%H%M%S"      # `chira mkdir` の name 省略時
```

- `chrono` のフォーマット指定子 ( https://docs.rs/chrono/latest/chrono/format/strftime/ ) がそのまま使え、評価結果が実際の name になる。
- 前後の空白は trim される (空白のみの値は未設定扱い)。
- 未設定 / 空文字なら上記のハードコード default にフォールバックする。
- TUI の `n` (新規ファイル) / `N` (新規ディレクトリ) の placeholder も同じテンプレを参照するため、CLI と TUI で挙動が揃う。
- 評価結果が name の安全条件 (空・`/` 含み・先頭 `.` のいずれか) に該当する場合、または `chrono` が解釈できない specifier (`%Q` 等) を含む場合は、起動時に警告を出して default にフォールバックする。

## アクション（作成と同時にコマンドを走らせる）

新しいディレクトリを作ったあとにスケルトンを rsync したり `git clone` したり `cookiecutter` をかけたり、任意のコマンドで bootstrap したい場合は `config.toml` に `[actions.<name>]` を定義する。`t` キーでピッカーを開き、新ディレクトリ名を入力し、解決された **コマンド全文を確認画面で表示**（信頼ゲート: config 由来の任意 shell を user 権限で実行する）してから、`y` でディレクトリを作成し `sh -c` でそのディレクトリ内で実行する。

```toml
# 隔離 dev shell を bootstrap
[actions.nix-sandbox]
description = "Nix flake + direnv の sandbox"
run = "rsync -a ~/.config/chira/skel/nix-sandbox/ ./ && direnv allow"

# sandbox repo を clone
[actions.clone-sandbox]
description = "自分の sandbox repo を clone"
run = "git clone --depth 1 git@github.com:me/sandbox.git ."

# 内容補間は専門ツールに委譲
[actions.from-copier]
run = "copier copy --trust ~/.config/chira/templates/app \"$CHIRA_TARGET\""
```

任意: 素の `N` で特定のアクションを自動実行する。既定無効。これを書くと `N` が `t` → `<name>` と同じ flow になる。**`default_action` は `[actions.*]` table より前に書くこと** (`[actions.*]` の後ろに書くと TOML 仕様で直前のテーブル内 key として解釈され、root の `default_action` には設定されず silently 無視される):

```toml
default_action = "nix-sandbox"

[actions.nix-sandbox]
# ...
```

- `description` はピッカー表示用（任意）。`run` は必須で、欠落・空文字のエントリは silently スキップされる。
- `run` は `/bin/sh -c` 経由で実行されるので `&&` / `|` / `~` 展開 / `$VAR` は通常通り動く。
- chira 自体はファイル内容やファイル名の補間を**しない**。テンプレ補間が必要なら `run` から専用ツール（`cookiecutter --no-input`、`copier copy --trust`、`envsubst`、`sed` 等）を呼ぶ。
- 新しいディレクトリが cwd。`run` には以下の環境変数も渡る:
  - `CHIRA_TARGET` — 新ディレクトリの絶対パス
  - `CHIRA_TARGET_NAME` — ディレクトリ名
  - `CHIRA_ROOT` — scratch のルート（`$CHIRA_DIR`）
- 非ゼロ終了時はディレクトリを**残す**（auto-rollback しない — 部分的な成果や調査情報を捨てない方針）。`.chira/bootstrap-failed` を中に書き、一覧でその dir の先頭に `[!]` を表示するので半端な dir を見分けられる。retry は `d` で削除してからアクションを再実行する（chira は常に新規 dir を作成し、既存名は reject する）。あるいは `.chira/bootstrap-failed` を手動削除すれば再実行せずに marker だけクリアできる。
- `default_action = "<name>"` を書くと素の `N` キーもピッカー無しで同じ confirm + run flow に流れる。未設定（既定）なら `N` は従来通り空ディレクトリ作成。存在しないアクション名は silently fallback して従来 `N` の挙動に戻る。
### TUI 内からの編集

TUI 起動中に `,` キーで設定画面を開く。各項目の現在値と source（`(env: CHIRA_DIR)` / `(config)` / `(default)`）、解決順、書き込み先の絶対パスが表示される。`Enter` で選択中の項目を編集（bool は `Space` で toggle）、`s` で同じ `config.toml` に書き戻し（フォーマットとコメントは保持）、`Esc` でファイル一覧に戻る。env で上書きされている項目には `⚠ env override` バッジが付く — config.toml には保存されるが、次回起動時も env が優先される。TUI で書き戻した内容は次回起動から有効（現在のセッションは起動時に読んだスナップショットで動き続ける）。

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
| `N` | 新規ディレクトリ（`default_action` 設定時は `t` + そのアクションと同じ flow に） |
| `t` | アクションから新規ディレクトリ — 選択 → 名前 → コマンド確認 → 実行（`[actions.*]`） |
| `r` | 名前を変更 |
| `d` | 削除（確認あり。ディレクトリは中身ごと） |
| `/` | 名前で絞り込み検索 |
| `,` | 設定画面を開く（`config.toml` を上書き編集） |
| `?` | ヘルプを画面に表示（何かキーで閉じる） |
| `q` | 終了 |

vim と同じく `h`/`j`/`k`/`l` で移動（`h`=親、`l`=開く）でき、方向キーでも操作可能。ヘルプは ranger/nnn と同じ `?`。ディレクトリを選ぶと右ペインに中身が `tree` 風（深さ 4・最大 100 行）で表示される。

`$EDITOR`（未設定なら `vi`）や `s` で開いたシェル（未設定なら `/bin/sh`）を終了すると TUI に復帰し、その間に作られたファイルも一覧へ自動反映される。作成・改名・削除はディスクへ即反映される。

## CLI サブコマンド

引数なしの `chira` は従来どおり TUI を起動する。サブコマンドを渡すと 1 ショットの CLI として動く。`chira ls | fzf` のようなパイプ連携、スクリプト、TUI を立ち上げずに `cd` だけしたいケース等に使う。

| サブコマンド | TUI 対応キー | 説明 |
|---|---|---|
| `chira ls [<path>]` | （一覧表示） | 1 行 1 件で名前のみ。`-l` で `<mtime>\t<size>\t<name>` |
| `chira tree [<path>]` | （右ペイン） | `tree` 風表示（深さ 4・最大 100 行） |
| `chira new [<name>]` | `n` | 新規ファイル作成 + `$EDITOR` で開く（`--no-edit` でエディタを開かない、`<name>` 省略時は `[new] name_template` で評価。default は `scratch-%Y%m%d-%H%M%S.md`） |
| `chira mkdir [<name>]` | `N` | 新規ディレクトリ作成（`<name>` 省略時は `[new] dir_template` で評価。default は `scratch-%Y%m%d-%H%M%S`） |
| `chira edit <name>` | `e` | `<name>` を `$EDITOR` で開く |
| `chira shell [<dir>]` | `s` | `<dir>`（省略時は `CHIRA_DIR`）で `$SHELL` を開く |
| `chira rm <name>` | `d` | 削除。ディレクトリは `-r` 必須、`-f` で確認スキップ |
| `chira mv <old> <new>` | `r` | リネーム |
| `chira path [<name>]` | — | エントリのフルパスを出力（省略時は `CHIRA_DIR`） |
| `chira find <query> [<path>]` | `/` | 名前で絞り込み一覧（substring match、`ls` 同様の書式） |
| `chira gc [--ttl <dur>] [--archive-dir <path>] [--dry-run]` | — | `mtime` が TTL を超えたエントリを archive へ移動（下記参照） |

出力は機械可読寄り。`ls` / `find` は 1 行 1 名前で、色やディレクトリの末尾 `/` は stdout が TTY のときだけ付く。エラーは stderr、不在エントリは exit 1、引数誤りは exit 2 になる。破壊的操作（`rm` / `mv`）は対象パスが `CHIRA_DIR` 配下にあることを canonicalize して検証する（`..` や symlink 経由の root escape は拒否）。symlink に対する `rm` は unix の `rm` 同様 symlink 自体を消す（target は辿らない）。非対話（stdin が非 TTY）では `rm` は `-f` が必須で、未指定時は確認プロンプトが自動キャンセルされ exit 1 になる。`rm` / `mv` は scratch root 自身（`.` / 空文字列）への操作を拒否する（`CHIRA_DIR` 全消し防止）。

`new` / `mkdir` は basename だけ受け取り、必ず `CHIRA_DIR` root 直下にエントリを作成する（名前に `/` を含むと拒否）。他のコマンド（`ls`, `tree`, `edit`, `shell`, `rm`, `mv`, `path`, `find`）は root 相対パスを受け付け、ネストしたエントリ（例: `chira edit ws/note.md`）を直接指定できる。subdir 内で新規作成したいときは `chira shell ws` でその場所のシェルに入ってから作業する。

`chira path` を使うと TUI を立ち上げずに shell 側で `cd` できる:

```sh
cd "$(chira path)"               # CHIRA_DIR へ cd
cd "$(chira path my-experiment)" # 任意エントリへ cd
```

## アーカイブ（chira gc）

chira のエントリは使い捨て前提だが、手で消さない限り溜まり続ける。`chira gc` は `mtime` が TTL を超えたエントリを `<CHIRA_DIR>/.archive/` 配下へ move する（隠しディレクトリなので一覧からは消える）。アーカイブ後も素のファイル/ディレクトリのまま残るので、`find` / `grep` で発掘できる。

```sh
chira gc --ttl 30d              # 30 日触っていないエントリを archive
chira gc --ttl 12h --dry-run    # 対象だけ表示（move しない）
chira gc --archive-dir ~/old    # archive 先を別の場所にする
```

時間単位: `s` / `m` / `h` / `d` / `w`（単位なしは秒）。TTL は必須で、`--ttl` も `[archive] ttl_days` も無いと `chira gc` はエラーで終了する（未設定で実行して全消えする事故を防ぐため）。

判定はエントリ自身の `mtime` のみ（`symlink_metadata` を使うため、正常な symlink はリンク自体の寿命で判定、リンク先は見ない）。relatime / noatime mount では atime が更新されないため atime は使わない。target が存在しない dangling symlink は「mtime 取れない」扱いで `errors` に積まれ、リンク自身の mtime では判定しない (broken symlink を意図せず archive しないため)。

### 対象外

TTL を超えても以下は移動せず、summary では `kept` にカウントされる:

- `.archive/` 自身（再帰防止）
- ディレクトリ直下に `.keep` ファイルがあるもの（lf / nnn 等の慣習）
- `[archive] keep` の glob にマッチする名前（下記参照）

mtime が取れないエントリ（壊れた symlink 等）は **errors** として別カテゴリ扱い — skip + stderr warning + `errors` にカウントされ、`kept` には含まれない。summary 行は `archived / kept / errors` を独立した 3 区分で表示し、`errors > 0` の場合は exit 1 で cron 等から検知できる。

### 設定（`[archive]`）

```toml
[archive]
# TTL（日数）。0 または未設定なら archive 機能 off（CLI --ttl による単発実行は可能）
ttl_days = 30

# archive 先。~ は展開される。省略時は <CHIRA_DIR>/.archive
dir = "~/scratch-archive"

# TUI 起動時に sweep するか。default false（毎回 sweep されると驚きが大きいため）
on_startup = false

# 名前がこれらの glob にマッチするものは保持する。末尾 `/` でディレクトリ限定
keep = ["pinned-*", "longterm/"]
```

glob は `*`（任意の連続）と `?`（任意の 1 文字）のみ対応。末尾 `/` はディレクトリだけにマッチさせる。

archive 先で同名衝突が起きた場合は `.<unix_ts>` suffix が付く（例: `old.md.1742278300`）。同じ秒に二度衝突したら `_1`, `_2` … と連番でユニーク化する。`dir` が `CHIRA_DIR` と別 filesystem だと `fs::rename` が `EXDEV` で失敗するので、その場合は同じ filesystem を指すこと（エラーは stderr に出る）。

## 開発

```sh
cargo run               # ソースから実行
cargo build --release   # 単一バイナリをビルド
```

## License

MIT
