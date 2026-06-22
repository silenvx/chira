use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::i18n::{self, Lang};
use crate::scratch::env_path;

/// config.toml から読んだ値。未指定 (キー不在・空文字・型不一致) は None で、
/// 呼び出し側が env → ハードコード default へフォールバックする。
#[derive(Default, Debug, PartialEq, Eq)]
pub struct Config {
    pub dir: Option<String>,
    pub editor: Option<String>,
    pub shell: Option<String>,
    pub archive: ArchiveConfig,
    pub actions: Vec<Action>,
    /// `N` (空ディレクトリ作成) を押したときに `t` 経由と同じ confirm + run フローに流すアクション名。
    /// 未設定なら従来通り空ディレクトリ作成のみ (既存挙動を変えない opt-in)。
    /// アクションが存在しない名前なら main 側 (app) で無視され従来の N 挙動になる。
    pub default_action: Option<String>,
}

/// `[actions.<name>]` の 1 エントリ。`t` で選んで新ディレクトリ内で `run` を foreground 実行する。
/// コピーもクローンも生成も `run` の中の shell コマンド (rsync / git clone / script) で表現する。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Action {
    pub name: String,
    pub description: Option<String>,
    pub run: String,
}

/// `[archive]` セクション。`ttl_days = 0` または未指定で archive 機能 off。
#[derive(Default, Debug, PartialEq, Eq)]
pub struct ArchiveConfig {
    pub ttl_days: Option<u64>,
    pub dir: Option<String>,
    pub on_startup: bool,
    pub keep: Vec<String>,
}

/// 設定ファイルを読み込む。不在・空は未設定扱い (warning なし)。
/// 読み取り/パース失敗は stderr に warning を出し、デフォルト設定で起動を継続する。
pub fn load(lang: Lang) -> Config {
    let Some(path) = resolve_path(
        env_path("CHIRA_CONFIG"),
        env_path("XDG_CONFIG_HOME"),
        env_path("HOME"),
    ) else {
        return Config::default();
    };
    load_from_path(&path, lang)
}

/// 解決済みパスから読み込み・パースし、出る warning を stderr へ流す。
fn load_from_path(path: &Path, lang: Lang) -> Config {
    let (config, warning) = read_and_parse(path, lang);
    if let Some(warning) = warning {
        eprintln!("{warning}");
    }
    config
}

/// 読み込み・パースの結果と warning を返す (stderr 副作用を分離して warning 契約をテスト可能にする)。
/// 不在は silent (None)、パース失敗・読み取り失敗は warning 文言を Some で返す。
fn read_and_parse(path: &Path, lang: Lang) -> (Config, Option<String>) {
    match fs::read_to_string(path) {
        Ok(text) => match parse(&text) {
            Ok(config) => (config, None),
            Err(e) => (
                Config::default(),
                Some(i18n::warn_config_parse(lang, &path.display(), &e)),
            ),
        },
        Err(e) if e.kind() == io::ErrorKind::NotFound => (Config::default(), None),
        Err(e) => (
            Config::default(),
            Some(i18n::warn_config_unreadable(lang, &path.display(), &e)),
        ),
    }
}

/// 設定ファイルパスの解決順: $CHIRA_CONFIG → $XDG_CONFIG_HOME/chira → ~/.config/chira。
fn resolve_path(
    chira_config: Option<PathBuf>,
    xdg_config: Option<PathBuf>,
    home: Option<PathBuf>,
) -> Option<PathBuf> {
    if let Some(p) = chira_config {
        return Some(p);
    }
    if let Some(d) = xdg_config {
        return Some(d.join("chira/config.toml"));
    }
    Some(home?.join(".config/chira/config.toml"))
}

fn parse(text: &str) -> Result<Config, toml::de::Error> {
    let table: toml::Table = toml::from_str(text)?;
    Ok(Config {
        dir: get_str(&table, "dir"),
        editor: get_str(&table, "editor"),
        shell: get_str(&table, "shell"),
        archive: parse_archive(table.get("archive").and_then(|v| v.as_table())),
        actions: parse_actions(table.get("actions").and_then(|v| v.as_table())),
        default_action: get_str(&table, "default_action"),
    })
}

/// `[actions.*]` をパースする。`run` が非空文字列のエントリのみ採用し、名前順にソートする。
/// `run` 欠落・空・型不一致のエントリは無効として黙って除外する (keep[] と同じ要素単位フィルタ方針)。
fn parse_actions(table: Option<&toml::Table>) -> Vec<Action> {
    let Some(t) = table else {
        return Vec::new();
    };
    let mut actions: Vec<Action> = t
        .iter()
        .filter_map(|(name, value)| {
            let tbl = value.as_table()?;
            // trim 後で空判定する: `run = "   "` のような空白のみエントリは no-op で実害がないため除外。
            // 採用時は trim 済み文字列にして `sh -c` への前後余分空白を落とす。
            let run = tbl
                .get("run")
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|s| !s.is_empty())?
                .to_string();
            Some(Action {
                name: name.clone(),
                description: get_str(tbl, "description"),
                run,
            })
        })
        .collect();
    actions.sort_by(|a, b| a.name.cmp(&b.name));
    actions
}

fn parse_archive(table: Option<&toml::Table>) -> ArchiveConfig {
    let Some(t) = table else {
        return ArchiveConfig::default();
    };
    ArchiveConfig {
        ttl_days: t
            .get("ttl_days")
            .and_then(|v| v.as_integer())
            .and_then(|n| u64::try_from(n).ok()),
        dir: get_str(t, "dir"),
        on_startup: t
            .get("on_startup")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        keep: t
            .get("keep")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str())
                    .filter(|s| !s.is_empty())
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default(),
    }
}

/// 文字列値のみ採用する。型不一致・空文字は未設定扱い。
fn get_str(table: &toml::Table, key: &str) -> Option<String> {
    table
        .get(key)
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU32, Ordering};

    use super::*;

    fn temp_dir() -> PathBuf {
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("chira-config-{}-{}", std::process::id(), n));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn resolve_path_precedence() {
        // CHIRA_CONFIG は絶対パスをそのまま使う
        assert_eq!(
            resolve_path(
                Some(PathBuf::from("/etc/chira.toml")),
                Some(PathBuf::from("/xdg")),
                Some(PathBuf::from("/home/u")),
            ),
            Some(PathBuf::from("/etc/chira.toml"))
        );
        // CHIRA_CONFIG 不在なら XDG_CONFIG_HOME/chira
        assert_eq!(
            resolve_path(
                None,
                Some(PathBuf::from("/xdg")),
                Some(PathBuf::from("/home/u"))
            ),
            Some(PathBuf::from("/xdg/chira/config.toml"))
        );
        // どちらも無ければ ~/.config/chira
        assert_eq!(
            resolve_path(None, None, Some(PathBuf::from("/home/u"))),
            Some(PathBuf::from("/home/u/.config/chira/config.toml"))
        );
        // 解決先が一つも無ければ None
        assert_eq!(resolve_path(None, None, None), None);
    }

    #[test]
    fn read_and_parse_missing_is_silent_default() {
        let dir = temp_dir();
        // 不在ファイルは warning なし (None) で default
        let (config, warning) = read_and_parse(&dir.join("absent.toml"), Lang::En);
        assert_eq!(config, Config::default());
        assert_eq!(warning, None);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn read_and_parse_valid_has_no_warning() {
        let dir = temp_dir();
        let path = dir.join("config.toml");
        fs::write(&path, "dir = \"/scratch\"\neditor = \"nvim\"\n").unwrap();
        let (config, warning) = read_and_parse(&path, Lang::En);
        assert_eq!(config.dir.as_deref(), Some("/scratch"));
        assert_eq!(config.editor.as_deref(), Some("nvim"));
        assert_eq!(config.shell, None);
        assert_eq!(warning, None);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn read_and_parse_broken_toml_warns_and_defaults() {
        let dir = temp_dir();
        let path = dir.join("config.toml");
        // 壊れた TOML は warning を出して default で起動継続 (README 契約)
        fs::write(&path, "dir = ").unwrap();
        let (config, warning) = read_and_parse(&path, Lang::En);
        assert_eq!(config, Config::default());
        assert!(warning.is_some_and(|w| w.contains("parse")));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn read_and_parse_unreadable_warns_and_defaults() {
        use std::os::unix::fs::PermissionsExt;
        let dir = temp_dir();
        let path = dir.join("config.toml");
        fs::write(&path, "dir = \"/scratch\"").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o000)).unwrap();
        let (config, warning) = read_and_parse(&path, Lang::En);
        // root は権限を無視して読めるため、読めた (warning なし) 場合のみ assert を skip する
        if let Some(warning) = warning {
            assert_eq!(config, Config::default());
            assert!(warning.contains("read"));
        }
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn parse_extracts_known_string_keys() {
        let config = parse(
            r#"
            dir = "~/scratch"
            editor = "nvim --clean"
            shell = "/bin/zsh"
            "#,
        )
        .unwrap();
        assert_eq!(
            config,
            Config {
                dir: Some("~/scratch".into()),
                editor: Some("nvim --clean".into()),
                shell: Some("/bin/zsh".into()),
                ..Default::default()
            }
        );
    }

    #[test]
    fn parse_empty_is_all_none() {
        assert_eq!(parse("").unwrap(), Config::default());
        // 未知キーや空文字値も未設定扱い
        assert_eq!(
            parse("foo = \"bar\"\ndir = \"\"").unwrap(),
            Config::default()
        );
    }

    #[test]
    fn parse_ignores_non_string_values() {
        // 型不一致 (数値・テーブル等) は採用せず None にする
        let config = parse("dir = 42\neditor = \"vi\"\n[shell]\nx = 1").unwrap();
        assert_eq!(
            config,
            Config {
                dir: None,
                editor: Some("vi".into()),
                shell: None,
                ..Default::default()
            }
        );
    }

    #[test]
    fn parse_extracts_archive_section() {
        let config = parse(
            r#"
            [archive]
            ttl_days = 30
            dir = "~/scratch-archive"
            on_startup = true
            keep = ["pinned-*", "longterm/"]
            "#,
        )
        .unwrap();
        assert_eq!(config.archive.ttl_days, Some(30));
        assert_eq!(config.archive.dir.as_deref(), Some("~/scratch-archive"));
        assert!(config.archive.on_startup);
        assert_eq!(
            config.archive.keep,
            vec!["pinned-*".to_string(), "longterm/".to_string()]
        );
    }

    #[test]
    fn parse_archive_missing_is_default_off() {
        let config = parse("dir = \"/scratch\"").unwrap();
        assert_eq!(config.archive, ArchiveConfig::default());
        assert!(!config.archive.on_startup);
        assert_eq!(config.archive.ttl_days, None);
        assert!(config.archive.keep.is_empty());
    }

    #[test]
    fn parse_archive_ignores_non_string_keep_entries() {
        // 型不一致・空文字は keep から弾く (一覧全体ではなく要素単位で除外)
        let config = parse(
            r#"
            [archive]
            keep = ["ok-*", 42, "", "longterm/"]
            "#,
        )
        .unwrap();
        assert_eq!(
            config.archive.keep,
            vec!["ok-*".to_string(), "longterm/".to_string()]
        );
    }

    #[test]
    fn parse_archive_rejects_negative_ttl() {
        // u64 へ収まらない値 (負値) は未設定扱い (誤って off 化を防ぐため None で返す)
        let config = parse("[archive]\nttl_days = -1").unwrap();
        assert_eq!(config.archive.ttl_days, None);
    }

    #[test]
    fn parse_extracts_actions_sorted_by_name() {
        let config = parse(
            r#"
            [actions.rust]
            description = "rust skeleton"
            run = "rsync -a ~/.config/chira/skel/rust/ ./ && cargo init -q"

            [actions.clone]
            run = "git clone --depth 1 git@example.com:me/sandbox.git ."
            "#,
        )
        .unwrap();
        // 名前順 (clone < rust) でソートされる
        assert_eq!(config.actions.len(), 2);
        assert_eq!(config.actions[0].name, "clone");
        assert_eq!(config.actions[0].description, None);
        assert!(config.actions[0].run.starts_with("git clone"));
        assert_eq!(config.actions[1].name, "rust");
        assert_eq!(
            config.actions[1].description.as_deref(),
            Some("rust skeleton")
        );
    }

    #[test]
    fn parse_actions_treats_whitespace_only_run_as_empty_and_trims() {
        // run = "   " は no-op で実害がないため除外。採用時は trim 済み文字列で保存する。
        let config = parse(
            r#"
            [actions.blank]
            run = "   "

            [actions.padded]
            run = "  git init -q  "
            "#,
        )
        .unwrap();
        assert_eq!(config.actions.len(), 1);
        assert_eq!(config.actions[0].name, "padded");
        assert_eq!(config.actions[0].run, "git init -q");
    }

    #[test]
    fn parse_actions_skips_entries_without_run() {
        // run 欠落・空・非文字列は無効として除外する
        let config = parse(
            r#"
            [actions.ok]
            run = "git init -q"

            [actions.no_run]
            description = "missing run"

            [actions.empty_run]
            run = ""

            [actions.bad_type]
            run = 42
            "#,
        )
        .unwrap();
        assert_eq!(config.actions.len(), 1);
        assert_eq!(config.actions[0].name, "ok");
    }

    #[test]
    fn parse_actions_missing_is_empty() {
        let config = parse("dir = \"/scratch\"").unwrap();
        assert!(config.actions.is_empty());
    }

    #[test]
    fn parse_extracts_default_action() {
        let config = parse(
            r#"
            default_action = "rust"

            [actions.rust]
            run = "cargo init -q"
            "#,
        )
        .unwrap();
        assert_eq!(config.default_action.as_deref(), Some("rust"));
        assert_eq!(config.actions.len(), 1);
    }

    #[test]
    fn parse_default_action_missing_and_empty_are_none() {
        // 未設定は None (既存挙動を変えない default-off の op-in)
        assert_eq!(parse("dir = \"/scratch\"").unwrap().default_action, None);
        // 空文字は未設定扱い
        assert_eq!(parse("default_action = \"\"").unwrap().default_action, None);
        // 型不一致 (数値・テーブル等) も None
        assert_eq!(parse("default_action = 42").unwrap().default_action, None);
    }

    #[test]
    fn parse_broken_toml_is_err() {
        // 壊れた TOML は Err (load 側で warning + default にフォールバックする)
        assert!(parse("dir = ").is_err());
        assert!(parse("[unterminated").is_err());
    }
}
