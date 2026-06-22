use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use toml_edit::{Array, DocumentMut, value};

use crate::i18n::{self, Lang};
use crate::scratch::env_path;

/// config.toml から読んだ値。未指定 (キー不在・空文字・型不一致) は None で、
/// 呼び出し側が env → ハードコード default へフォールバックする。
#[derive(Default, Debug, Clone, PartialEq, Eq)]
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
#[derive(Default, Debug, Clone, PartialEq, Eq)]
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

/// 設定値の出どころ。TUI config 画面で source を表示するために使う。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    /// env で上書き中。値は env 変数名 (`CHIRA_DIR` 等)
    Env(&'static str),
    Config,
    /// ハードコード default に倒れている (config 未設定 + env 未設定)
    Default,
}

/// 各項目の effective 値 (= 起動時に有効になる値) と source。
/// `archive.*` 系は env で上書きされる経路がないため、config の値があれば Config、
/// なければ Default として扱う (明示的 false / 明示的 [] と未指定は Config 構造体側で
/// 区別していないため、TUI 上は「config 経由か default か」の粒度で表示する)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Effective {
    pub dir: (String, Source),
    pub editor: (String, Source),
    pub shell: (String, Source),
    pub archive_ttl_days: (Option<u64>, Source),
    pub archive_dir: (String, Source),
    pub archive_on_startup: (bool, Source),
    pub archive_keep: (Vec<String>, Source),
}

/// テスト容易性のため env を引数で受け取る純関数。production からは [`effective`] が呼ぶ。
pub fn resolve_effective(
    config: &Config,
    chira_dir: Option<String>,
    editor_env: Option<String>,
    shell_env: Option<String>,
) -> Effective {
    Effective {
        dir: pick_string(chira_dir, "CHIRA_DIR", config.dir.clone(), ""),
        editor: pick_string(editor_env, "EDITOR", config.editor.clone(), "vi"),
        shell: pick_string(shell_env, "SHELL", config.shell.clone(), "/bin/sh"),
        archive_ttl_days: match config.archive.ttl_days {
            Some(v) => (Some(v), Source::Config),
            None => (None, Source::Default),
        },
        archive_dir: match config.archive.dir.clone() {
            Some(s) => (s, Source::Config),
            None => (String::new(), Source::Default),
        },
        // on_startup / keep の default は false / [] で、明示値との区別は構造体上できないため、
        // 値そのものが non-default なら Config、default なら Default として表示する。
        archive_on_startup: if config.archive.on_startup {
            (true, Source::Config)
        } else {
            (false, Source::Default)
        },
        archive_keep: if config.archive.keep.is_empty() {
            (Vec::new(), Source::Default)
        } else {
            (config.archive.keep.clone(), Source::Config)
        },
    }
}

/// 現在の env から effective 値を解決する (production 入り口)。
pub fn effective(config: &Config) -> Effective {
    resolve_effective(
        config,
        env::var("CHIRA_DIR").ok().filter(|s| !s.is_empty()),
        env::var("EDITOR").ok().filter(|s| !s.is_empty()),
        env::var("SHELL").ok().filter(|s| !s.is_empty()),
    )
}

/// env が非空なら env、次に config、最後にハードコード default を採用する。
fn pick_string(
    env_val: Option<String>,
    env_name: &'static str,
    config_val: Option<String>,
    default: &str,
) -> (String, Source) {
    if let Some(v) = env_val {
        return (v, Source::Env(env_name));
    }
    if let Some(v) = config_val {
        return (v, Source::Config);
    }
    (default.to_string(), Source::Default)
}

/// TUI から渡される編集差分。`None` の項目は触らず、`Some(v)` は v で上書きする。
/// 空文字列は「キーを残したまま空文字 set」ではなく「キーを削除」として扱う
/// (load 側が空文字を未設定扱いするため、書き戻し側もそれに揃える)。
#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub struct ConfigEdit {
    pub dir: Option<String>,
    pub editor: Option<String>,
    pub shell: Option<String>,
    pub archive_ttl_days: Option<u64>,
    pub archive_dir: Option<String>,
    pub archive_on_startup: Option<bool>,
    pub archive_keep: Option<Vec<String>>,
}

/// 書き戻し先の config.toml パスを解決する (load と同じ優先順位)。
/// 解決できる候補が一つも無い場合は None (HOME も XDG も無い極端な環境)。
pub fn save_path() -> Option<PathBuf> {
    resolve_path(
        env_path("CHIRA_CONFIG"),
        env_path("XDG_CONFIG_HOME"),
        env_path("HOME"),
    )
}

/// 既存 config.toml に編集差分をマージして書き戻す (フォーマット・コメント保持)。
/// 親ディレクトリは必要に応じて mkdir -p、書き込みは tmp → rename の atomic 経路。
/// path が symlink の場合は実体側に書き戻し、dotfiles 管理経路 (`~/.config/chira/config.toml`
/// → `~/dotfiles/...`) を壊さない。既存ファイルの permission (例: `0600`) は umask に倒れず
/// rename 後も保持する (機微な command / path が読みやすい mode に格下げされるのを防ぐ)。
pub fn save(path: &Path, edit: &ConfigEdit) -> io::Result<()> {
    let text = match fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) if e.kind() == io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(e),
    };
    let new_text = apply_edit(&text, edit)?;
    // symlink (dotfiles 経路) は target 側へ書き戻す。symlink 解決失敗 (broken link 等) は
    // 通常 path で続行 (read 段階で NotFound にならない限りこのケースは入らない)。
    let target = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    if let Some(parent) = target.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent)?;
    }
    // 既存 mode を保持: 不在 (新規作成時) や 読めない (権限なし) は None で続行 = umask 任せ
    let preserved_mode = preserve_mode(&target);
    let tmp = tmp_path(&target);
    fs::write(&tmp, new_text)?;
    if let Some(mode) = preserved_mode {
        apply_mode(&tmp, mode)?;
    }
    fs::rename(&tmp, &target)?;
    Ok(())
}

#[cfg(unix)]
fn preserve_mode(target: &Path) -> Option<u32> {
    use std::os::unix::fs::PermissionsExt;
    fs::metadata(target).ok().map(|m| m.permissions().mode())
}

#[cfg(not(unix))]
fn preserve_mode(_target: &Path) -> Option<u32> {
    None
}

#[cfg(unix)]
fn apply_mode(path: &Path, mode: u32) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(mode))
}

#[cfg(not(unix))]
fn apply_mode(_path: &Path, _mode: u32) -> io::Result<()> {
    Ok(())
}

/// path に sibling な tmp ファイル名を生成する (atomic rename の中継先)。
fn tmp_path(path: &Path) -> PathBuf {
    let mut name = path
        .file_name()
        .map(|s| s.to_os_string())
        .unwrap_or_default();
    name.push(".tmp");
    path.with_file_name(name)
}

/// 既存 TOML テキストに編集差分を適用して文字列で返す (純関数。テストから直接叩く)。
/// エラー: TOML parse 失敗 / TTL が i64::MAX 超 (= TOML integer は i64 のため round-trip 不能)。
fn apply_edit(text: &str, edit: &ConfigEdit) -> io::Result<String> {
    let mut doc: DocumentMut = if text.is_empty() {
        DocumentMut::new()
    } else {
        text.parse().map_err(io::Error::other)?
    };
    set_or_remove_str(doc.as_table_mut(), "dir", edit.dir.as_deref());
    set_or_remove_str(doc.as_table_mut(), "editor", edit.editor.as_deref());
    set_or_remove_str(doc.as_table_mut(), "shell", edit.shell.as_deref());
    if needs_archive_table(edit) {
        // inline form (`archive = { ttl_days = 7 }`) を standard table に coerce して
        // 既存設定からの編集を silent skip しないよう、まず inline / 非 table を弾く
        coerce_to_table(doc.as_table_mut(), "archive");
        let archive = doc
            .entry("archive")
            .or_insert(toml_edit::Item::Table(toml_edit::Table::new()));
        if let Some(t) = archive.as_table_mut() {
            if let Some(n) = edit.archive_ttl_days {
                let signed = i64::try_from(n).map_err(|_| {
                    io::Error::other(format!(
                        "archive.ttl_days {n} exceeds TOML integer range (max {})",
                        i64::MAX
                    ))
                })?;
                t.insert("ttl_days", value(signed));
            }
            if let Some(s) = edit.archive_dir.as_deref() {
                set_or_remove_str(t, "dir", Some(s));
            }
            if let Some(b) = edit.archive_on_startup {
                t.insert("on_startup", value(b));
            }
            if let Some(keep) = edit.archive_keep.as_ref() {
                let mut arr = Array::new();
                for s in keep {
                    arr.push(s.as_str());
                }
                t.insert("keep", value(arr));
            }
        }
    }
    Ok(doc.to_string())
}

/// `[key]` が inline table / 非 table の場合に standard table へ書き換える。
/// 既に standard table、または存在しない場合は no-op (or_insert 側で table 生成)。
/// inline → standard 変換時に元の key/value を保持する。
fn coerce_to_table(doc: &mut toml_edit::Table, key: &str) {
    let Some(item) = doc.get_mut(key) else {
        return;
    };
    if item.is_table() {
        return;
    }
    let mut t = toml_edit::Table::new();
    if let Some(inline) = item.as_inline_table() {
        for (k, v) in inline.iter() {
            t.insert(k, toml_edit::Item::Value(v.clone()));
        }
    }
    *item = toml_edit::Item::Table(t);
}

fn needs_archive_table(edit: &ConfigEdit) -> bool {
    edit.archive_ttl_days.is_some()
        || edit.archive_dir.is_some()
        || edit.archive_on_startup.is_some()
        || edit.archive_keep.is_some()
}

/// 空文字は「キー削除」、非空は「set」として扱う (load 側の空文字=未設定契約に揃える)。
fn set_or_remove_str(table: &mut toml_edit::Table, key: &str, val: Option<&str>) {
    match val {
        Some("") => {
            table.remove(key);
        }
        Some(s) => {
            table.insert(key, value(s));
        }
        None => {}
    }
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

    fn cfg(dir: Option<&str>, editor: Option<&str>) -> Config {
        Config {
            dir: dir.map(str::to_string),
            editor: editor.map(str::to_string),
            ..Default::default()
        }
    }

    #[test]
    fn effective_env_beats_config_beats_default() {
        let c = cfg(Some("/cfg"), Some("nvim"));
        let eff = resolve_effective(&c, Some("/env".into()), None, Some("/bin/zsh".into()));
        assert_eq!(eff.dir, ("/env".to_string(), Source::Env("CHIRA_DIR")));
        assert_eq!(eff.editor, ("nvim".to_string(), Source::Config));
        assert_eq!(eff.shell, ("/bin/zsh".to_string(), Source::Env("SHELL")));
    }

    #[test]
    fn effective_default_when_nothing_set() {
        let eff = resolve_effective(&Config::default(), None, None, None);
        assert_eq!(eff.dir, (String::new(), Source::Default));
        assert_eq!(eff.editor, ("vi".to_string(), Source::Default));
        assert_eq!(eff.shell, ("/bin/sh".to_string(), Source::Default));
    }

    #[test]
    fn effective_archive_source_reflects_non_default_value() {
        // archive 系は env 上書き経路を持たないため、source は値が default かどうかで決まる
        let mut c = Config::default();
        c.archive.ttl_days = Some(30);
        c.archive.dir = Some("~/old".into());
        c.archive.on_startup = true;
        c.archive.keep = vec!["pinned-*".into()];
        let eff = resolve_effective(&c, None, None, None);
        assert_eq!(eff.archive_ttl_days, (Some(30), Source::Config));
        assert_eq!(eff.archive_dir, ("~/old".to_string(), Source::Config));
        assert_eq!(eff.archive_on_startup, (true, Source::Config));
        assert_eq!(
            eff.archive_keep,
            (vec!["pinned-*".to_string()], Source::Config)
        );

        let eff_def = resolve_effective(&Config::default(), None, None, None);
        assert_eq!(eff_def.archive_ttl_days, (None, Source::Default));
        assert_eq!(eff_def.archive_dir, (String::new(), Source::Default));
        assert_eq!(eff_def.archive_on_startup, (false, Source::Default));
        assert_eq!(eff_def.archive_keep, (Vec::new(), Source::Default));
    }

    #[test]
    fn apply_edit_creates_from_empty() {
        let edit = ConfigEdit {
            dir: Some("~/scratch".into()),
            editor: Some("nvim".into()),
            archive_ttl_days: Some(30),
            archive_on_startup: Some(true),
            archive_keep: Some(vec!["pinned-*".into(), "longterm/".into()]),
            ..Default::default()
        };
        let out = apply_edit("", &edit).unwrap();
        let parsed = parse(&out).unwrap();
        assert_eq!(parsed.dir.as_deref(), Some("~/scratch"));
        assert_eq!(parsed.editor.as_deref(), Some("nvim"));
        assert_eq!(parsed.archive.ttl_days, Some(30));
        assert!(parsed.archive.on_startup);
        assert_eq!(parsed.archive.keep, vec!["pinned-*", "longterm/"]);
    }

    #[test]
    fn apply_edit_preserves_comments_and_unrelated_keys() {
        // フォーマット保持: コメント・無関係キー・余白を残したまま 1 キーだけ更新
        let original = "\
# top-level comment
dir = \"/old\"   # inline comment
editor = \"vi\"

# archive section
[archive]
ttl_days = 7   # weekly
unrelated_key = \"keep me\"
";
        let edit = ConfigEdit {
            editor: Some("nvim".into()),
            ..Default::default()
        };
        let out = apply_edit(original, &edit).unwrap();
        assert!(out.contains("# top-level comment"));
        assert!(out.contains("# inline comment"));
        assert!(out.contains("# archive section"));
        assert!(out.contains("unrelated_key = \"keep me\""));
        assert!(out.contains("editor = \"nvim\""));
        assert!(out.contains("dir = \"/old\""));
    }

    #[test]
    fn apply_edit_empty_string_removes_key() {
        // 空文字 set はキー削除 (load 側の空文字=未設定契約に揃える)
        let original = "dir = \"/old\"\neditor = \"vi\"\n";
        let edit = ConfigEdit {
            dir: Some(String::new()),
            ..Default::default()
        };
        let out = apply_edit(original, &edit).unwrap();
        assert!(!out.contains("dir ="));
        assert!(out.contains("editor = \"vi\""));
    }

    #[test]
    fn apply_edit_creates_archive_table_when_needed() {
        let edit = ConfigEdit {
            archive_ttl_days: Some(14),
            ..Default::default()
        };
        let out = apply_edit("dir = \"/scratch\"\n", &edit).unwrap();
        let parsed = parse(&out).unwrap();
        assert_eq!(parsed.archive.ttl_days, Some(14));
        assert_eq!(parsed.dir.as_deref(), Some("/scratch"));
    }

    #[test]
    fn apply_edit_keep_overwrites_array() {
        // keep は丸ごと置換 (TUI 側で add/remove した結果を渡す前提)
        let original = "[archive]\nkeep = [\"old-*\"]\n";
        let edit = ConfigEdit {
            archive_keep: Some(vec!["new-*".into(), "longterm/".into()]),
            ..Default::default()
        };
        let out = apply_edit(original, &edit).unwrap();
        let parsed = parse(&out).unwrap();
        assert_eq!(parsed.archive.keep, vec!["new-*", "longterm/"]);
        assert!(!parsed.archive.keep.iter().any(|s| s == "old-*"));
    }

    #[test]
    fn apply_edit_none_fields_are_noop() {
        // すべて None なら原文そのまま (改行/空白も維持。差分 0 行)
        let original = "dir = \"/x\"\n# c\n[archive]\nttl_days = 1\n";
        let out = apply_edit(original, &ConfigEdit::default()).unwrap();
        assert_eq!(out, original);
    }

    #[test]
    fn apply_edit_rejects_ttl_over_i64_max() {
        // TOML integer は i64。u64::MAX 等 i64 範囲外の値は round-trip 不能なので
        // wrap で負値書き込み → 次回 load で silent unset、を防ぐため Err にする
        let edit = ConfigEdit {
            archive_ttl_days: Some(i64::MAX as u64 + 1),
            ..Default::default()
        };
        let err = apply_edit("", &edit).unwrap_err();
        assert!(err.to_string().contains("ttl_days"));

        // i64::MAX 以下は OK
        let edit_ok = ConfigEdit {
            archive_ttl_days: Some(i64::MAX as u64),
            ..Default::default()
        };
        assert!(apply_edit("", &edit_ok).is_ok());
    }

    #[test]
    fn apply_edit_coerces_inline_archive_table() {
        // 既存 config が inline form (`archive = { ttl_days = 7 }`) でも、
        // standard table へ昇格してから書き込むので edit が silent skip されない
        let original = "archive = { ttl_days = 7, dir = \"~/old\" }\n";
        let edit = ConfigEdit {
            archive_ttl_days: Some(30),
            ..Default::default()
        };
        let out = apply_edit(original, &edit).unwrap();
        let parsed = parse(&out).unwrap();
        assert_eq!(parsed.archive.ttl_days, Some(30));
        // 元の値は coerce 時に保持
        assert_eq!(parsed.archive.dir.as_deref(), Some("~/old"));
    }

    #[cfg(unix)]
    #[test]
    fn save_preserves_existing_file_mode() {
        // 既存 config.toml が 0600 (機微 path / command を含むため restrict 想定) のとき、
        // save 後も 0600 を維持する (tmp→rename が umask に倒れ 0644 に格下げするのを防ぐ)
        use std::os::unix::fs::PermissionsExt;
        let dir = temp_dir();
        let path = dir.join("config.toml");
        fs::write(&path, "editor = \"vi\"\n").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        let edit = ConfigEdit {
            editor: Some("nvim".into()),
            ..Default::default()
        };
        save(&path, &edit).unwrap();
        let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "file mode after save should remain 0600");
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.contains("editor = \"nvim\""));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn save_round_trip_to_disk() {
        // save → load の往復で値が一致 (atomic write 経路 + parent mkdir の動作確認)
        let dir = temp_dir();
        let path = dir.join("nested/config.toml");
        let edit = ConfigEdit {
            dir: Some("/scratch".into()),
            archive_ttl_days: Some(30),
            archive_keep: Some(vec!["pinned-*".into()]),
            ..Default::default()
        };
        save(&path, &edit).unwrap();
        let loaded = parse(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(loaded.dir.as_deref(), Some("/scratch"));
        assert_eq!(loaded.archive.ttl_days, Some(30));
        assert_eq!(loaded.archive.keep, vec!["pinned-*"]);
        fs::remove_dir_all(&dir).unwrap();
    }
}
