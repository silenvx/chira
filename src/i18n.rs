use std::env;
use std::sync::OnceLock;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lang {
    En,
    Ja,
}

static LANG: OnceLock<Lang> = OnceLock::new();

pub fn lang() -> Lang {
    *LANG.get_or_init(detect_lang)
}

/// 環境変数から表示言語を決定する。優先順位:
/// 1. `CHIRA_LANG` (明示的な override。case-insensitive で `ja` / `ja_jp` / `japanese`
///    → Ja、`en` / `en_us` / `english` → En。それ以外の値は無視して locale へフォールバック)
/// 2. POSIX locale: `LC_ALL` → `LC_MESSAGES` → `LANG`、`ja*` で Ja、他は En
/// 3. いずれも未設定なら En (global 公開向けの default)
fn detect_lang() -> Lang {
    resolve_lang(
        env::var("CHIRA_LANG").ok().as_deref(),
        env::var("LC_ALL").ok().as_deref(),
        env::var("LC_MESSAGES").ok().as_deref(),
        env::var("LANG").ok().as_deref(),
    )
}

/// detect_lang() のロジックを env から分離した純粋関数。production / test 双方から呼ぶ。
fn resolve_lang(
    chira_lang: Option<&str>,
    lc_all: Option<&str>,
    lc_messages: Option<&str>,
    lang_env: Option<&str>,
) -> Lang {
    if let Some(v) = chira_lang {
        match v.to_lowercase().as_str() {
            "ja" | "ja_jp" | "japanese" => return Lang::Ja,
            "en" | "en_us" | "english" => return Lang::En,
            _ => {}
        }
    }
    for v in [lc_all, lc_messages, lang_env].into_iter().flatten() {
        if v.is_empty() {
            continue;
        }
        if v.to_lowercase().starts_with("ja") {
            return Lang::Ja;
        }
        return Lang::En;
    }
    Lang::En
}

pub fn usage(lang: Lang) -> &'static str {
    match lang {
        Lang::Ja => {
            "\
chira — 一時的な scratch ディレクトリを管理する TUI/CLI

usage: chira                          TUI を起動する
       chira [--cd-file <path>]       TUI 起動 (終了時に最終 dir を書き出す)
       chira <subcommand> [args]      CLI として 1 ショット実行

TUI オプション:
  --cd-file <path>   終了時に最終ディレクトリを <path> へ書き出す
                     (シェル関数で cd するための連携用。README 参照)
  -h, --help         このヘルプを表示
  -V, --version      バージョン情報を表示

サブコマンド:
  ls [<path>]              エントリ一覧 (-l で <mtime>\\t<size>\\t<name>)
  tree [<path>]            ディレクトリ構造を tree 風に表示 (深さ 4 / 100 行)
  new [<name>] [--no-edit] 新規ファイル作成 + $EDITOR を開く (省略時は scratch-YYYYMMDD-HHMMSS.md)
  mkdir [<name>]           新規ディレクトリ作成 (省略時は scratch-YYYYMMDD-HHMMSS)
  edit <name>              $EDITOR でファイルを開く
  shell [<dir>]            指定ディレクトリ (省略時は root) で $SHELL を開く
  rm <name> [-r] [-f]      削除 (dir は -r、-f で確認スキップ)
  mv <old> <new>           リネーム
  path [<name>]            エントリのフルパスを出力 (cd 連携用)
  find <query> [<path>]    名前で絞り込み一覧 (substring match)
  gc [--ttl <dur>] [--archive-dir <path>] [--dry-run]
                           mtime が TTL を超えたエントリを archive へ移動
"
        }
        Lang::En => {
            "\
chira — Manage throwaway scratch directories from a TUI/CLI.

usage: chira                          Launch the TUI
       chira [--cd-file <path>]       Launch the TUI (write final dir on exit)
       chira <subcommand> [args]      One-shot CLI invocation

TUI options:
  --cd-file <path>   On exit, write the final directory to <path>
                     (for shell-function cd integration; see README)
  -h, --help         Show this help
  -V, --version      Print version information

Subcommands:
  ls [<path>]              List entries (-l for <mtime>\\t<size>\\t<name>)
  tree [<path>]            Print a tree view (depth 4 / 100 lines)
  new [<name>] [--no-edit] Create a file and open $EDITOR (default: scratch-YYYYMMDD-HHMMSS.md)
  mkdir [<name>]           Create a directory (default: scratch-YYYYMMDD-HHMMSS)
  edit <name>              Open a file in $EDITOR
  shell [<dir>]            Open $SHELL in the given directory (default: root)
  rm <name> [-r] [-f]      Delete (-r for dirs, -f to skip confirm)
  mv <old> <new>           Rename
  path [<name>]            Print the full path of an entry (for cd integration)
  find <query> [<path>]    List entries whose name matches the substring
  gc [--ttl <dur>] [--archive-dir <path>] [--dry-run]
                           Move entries whose mtime exceeds TTL to the archive dir
"
        }
    }
}

pub fn err_cd_file_needs_arg(lang: Lang) -> &'static str {
    match lang {
        Lang::Ja => "--cd-file には引数が必要です\n",
        Lang::En => "--cd-file requires an argument\n",
    }
}

pub fn err_option_needs_arg(lang: Lang, opt: &str) -> String {
    match lang {
        Lang::Ja => format!("{opt} には引数が必要です\n"),
        Lang::En => format!("{opt} requires an argument\n"),
    }
}

pub fn err_gc_ttl_missing(lang: Lang) -> &'static str {
    match lang {
        Lang::Ja => {
            "TTL が指定されていません。--ttl <dur> または config.toml の [archive] ttl_days を設定してください\n"
        }
        Lang::En => "No TTL specified. Pass --ttl <dur> or set [archive] ttl_days in config.toml\n",
    }
}

pub fn err_gc_ttl_invalid(lang: Lang, e: &dyn std::fmt::Display) -> String {
    match lang {
        Lang::Ja => format!("--ttl の解釈に失敗: {e}\n"),
        Lang::En => format!("Failed to parse --ttl: {e}\n"),
    }
}

pub fn err_gc_sweep(lang: Lang, e: &dyn std::fmt::Display) -> String {
    match lang {
        Lang::Ja => format!("archive 中にエラー: {e}\n"),
        Lang::En => format!("Error while archiving: {e}\n"),
    }
}

pub fn warn_archive_mkdir(
    lang: Lang,
    path: &dyn std::fmt::Display,
    e: &dyn std::fmt::Display,
) -> String {
    match lang {
        Lang::Ja => format!("archive ディレクトリの作成に失敗 ({path}): {e}"),
        Lang::En => format!("Failed to create archive directory ({path}): {e}"),
    }
}

pub fn warn_archive_move(
    lang: Lang,
    name: &str,
    dest: &dyn std::fmt::Display,
    e: &dyn std::fmt::Display,
) -> String {
    match lang {
        Lang::Ja => format!("{name} の archive に失敗 (→ {dest}): {e}"),
        Lang::En => format!("Failed to archive {name} (→ {dest}): {e}"),
    }
}

pub fn warn_archive_mtime(lang: Lang, name: &str, e: &dyn std::fmt::Display) -> String {
    match lang {
        Lang::Ja => format!("{name} の mtime 取得に失敗 (skip): {e}"),
        Lang::En => format!("Failed to read mtime for {name} (skipped): {e}"),
    }
}

pub fn warn_archive_keep_probe(lang: Lang, name: &str, e: &dyn std::fmt::Display) -> String {
    match lang {
        Lang::Ja => format!("{name} の .keep 確認に失敗 (skip、保護側に倒す): {e}"),
        Lang::En => {
            format!("Failed to probe .keep in {name} (skipped, erring on the protected side): {e}")
        }
    }
}

pub fn gc_dry_run_header(lang: Lang) -> &'static str {
    match lang {
        Lang::Ja => "dry-run: 以下のエントリを archive します",
        Lang::En => "dry-run: the following entries would be archived",
    }
}

pub fn gc_dry_run_entry(name: &str, dest: &dyn std::fmt::Display) -> String {
    format!("  {name} → {dest}")
}

pub fn gc_summary(lang: Lang, archived: usize, kept: usize, errors: usize) -> String {
    match lang {
        Lang::Ja => format!("archive 完了: {archived} 件移動 / {kept} 件保持 / {errors} 件エラー"),
        Lang::En => {
            format!("archived: {archived} moved / {kept} kept / {errors} errors")
        }
    }
}

pub fn gc_summary_dry_run(lang: Lang, candidates: usize, kept: usize, errors: usize) -> String {
    match lang {
        Lang::Ja => {
            format!("dry-run: {candidates} 件が候補 / {kept} 件は対象外 / {errors} 件エラー")
        }
        Lang::En => format!("dry-run: {candidates} candidate(s) / {kept} kept / {errors} errors"),
    }
}

pub fn gc_archived(lang: Lang, name: &str, dest: &dyn std::fmt::Display) -> String {
    match lang {
        Lang::Ja => format!("移動: {name} → {dest}"),
        Lang::En => format!("moved: {name} → {dest}"),
    }
}

pub fn err_unknown_arg(lang: Lang, arg: &str) -> String {
    match lang {
        Lang::Ja => format!("不明な引数: {arg}\n{}", usage(lang)),
        Lang::En => format!("Unknown argument: {arg}\n{}", usage(lang)),
    }
}

pub fn warn_config_parse(
    lang: Lang,
    path: &dyn std::fmt::Display,
    e: &dyn std::fmt::Display,
) -> String {
    match lang {
        Lang::Ja => format!("設定ファイルの解析に失敗 ({path}): {e} — デフォルト設定で起動します"),
        Lang::En => format!("Failed to parse config file ({path}): {e} — starting with defaults"),
    }
}

pub fn warn_config_unreadable(
    lang: Lang,
    path: &dyn std::fmt::Display,
    e: &dyn std::fmt::Display,
) -> String {
    match lang {
        Lang::Ja => {
            format!("設定ファイルの読み取りに失敗 ({path}): {e} — デフォルト設定で起動します")
        }
        Lang::En => format!("Failed to read config file ({path}): {e} — starting with defaults"),
    }
}

pub fn err_external_launch(lang: Lang, e: &dyn std::fmt::Display) -> String {
    match lang {
        Lang::Ja => format!("外部プロセスの起動に失敗: {e}"),
        Lang::En => format!("Failed to launch external process: {e}"),
    }
}

pub fn err_command_parse(lang: Lang, e: &dyn std::fmt::Display) -> String {
    match lang {
        Lang::Ja => format!("コマンドの解析に失敗: {e}"),
        Lang::En => format!("failed to parse command: {e}"),
    }
}

pub fn err_command_empty(lang: Lang) -> &'static str {
    match lang {
        Lang::Ja => "コマンドが空です",
        Lang::En => "command is empty",
    }
}

pub fn empty_directory(lang: Lang) -> &'static str {
    match lang {
        Lang::Ja => "(空のディレクトリ)",
        Lang::En => "(empty directory)",
    }
}

pub fn err_unreadable(lang: Lang, e: &dyn std::fmt::Display) -> String {
    match lang {
        Lang::Ja => format!("(読み取り不可: {e})"),
        Lang::En => format!("(unreadable: {e})"),
    }
}

pub fn status_deleted(lang: Lang, name: &str) -> String {
    match lang {
        Lang::Ja => format!("削除しました: {name}"),
        Lang::En => format!("Deleted: {name}"),
    }
}

pub fn status_delete_failed(lang: Lang, e: &dyn std::fmt::Display) -> String {
    match lang {
        Lang::Ja => format!("削除に失敗: {e}"),
        Lang::En => format!("Failed to delete: {e}"),
    }
}

pub fn status_create_failed(lang: Lang, e: &dyn std::fmt::Display) -> String {
    match lang {
        Lang::Ja => format!("作成に失敗: {e}"),
        Lang::En => format!("Failed to create: {e}"),
    }
}

pub fn status_created_dir(lang: Lang, name: &str) -> String {
    match lang {
        Lang::Ja => format!("作成しました: {name}/"),
        Lang::En => format!("Created: {name}/"),
    }
}

pub fn status_no_actions(lang: Lang) -> &'static str {
    match lang {
        Lang::Ja => "アクションが設定されていません ([actions.*] を config.toml に追加)",
        Lang::En => "No actions configured (add [actions.*] to config.toml)",
    }
}

/// 文言は過去形。status は外部プロセス復帰後に初めて描画される (実行中は TUI を抜けている) ため、
/// "実行しました" の方が表示時点の状態に一致する。
pub fn status_run_action(lang: Lang, name: &str) -> String {
    match lang {
        Lang::Ja => format!("アクション実行しました: {name}"),
        Lang::En => format!("Ran action: {name}"),
    }
}

pub fn status_renamed(lang: Lang) -> &'static str {
    match lang {
        Lang::Ja => "名前を変更しました",
        Lang::En => "Renamed",
    }
}

pub fn status_rename_failed(lang: Lang, e: &dyn std::fmt::Display) -> String {
    match lang {
        Lang::Ja => format!("変更に失敗: {e}"),
        Lang::En => format!("Failed to rename: {e}"),
    }
}

pub fn preview_special_file(lang: Lang) -> &'static str {
    match lang {
        Lang::Ja => "(特殊ファイル: プレビュー不可)",
        Lang::En => "(special file: preview unavailable)",
    }
}

pub fn preview_large_file(lang: Lang, bytes: u64) -> String {
    match lang {
        Lang::Ja => format!("(大きいファイル: {bytes} bytes)"),
        Lang::En => format!("(large file: {bytes} bytes)"),
    }
}

pub fn preview_binary(lang: Lang) -> &'static str {
    match lang {
        Lang::Ja => "(バイナリ/読み取り不可)",
        Lang::En => "(binary / unreadable)",
    }
}

pub fn preview_unreadable(lang: Lang, e: &dyn std::fmt::Display) -> String {
    match lang {
        Lang::Ja => format!("(読み取り不可: {e})"),
        Lang::En => format!("(unreadable: {e})"),
    }
}

pub fn header_count(lang: Lang, n: usize) -> String {
    match lang {
        Lang::Ja => format!("{n} 件"),
        Lang::En => format!("{n} items"),
    }
}

pub fn header_search(lang: Lang, query: &str, cursor: &str) -> String {
    match lang {
        Lang::Ja => format!("検索: {query}{cursor}"),
        Lang::En => format!("Search: {query}{cursor}"),
    }
}

pub fn list_title(lang: Lang) -> &'static str {
    match lang {
        Lang::Ja => " 一覧 ",
        Lang::En => " List ",
    }
}

pub fn preview_dir_title(lang: Lang) -> &'static str {
    match lang {
        Lang::Ja => " ディレクトリ内容 ",
        Lang::En => " Directory contents ",
    }
}

pub fn preview_file_title(lang: Lang) -> &'static str {
    match lang {
        Lang::Ja => " プレビュー ",
        Lang::En => " Preview ",
    }
}

pub fn empty_hint(lang: Lang) -> &'static str {
    match lang {
        Lang::Ja => "空です。n: ファイル作成  N: ディレクトリ作成",
        Lang::En => "Empty. n: new file  N: new directory",
    }
}

pub fn empty_search_hint(lang: Lang) -> &'static str {
    match lang {
        Lang::Ja => "一致するエントリがありません。",
        Lang::En => "No matching entries.",
    }
}

pub fn input_title_new_file(lang: Lang) -> &'static str {
    match lang {
        Lang::Ja => " 新規ファイル名 ",
        Lang::En => " New file name ",
    }
}

pub fn input_title_new_dir(lang: Lang) -> &'static str {
    match lang {
        Lang::Ja => " 新規ディレクトリ名 ",
        Lang::En => " New directory name ",
    }
}

pub fn input_title_rename(lang: Lang) -> &'static str {
    match lang {
        Lang::Ja => " 名前を変更 ",
        Lang::En => " Rename ",
    }
}

pub fn confirm_delete_dir(lang: Lang, name: &str) -> String {
    match lang {
        Lang::Ja => format!("ディレクトリ「{name}」を中身ごと削除しますか?"),
        Lang::En => format!("Delete directory \"{name}\" and all its contents?"),
    }
}

pub fn confirm_delete_file(lang: Lang, name: &str) -> String {
    match lang {
        Lang::Ja => format!("「{name}」を削除しますか?"),
        Lang::En => format!("Delete \"{name}\"?"),
    }
}

pub fn confirm_delete_label(lang: Lang) -> &'static str {
    match lang {
        Lang::Ja => ": 削除   ",
        Lang::En => ": delete   ",
    }
}

pub fn confirm_cancel_label(lang: Lang) -> &'static str {
    match lang {
        Lang::Ja => ": キャンセル",
        Lang::En => ": cancel",
    }
}

pub fn confirm_title(lang: Lang) -> &'static str {
    match lang {
        Lang::Ja => " 確認 ",
        Lang::En => " Confirm ",
    }
}

pub fn action_pick_title(lang: Lang) -> &'static str {
    match lang {
        Lang::Ja => " アクションを選択 ",
        Lang::En => " Choose action ",
    }
}

pub fn confirm_action_prompt(lang: Lang, name: &str) -> String {
    match lang {
        Lang::Ja => format!("新規「{name}/」で以下を実行します:"),
        Lang::En => format!("Run the following in new \"{name}/\":"),
    }
}

pub fn confirm_action_run_label(lang: Lang) -> &'static str {
    match lang {
        Lang::Ja => ": 実行   ",
        Lang::En => ": run   ",
    }
}

pub fn help_title(lang: Lang) -> &'static str {
    match lang {
        Lang::Ja => " ヘルプ (vim-like) ",
        Lang::En => " Help (vim-like) ",
    }
}

pub fn help_rows(lang: Lang) -> &'static [(&'static str, &'static str)] {
    match lang {
        Lang::Ja => &[
            ("j / k, ↓ / ↑", "カーソル移動"),
            ("g / G", "先頭 / 末尾"),
            ("l / → / Enter", "開く (ファイル→$EDITOR, dir→中へ)"),
            ("h / ← / Backspace", "親ディレクトリへ戻る"),
            ("e", "$EDITOR で開く"),
            ("s", "シェルを開く (実験・agent 実行)"),
            ("n / N", "新規ファイル / ディレクトリ"),
            ("t", "アクションで新規 (テンプレ/clone/script)"),
            ("r", "名前を変更"),
            ("d", "削除 (確認あり)"),
            ("/", "名前で絞り込み検索"),
            ("?", "このヘルプ"),
            (",", "設定"),
            ("q", "終了"),
        ],
        Lang::En => &[
            ("j / k, ↓ / ↑", "Move cursor"),
            ("g / G", "Top / bottom"),
            ("l / → / Enter", "Open (file → $EDITOR, dir → enter)"),
            ("h / ← / Backspace", "Go to parent directory"),
            ("e", "Open in $EDITOR"),
            ("s", "Open shell (experiments / run agents)"),
            ("n / N", "New file / directory"),
            ("t", "New dir from an action (template/clone/script)"),
            ("r", "Rename"),
            ("d", "Delete (with confirmation)"),
            ("/", "Filter by name"),
            ("?", "This help"),
            (",", "Configuration"),
            ("q", "Quit"),
        ],
    }
}

pub fn footer_browse(lang: Lang) -> &'static str {
    match lang {
        Lang::Ja => {
            "j/k:移動  l:開く  h:親  s:シェル  n:新規  t:アクション  /:検索  ,:設定  ?:ヘルプ  q:終了"
        }
        Lang::En => {
            "j/k:move  l:open  h:parent  s:shell  n:new  t:action  /:filter  ,:config  ?:help  q:quit"
        }
    }
}

pub fn footer_action_pick(lang: Lang) -> &'static str {
    match lang {
        Lang::Ja => "j/k:移動  Enter:選択  Esc:キャンセル",
        Lang::En => "j/k:move  Enter:select  Esc:cancel",
    }
}

pub fn footer_confirm_action(lang: Lang) -> &'static str {
    match lang {
        Lang::Ja => "y:実行  n/Esc:キャンセル",
        Lang::En => "y:run  n/Esc:cancel",
    }
}

pub fn footer_search(lang: Lang) -> &'static str {
    match lang {
        Lang::Ja => "文字入力で絞り込み  Enter:確定  Esc:クリア",
        Lang::En => "Type to filter  Enter:confirm  Esc:clear",
    }
}

pub fn footer_input(lang: Lang) -> &'static str {
    match lang {
        Lang::Ja => "Enter:決定  Esc:キャンセル",
        Lang::En => "Enter:confirm  Esc:cancel",
    }
}

pub fn footer_confirm(lang: Lang) -> &'static str {
    match lang {
        Lang::Ja => "y:削除  n/Esc:キャンセル",
        Lang::En => "y:delete  n/Esc:cancel",
    }
}

pub fn footer_help_close(lang: Lang) -> &'static str {
    match lang {
        Lang::Ja => "何かキーを押すと閉じる",
        Lang::En => "Press any key to close",
    }
}

pub fn err_unknown_subcommand(lang: Lang, sub: &str) -> String {
    match lang {
        Lang::Ja => format!("不明なサブコマンド: {sub}\n{}", usage(lang)),
        Lang::En => format!("Unknown subcommand: {sub}\n{}", usage(lang)),
    }
}

pub fn err_cli_unknown_flag(lang: Lang, sub: &str, flag: &str) -> String {
    match lang {
        Lang::Ja => format!("{sub}: 不明なオプション: {flag}"),
        Lang::En => format!("{sub}: unknown option: {flag}"),
    }
}

pub fn err_cli_too_many_args(lang: Lang, sub: &str) -> String {
    match lang {
        Lang::Ja => format!("{sub}: 引数が多すぎます"),
        Lang::En => format!("{sub}: too many arguments"),
    }
}

pub fn err_cli_arg_required(lang: Lang, sub: &str, what: &str) -> String {
    match lang {
        Lang::Ja => format!("{sub}: 引数が必要です: {what}"),
        Lang::En => format!("{sub}: argument required: {what}"),
    }
}

pub fn err_cli_op(lang: Lang, sub: &str, e: &dyn std::fmt::Display) -> String {
    match lang {
        Lang::Ja => format!("{sub}: {e}"),
        Lang::En => format!("{sub}: {e}"),
    }
}

pub fn err_cli_root(lang: Lang, e: &dyn std::fmt::Display) -> String {
    match lang {
        Lang::Ja => format!("scratch root の解決に失敗: {e}"),
        Lang::En => format!("Failed to resolve scratch root: {e}"),
    }
}

pub fn err_cli_not_a_directory(lang: Lang, path: &dyn std::fmt::Display) -> String {
    match lang {
        Lang::Ja => format!("shell: ディレクトリではありません: {path}"),
        Lang::En => format!("shell: not a directory: {path}"),
    }
}

pub fn err_cli_rm_dir_needs_r(lang: Lang, name: &str) -> String {
    match lang {
        Lang::Ja => format!("rm: ディレクトリの削除には -r が必要です: {name}"),
        Lang::En => format!("rm: cannot remove directory '{name}' without -r"),
    }
}

pub fn status_cli_rm_cancelled(lang: Lang) -> &'static str {
    match lang {
        Lang::Ja => "rm: キャンセルしました",
        Lang::En => "rm: cancelled",
    }
}

// ─── Config TUI ──────────────────────────────────────────────────────────────

pub fn config_title(lang: Lang) -> &'static str {
    match lang {
        Lang::Ja => " 設定 ",
        Lang::En => " Configuration ",
    }
}

pub fn config_section_general(lang: Lang) -> &'static str {
    match lang {
        Lang::Ja => "一般",
        Lang::En => "general",
    }
}

pub fn config_section_archive(_lang: Lang) -> &'static str {
    "archive"
}

pub fn config_source_env(_lang: Lang, var: &str) -> String {
    format!("(env: {var})")
}

pub fn config_source_config(_lang: Lang) -> &'static str {
    "(config)"
}

pub fn config_source_default(_lang: Lang) -> &'static str {
    "(default)"
}

pub fn config_env_override_badge(lang: Lang) -> &'static str {
    match lang {
        Lang::Ja => "⚠ env が優先",
        Lang::En => "⚠ env override",
    }
}

pub fn config_saves_to(lang: Lang, path: &dyn std::fmt::Display) -> String {
    match lang {
        Lang::Ja => format!("保存先: {path}"),
        Lang::En => format!("Saves to: {path}"),
    }
}

pub fn config_saves_to_unknown(lang: Lang) -> &'static str {
    match lang {
        Lang::Ja => "保存先: $HOME も $XDG_CONFIG_HOME も解決できないため保存不能",
        Lang::En => "Saves to: cannot save (neither $HOME nor $XDG_CONFIG_HOME resolves)",
    }
}

pub fn config_env_override_note(lang: Lang, var: &str) -> String {
    match lang {
        Lang::Ja => format!(
            "⚠ 現在は env ${var} で上書きされています。config.toml には保存しますが、起動時は env が優先されます。"
        ),
        Lang::En => format!(
            "⚠ Currently overridden by env ${var}. Saving will update config.toml but the env var will take precedence at startup."
        ),
    }
}

/// 設定項目 1 行のラベル・型ヒント・説明・解決順。TUI render が下部のヘルプ表示で参照する。
pub struct ConfigItemHelp {
    pub label: &'static str,
    pub type_hint: &'static str,
    pub description: &'static str,
    pub resolution: &'static str,
}

pub fn config_item_dir(lang: Lang) -> ConfigItemHelp {
    match lang {
        Lang::Ja => ConfigItemHelp {
            label: "dir",
            type_hint: "string",
            description: "scratch エントリの保存先ディレクトリ。",
            resolution: "$CHIRA_DIR → config.dir → $XDG_DATA_HOME/chira → ~/.local/share/chira",
        },
        Lang::En => ConfigItemHelp {
            label: "dir",
            type_hint: "string",
            description: "Storage location for scratch entries.",
            resolution: "$CHIRA_DIR → config.dir → $XDG_DATA_HOME/chira → ~/.local/share/chira",
        },
    }
}

pub fn config_item_editor(lang: Lang) -> ConfigItemHelp {
    match lang {
        Lang::Ja => ConfigItemHelp {
            label: "editor",
            type_hint: "string",
            description: "ファイルを開くときに使う $EDITOR (引数可)。",
            resolution: "$EDITOR → config.editor → vi",
        },
        Lang::En => ConfigItemHelp {
            label: "editor",
            type_hint: "string",
            description: "Editor command used to open files (arguments allowed).",
            resolution: "$EDITOR → config.editor → vi",
        },
    }
}

pub fn config_item_shell(lang: Lang) -> ConfigItemHelp {
    match lang {
        Lang::Ja => ConfigItemHelp {
            label: "shell",
            type_hint: "string",
            description: "`s` で開く $SHELL (引数可)。",
            resolution: "$SHELL → config.shell → /bin/sh",
        },
        Lang::En => ConfigItemHelp {
            label: "shell",
            type_hint: "string",
            description: "Shell opened with `s` (arguments allowed).",
            resolution: "$SHELL → config.shell → /bin/sh",
        },
    }
}

pub fn config_item_archive_ttl(lang: Lang) -> ConfigItemHelp {
    match lang {
        Lang::Ja => ConfigItemHelp {
            label: "archive.ttl_days",
            type_hint: "u64 (日)",
            description: "mtime がこの日数を超えたエントリを archive 対象にする。0 or 未設定で off。",
            resolution: "config 値のみ (env 経由なし)",
        },
        Lang::En => ConfigItemHelp {
            label: "archive.ttl_days",
            type_hint: "u64 (days)",
            description: "Entries whose mtime exceeds this are archived. 0 / unset disables archive.",
            resolution: "config only (no env override)",
        },
    }
}

pub fn config_item_archive_dir(lang: Lang) -> ConfigItemHelp {
    match lang {
        Lang::Ja => ConfigItemHelp {
            label: "archive.dir",
            type_hint: "string",
            description: "archive 先ディレクトリ。未設定で <CHIRA_DIR>/.archive を使う。",
            resolution: "config 値のみ",
        },
        Lang::En => ConfigItemHelp {
            label: "archive.dir",
            type_hint: "string",
            description: "Archive destination. Defaults to <CHIRA_DIR>/.archive when unset.",
            resolution: "config only",
        },
    }
}

pub fn config_item_archive_on_startup(lang: Lang) -> ConfigItemHelp {
    match lang {
        Lang::Ja => ConfigItemHelp {
            label: "archive.on_startup",
            type_hint: "bool",
            description: "TUI 起動時に archive sweep を走らせるか。",
            resolution: "config 値のみ",
        },
        Lang::En => ConfigItemHelp {
            label: "archive.on_startup",
            type_hint: "bool",
            description: "Run archive sweep when the TUI starts.",
            resolution: "config only",
        },
    }
}

pub fn config_item_archive_keep(lang: Lang) -> ConfigItemHelp {
    match lang {
        Lang::Ja => ConfigItemHelp {
            label: "archive.keep",
            type_hint: "string[]",
            description: "archive 対象から除外するファイル名 glob (例: pinned-*, longterm/)。",
            resolution: "config 値のみ",
        },
        Lang::En => ConfigItemHelp {
            label: "archive.keep",
            type_hint: "string[]",
            description: "Globs whose names are kept (e.g. pinned-*, longterm/).",
            resolution: "config only",
        },
    }
}

pub fn footer_config(lang: Lang) -> &'static str {
    match lang {
        Lang::Ja => "↑↓:選択  Enter:編集  Space:toggle  s:保存  Esc:戻る",
        Lang::En => "↑↓:select  Enter:edit  Space:toggle  s:save  Esc:back",
    }
}

pub fn footer_config_edit(lang: Lang) -> &'static str {
    match lang {
        Lang::Ja => "Enter:確定  Esc:キャンセル",
        Lang::En => "Enter:confirm  Esc:cancel",
    }
}

pub fn footer_config_keep(lang: Lang) -> &'static str {
    match lang {
        Lang::Ja => "↑↓:選択  a:追加  d:削除  Enter:編集  Esc:戻る",
        Lang::En => "↑↓:select  a:add  d:remove  Enter:edit  Esc:back",
    }
}

pub fn status_config_saved(lang: Lang, path: &dyn std::fmt::Display) -> String {
    match lang {
        Lang::Ja => format!("保存しました: {path}"),
        Lang::En => format!("Saved: {path}"),
    }
}

pub fn status_config_save_failed(lang: Lang, e: &dyn std::fmt::Display) -> String {
    match lang {
        Lang::Ja => format!("保存に失敗: {e}"),
        Lang::En => format!("Failed to save: {e}"),
    }
}

pub fn status_config_invalid_number(lang: Lang) -> &'static str {
    match lang {
        Lang::Ja => "数値として解釈できません (0 以上の整数を入力)",
        Lang::En => "Not a valid non-negative integer",
    }
}

pub fn status_config_ttl_too_large(lang: Lang) -> &'static str {
    match lang {
        Lang::Ja => "値が大きすぎます (ttl_days * 86_400 が u64 を超えます)",
        Lang::En => "Value too large (ttl_days * 86_400 would overflow u64)",
    }
}

pub fn status_config_no_save_path(lang: Lang) -> &'static str {
    match lang {
        Lang::Ja => "保存先のパスが解決できません ($HOME も $XDG_CONFIG_HOME も未設定)",
        Lang::En => "No save path resolves ($HOME and $XDG_CONFIG_HOME both unset)",
    }
}

pub fn config_keep_title(lang: Lang) -> &'static str {
    match lang {
        Lang::Ja => " archive.keep ",
        Lang::En => " archive.keep ",
    }
}

pub fn config_keep_empty(lang: Lang) -> &'static str {
    match lang {
        Lang::Ja => "  (空)  a で追加",
        Lang::En => "  (empty)  press a to add",
    }
}

pub fn config_input_title(lang: Lang, key: &str) -> String {
    match lang {
        Lang::Ja => format!(" 編集: {key} "),
        Lang::En => format!(" Edit: {key} "),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chira_lang_override_wins() {
        assert_eq!(
            resolve_lang(Some("ja"), Some("en_US.UTF-8"), None, None),
            Lang::Ja
        );
        assert_eq!(
            resolve_lang(Some("en"), Some("ja_JP.UTF-8"), None, None),
            Lang::En
        );
    }

    #[test]
    fn chira_lang_aliases_and_case_insensitive() {
        // ja / ja_jp / japanese / case-insensitive すべて Ja
        assert_eq!(resolve_lang(Some("JA"), None, None, None), Lang::Ja);
        assert_eq!(resolve_lang(Some("ja_JP"), None, None, None), Lang::Ja);
        assert_eq!(resolve_lang(Some("Japanese"), None, None, None), Lang::Ja);
        // en / en_us / english すべて En
        assert_eq!(resolve_lang(Some("EN"), None, None, None), Lang::En);
        assert_eq!(resolve_lang(Some("en_US"), None, None, None), Lang::En);
        assert_eq!(resolve_lang(Some("English"), None, None, None), Lang::En);
    }

    #[test]
    fn chira_lang_unknown_falls_back_to_locale() {
        assert_eq!(
            resolve_lang(Some("xx"), Some("ja_JP.UTF-8"), None, None),
            Lang::Ja
        );
        assert_eq!(
            resolve_lang(Some(""), Some("ja_JP.UTF-8"), None, None),
            Lang::Ja
        );
    }

    #[test]
    fn locale_precedence_lc_all_first() {
        assert_eq!(
            resolve_lang(None, Some("ja_JP.UTF-8"), Some("en_US"), Some("en_US")),
            Lang::Ja
        );
        assert_eq!(
            resolve_lang(None, Some("en_US.UTF-8"), Some("ja_JP"), Some("ja_JP")),
            Lang::En
        );
    }

    #[test]
    fn empty_locale_skipped() {
        assert_eq!(
            resolve_lang(None, Some(""), Some("ja_JP.UTF-8"), None),
            Lang::Ja
        );
    }

    #[test]
    fn default_when_nothing_set() {
        assert_eq!(resolve_lang(None, None, None, None), Lang::En);
    }

    #[test]
    fn c_or_posix_treated_as_en() {
        assert_eq!(resolve_lang(None, Some("C"), None, None), Lang::En);
        assert_eq!(resolve_lang(None, Some("POSIX"), None, None), Lang::En);
    }
}
