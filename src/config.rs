use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::i18n::{self, Lang};

/// config.toml から読んだ値。未指定 (キー不在・空文字・型不一致) は None で、
/// 呼び出し側が env → ハードコード default へフォールバックする。
#[derive(Default, Debug, PartialEq, Eq)]
pub struct Config {
    pub dir: Option<String>,
    pub editor: Option<String>,
    pub shell: Option<String>,
}

/// 設定ファイルを読み込む。不在・空は未設定扱い (warning なし)。
/// 読み取り/パース失敗は stderr に warning を出し、デフォルト設定で起動を継続する。
pub fn load(lang: Lang) -> Config {
    let chira_config = env::var("CHIRA_CONFIG").ok();
    let xdg_config = env::var("XDG_CONFIG_HOME").ok();
    let home = env::var("HOME").ok();
    let Some(path) = resolve_path(
        chira_config.as_deref(),
        xdg_config.as_deref(),
        home.as_deref(),
    ) else {
        return Config::default();
    };

    match fs::read_to_string(&path) {
        Ok(text) => match parse(&text) {
            Ok(config) => config,
            Err(e) => {
                eprintln!("{}", i18n::warn_config_parse(lang, &path.display(), &e));
                Config::default()
            }
        },
        Err(e) if e.kind() == io::ErrorKind::NotFound => Config::default(),
        Err(e) => {
            eprintln!(
                "{}",
                i18n::warn_config_unreadable(lang, &path.display(), &e)
            );
            Config::default()
        }
    }
}

/// 設定ファイルパスの解決順: $CHIRA_CONFIG → $XDG_CONFIG_HOME/chira → ~/.config/chira。
fn resolve_path(
    chira_config: Option<&str>,
    xdg_config: Option<&str>,
    home: Option<&str>,
) -> Option<PathBuf> {
    if let Some(p) = chira_config.filter(|s| !s.is_empty()) {
        return Some(PathBuf::from(p));
    }
    if let Some(d) = xdg_config.filter(|s| !s.is_empty()) {
        return Some(Path::new(d).join("chira/config.toml"));
    }
    let home = home.filter(|s| !s.is_empty())?;
    Some(Path::new(home).join(".config/chira/config.toml"))
}

fn parse(text: &str) -> Result<Config, toml::de::Error> {
    let table: toml::Table = toml::from_str(text)?;
    Ok(Config {
        dir: get_str(&table, "dir"),
        editor: get_str(&table, "editor"),
        shell: get_str(&table, "shell"),
    })
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
    use super::*;

    #[test]
    fn resolve_path_precedence() {
        // CHIRA_CONFIG は絶対パスをそのまま使う
        assert_eq!(
            resolve_path(Some("/etc/chira.toml"), Some("/xdg"), Some("/home/u")),
            Some(PathBuf::from("/etc/chira.toml"))
        );
        // CHIRA_CONFIG 不在なら XDG_CONFIG_HOME/chira
        assert_eq!(
            resolve_path(None, Some("/xdg"), Some("/home/u")),
            Some(PathBuf::from("/xdg/chira/config.toml"))
        );
        // どちらも無ければ ~/.config/chira
        assert_eq!(
            resolve_path(None, None, Some("/home/u")),
            Some(PathBuf::from("/home/u/.config/chira/config.toml"))
        );
        // 空文字は未設定として次の候補へ送る
        assert_eq!(
            resolve_path(Some(""), Some(""), Some("/home/u")),
            Some(PathBuf::from("/home/u/.config/chira/config.toml"))
        );
        // 解決先が一つも無ければ None
        assert_eq!(resolve_path(None, None, None), None);
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
            }
        );
    }

    #[test]
    fn parse_broken_toml_is_err() {
        // 壊れた TOML は Err (load 側で warning + default にフォールバックする)
        assert!(parse("dir = ").is_err());
        assert!(parse("[unterminated").is_err());
    }
}
