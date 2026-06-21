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
/// 1. `CHIRA_LANG` (明示的な override。値は `en` / `ja` を許容、未知値は無視)
/// 2. POSIX locale: `LC_ALL` → `LC_MESSAGES` → `LANG`、`ja*` で Ja、他は En
/// 3. いずれも未設定なら En (global 公開向けの default)
fn detect_lang() -> Lang {
    if let Ok(v) = env::var("CHIRA_LANG") {
        match v.to_lowercase().as_str() {
            "ja" | "ja_jp" | "japanese" => return Lang::Ja,
            "en" | "en_us" | "english" => return Lang::En,
            _ => {}
        }
    }
    for key in ["LC_ALL", "LC_MESSAGES", "LANG"] {
        if let Ok(v) = env::var(key)
            && !v.is_empty()
        {
            if v.to_lowercase().starts_with("ja") {
                return Lang::Ja;
            }
            return Lang::En;
        }
    }
    Lang::En
}

pub fn usage(lang: Lang) -> &'static str {
    match lang {
        Lang::Ja => {
            "\
chira — 一時的な scratch ディレクトリを管理する TUI

usage: chira [--cd-file <path>]

  --cd-file <path>   終了時に最終ディレクトリを <path> へ書き出す
                     (シェル関数で cd するための連携用。README 参照)
  -h, --help         このヘルプを表示
"
        }
        Lang::En => {
            "\
chira — Manage throwaway scratch directories from a TUI.

usage: chira [--cd-file <path>]

  --cd-file <path>   On exit, write the final directory to <path>
                     (for shell-function cd integration; see README)
  -h, --help         Show this help
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

pub fn err_unknown_arg(lang: Lang, arg: &str) -> String {
    match lang {
        Lang::Ja => format!("不明な引数: {arg}\n{}", usage(lang)),
        Lang::En => format!("Unknown argument: {arg}\n{}", usage(lang)),
    }
}

pub fn err_external_launch(lang: Lang, e: &dyn std::fmt::Display) -> String {
    match lang {
        Lang::Ja => format!("外部プロセスの起動に失敗: {e}"),
        Lang::En => format!("Failed to launch external process: {e}"),
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
            ("r", "名前を変更"),
            ("d", "削除 (確認あり)"),
            ("/", "名前で絞り込み検索"),
            ("?", "このヘルプ"),
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
            ("r", "Rename"),
            ("d", "Delete (with confirmation)"),
            ("/", "Filter by name"),
            ("?", "This help"),
            ("q", "Quit"),
        ],
    }
}

pub fn footer_browse(lang: Lang) -> &'static str {
    match lang {
        Lang::Ja => "j/k:移動  l:開く  h:親  s:シェル  n:新規  /:検索  ?:ヘルプ  q:終了",
        Lang::En => "j/k:move  l:open  h:parent  s:shell  n:new  /:filter  ?:help  q:quit",
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

#[cfg(test)]
mod tests {
    use super::*;

    fn lang_with_env(
        chira_lang: Option<&str>,
        lc_all: Option<&str>,
        lc_messages: Option<&str>,
        lang_env: Option<&str>,
    ) -> Lang {
        // detect_lang() の純粋ロジックを再現 (env 直書き換えは並列 test で race するため避ける)
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

    #[test]
    fn chira_lang_override_wins() {
        assert_eq!(
            lang_with_env(Some("ja"), Some("en_US.UTF-8"), None, None),
            Lang::Ja
        );
        assert_eq!(
            lang_with_env(Some("en"), Some("ja_JP.UTF-8"), None, None),
            Lang::En
        );
    }

    #[test]
    fn chira_lang_unknown_falls_back_to_locale() {
        assert_eq!(
            lang_with_env(Some("xx"), Some("ja_JP.UTF-8"), None, None),
            Lang::Ja
        );
        assert_eq!(
            lang_with_env(Some(""), Some("ja_JP.UTF-8"), None, None),
            Lang::Ja
        );
    }

    #[test]
    fn locale_precedence_lc_all_first() {
        assert_eq!(
            lang_with_env(None, Some("ja_JP.UTF-8"), Some("en_US"), Some("en_US")),
            Lang::Ja
        );
        assert_eq!(
            lang_with_env(None, Some("en_US.UTF-8"), Some("ja_JP"), Some("ja_JP")),
            Lang::En
        );
    }

    #[test]
    fn empty_locale_skipped() {
        assert_eq!(
            lang_with_env(None, Some(""), Some("ja_JP.UTF-8"), None),
            Lang::Ja
        );
    }

    #[test]
    fn default_when_nothing_set() {
        assert_eq!(lang_with_env(None, None, None, None), Lang::En);
    }

    #[test]
    fn c_or_posix_treated_as_en() {
        assert_eq!(lang_with_env(None, Some("C"), None, None), Lang::En);
        assert_eq!(lang_with_env(None, Some("POSIX"), None, None), Lang::En);
    }
}
