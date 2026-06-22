use std::io::{self, BufRead, IsTerminal, Write};
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Local};

use crate::config::Config;
use crate::external;
use crate::i18n::{self, Lang};
use crate::scratch::{self, Entry};

const SUBCOMMANDS: &[&str] = &[
    "ls", "tree", "new", "mkdir", "edit", "shell", "rm", "mv", "path", "find",
];

pub fn is_subcommand(s: &str) -> bool {
    SUBCOMMANDS.contains(&s)
}

/// サブコマンドを実行し process exit code を返す。
pub fn run(lang: Lang, config: &Config, sub: &str, args: Vec<String>) -> i32 {
    // 各サブコマンドの -h/--help は usage を出して 0 で終わる契約
    if args.iter().any(|a| a == "-h" || a == "--help") {
        print!("{}", i18n::usage(lang));
        return 0;
    }
    match sub {
        "ls" => cmd_ls(lang, config, args),
        "tree" => cmd_tree(lang, config, args),
        "new" => cmd_new(lang, config, args),
        "mkdir" => cmd_mkdir(lang, config, args),
        "edit" => cmd_edit(lang, config, args),
        "shell" => cmd_shell(lang, config, args),
        "rm" => cmd_rm(lang, config, args),
        "mv" => cmd_mv(lang, config, args),
        "path" => cmd_path(lang, config, args),
        "find" => cmd_find(lang, config, args),
        _ => {
            eprint!("{}", i18n::err_unknown_subcommand(lang, sub));
            2
        }
    }
}

fn cmd_ls(lang: Lang, config: &Config, args: Vec<String>) -> i32 {
    let mut long = false;
    let mut path: Option<String> = None;
    for a in args {
        match a.as_str() {
            "-l" | "--long" => long = true,
            other if other.starts_with('-') => {
                eprintln!("{}", i18n::err_cli_unknown_flag(lang, "ls", other));
                return 2;
            }
            other => {
                if path.is_some() {
                    eprintln!("{}", i18n::err_cli_too_many_args(lang, "ls"));
                    return 2;
                }
                path = Some(other.into());
            }
        }
    }

    let Some(root) = resolve_root(lang, config) else {
        return 1;
    };
    let target = match resolve_existing_under_root(&root, path.as_deref()) {
        Ok(p) => p,
        Err(e) => return cli_error(lang, "ls", &e),
    };
    let entries = match scratch::list(&target) {
        Ok(e) => e,
        Err(e) => return cli_error(lang, "ls", &e),
    };

    let use_color = io::stdout().is_terminal();
    let stdout = io::stdout();
    let mut out = stdout.lock();
    for entry in entries {
        let line = format_ls_line(&entry, long, use_color);
        let _ = writeln!(out, "{line}");
    }
    0
}

/// `ls` の 1 行を組み立てる。デフォルトは name のみ、`-l` で `<mtime>\t<size>\t<name>`。
fn format_ls_line(entry: &Entry, long: bool, use_color: bool) -> String {
    let name = format_name(entry, use_color);
    if !long {
        return name;
    }
    let mtime: DateTime<Local> = entry.modified.into();
    let mtime = mtime.format("%Y-%m-%d %H:%M:%S");
    let size = if entry.is_dir {
        "-".to_string()
    } else {
        // ファイル本体のサイズ。symlink は lstat 由来 (link 自体の size) で OK
        match std::fs::symlink_metadata(&entry.path) {
            Ok(m) => m.size().to_string(),
            Err(_) => "-".to_string(),
        }
    };
    format!("{mtime}\t{size}\t{name}")
}

fn format_name(entry: &Entry, use_color: bool) -> String {
    // 機械可読寄りに倒すため非 TTY 出力では装飾を一切付けない (`/` も色も)。
    // TTY 出力ではディレクトリを ls 慣習に合わせて青字 + 末尾 `/` で示す。
    if !use_color {
        return entry.name.clone();
    }
    if entry.is_dir {
        format!("\x1b[1;34m{}/\x1b[0m", entry.name)
    } else {
        entry.name.clone()
    }
}

fn cmd_tree(lang: Lang, config: &Config, args: Vec<String>) -> i32 {
    let mut path: Option<String> = None;
    for a in args {
        match a.as_str() {
            other if other.starts_with('-') => {
                eprintln!("{}", i18n::err_cli_unknown_flag(lang, "tree", other));
                return 2;
            }
            other => {
                if path.is_some() {
                    eprintln!("{}", i18n::err_cli_too_many_args(lang, "tree"));
                    return 2;
                }
                path = Some(other.into());
            }
        }
    }

    let Some(root) = resolve_root(lang, config) else {
        return 1;
    };
    let target = match resolve_existing_under_root(&root, path.as_deref()) {
        Ok(p) => p,
        Err(e) => return cli_error(lang, "tree", &e),
    };
    println!("{}", scratch::tree(lang, &target, 4, 100));
    0
}

fn cmd_new(lang: Lang, config: &Config, args: Vec<String>) -> i32 {
    let mut no_edit = false;
    let mut name: Option<String> = None;
    for a in args {
        match a.as_str() {
            "--no-edit" => no_edit = true,
            other if other.starts_with('-') => {
                eprintln!("{}", i18n::err_cli_unknown_flag(lang, "new", other));
                return 2;
            }
            other => {
                if name.is_some() {
                    eprintln!("{}", i18n::err_cli_too_many_args(lang, "new"));
                    return 2;
                }
                name = Some(other.into());
            }
        }
    }
    let Some(name) = name else {
        eprintln!("{}", i18n::err_cli_arg_required(lang, "new", "<name>"));
        return 2;
    };
    let Some(root) = resolve_root(lang, config) else {
        return 1;
    };
    let path = match scratch::create_file(&root, &name) {
        Ok(p) => p,
        Err(e) => return cli_error(lang, "new", &e),
    };
    if no_edit {
        println!("{}", path.display());
        return 0;
    }
    match external::spawn_editor(lang, &path, config.editor.as_deref()) {
        Ok(status) => status.code().unwrap_or(0),
        Err(e) => cli_error(lang, "new", &e),
    }
}

fn cmd_mkdir(lang: Lang, config: &Config, args: Vec<String>) -> i32 {
    let mut name: Option<String> = None;
    for a in args {
        match a.as_str() {
            other if other.starts_with('-') => {
                eprintln!("{}", i18n::err_cli_unknown_flag(lang, "mkdir", other));
                return 2;
            }
            other => {
                if name.is_some() {
                    eprintln!("{}", i18n::err_cli_too_many_args(lang, "mkdir"));
                    return 2;
                }
                name = Some(other.into());
            }
        }
    }
    let Some(name) = name else {
        eprintln!("{}", i18n::err_cli_arg_required(lang, "mkdir", "<name>"));
        return 2;
    };
    let Some(root) = resolve_root(lang, config) else {
        return 1;
    };
    let path = match scratch::create_dir(&root, &name) {
        Ok(p) => p,
        Err(e) => return cli_error(lang, "mkdir", &e),
    };
    println!("{}", path.display());
    0
}

fn cmd_edit(lang: Lang, config: &Config, args: Vec<String>) -> i32 {
    let mut name: Option<String> = None;
    for a in args {
        match a.as_str() {
            other if other.starts_with('-') => {
                eprintln!("{}", i18n::err_cli_unknown_flag(lang, "edit", other));
                return 2;
            }
            other => {
                if name.is_some() {
                    eprintln!("{}", i18n::err_cli_too_many_args(lang, "edit"));
                    return 2;
                }
                name = Some(other.into());
            }
        }
    }
    let Some(name) = name else {
        eprintln!("{}", i18n::err_cli_arg_required(lang, "edit", "<name>"));
        return 2;
    };
    let Some(root) = resolve_root(lang, config) else {
        return 1;
    };
    let path = match resolve_existing_under_root(&root, Some(&name)) {
        Ok(p) => p,
        Err(e) => return cli_error(lang, "edit", &e),
    };
    match external::spawn_editor(lang, &path, config.editor.as_deref()) {
        Ok(status) => status.code().unwrap_or(0),
        Err(e) => cli_error(lang, "edit", &e),
    }
}

fn cmd_shell(lang: Lang, config: &Config, args: Vec<String>) -> i32 {
    let mut dir: Option<String> = None;
    for a in args {
        match a.as_str() {
            other if other.starts_with('-') => {
                eprintln!("{}", i18n::err_cli_unknown_flag(lang, "shell", other));
                return 2;
            }
            other => {
                if dir.is_some() {
                    eprintln!("{}", i18n::err_cli_too_many_args(lang, "shell"));
                    return 2;
                }
                dir = Some(other.into());
            }
        }
    }
    let Some(root) = resolve_root(lang, config) else {
        return 1;
    };
    let target = match resolve_existing_under_root(&root, dir.as_deref()) {
        Ok(p) => p,
        Err(e) => return cli_error(lang, "shell", &e),
    };
    if !target.is_dir() {
        eprintln!("{}", i18n::err_cli_not_a_directory(lang, &target.display()));
        return 1;
    }
    match external::spawn_shell(lang, &target, config.shell.as_deref()) {
        Ok(status) => status.code().unwrap_or(0),
        Err(e) => cli_error(lang, "shell", &e),
    }
}

fn cmd_rm(lang: Lang, config: &Config, args: Vec<String>) -> i32 {
    let mut force = false;
    let mut recursive = false;
    let mut name: Option<String> = None;
    for a in args {
        match a.as_str() {
            "-f" | "--force" => force = true,
            "-r" | "-R" | "--recursive" => recursive = true,
            // 慣用の -rf / -fr 短縮表記を一発で受ける (rm の UX 互換)
            "-rf" | "-fr" | "-Rf" | "-fR" => {
                force = true;
                recursive = true;
            }
            other if other.starts_with('-') => {
                eprintln!("{}", i18n::err_cli_unknown_flag(lang, "rm", other));
                return 2;
            }
            other => {
                if name.is_some() {
                    eprintln!("{}", i18n::err_cli_too_many_args(lang, "rm"));
                    return 2;
                }
                name = Some(other.into());
            }
        }
    }
    let Some(name) = name else {
        eprintln!("{}", i18n::err_cli_arg_required(lang, "rm", "<name>"));
        return 2;
    };
    let Some(root) = resolve_root(lang, config) else {
        return 1;
    };
    let path = match resolve_existing_under_root(&root, Some(&name)) {
        Ok(p) => p,
        Err(e) => return cli_error(lang, "rm", &e),
    };
    let entry = match scratch::entry_from_path(&path) {
        Ok(e) => e,
        Err(e) => return cli_error(lang, "rm", &e),
    };

    // ディレクトリは `-r` 必須 (誤って中身ごと吹き飛ばさないため、unix rm と同じ UX)
    if entry.is_dir && !recursive {
        eprintln!("{}", i18n::err_cli_rm_dir_needs_r(lang, &entry.name));
        return 1;
    }

    if !force && !confirm_delete(lang, &entry) {
        eprintln!("{}", i18n::status_cli_rm_cancelled(lang));
        return 1;
    }

    match scratch::remove(&entry) {
        Ok(()) => 0,
        Err(e) => cli_error(lang, "rm", &e),
    }
}

/// stdin が TTY なら y/N 確認、非 TTY (パイプ等) なら自動で false (拒否) を返す。
/// 非対話で消しちゃう事故を防ぐため、非 TTY では明示的に -f が必要。
fn confirm_delete(lang: Lang, entry: &Entry) -> bool {
    let stdin = io::stdin();
    if !stdin.is_terminal() {
        return false;
    }
    let prompt = if entry.is_dir {
        i18n::confirm_delete_dir(lang, &entry.name)
    } else {
        i18n::confirm_delete_file(lang, &entry.name)
    };
    eprint!("{prompt} [y/N] ");
    let _ = io::stderr().flush();
    let mut line = String::new();
    if stdin.lock().read_line(&mut line).is_err() {
        return false;
    }
    matches!(line.trim(), "y" | "Y" | "yes")
}

fn cmd_mv(lang: Lang, config: &Config, args: Vec<String>) -> i32 {
    let mut positional = Vec::new();
    for a in args {
        if a.starts_with('-') {
            eprintln!("{}", i18n::err_cli_unknown_flag(lang, "mv", &a));
            return 2;
        }
        positional.push(a);
    }
    if positional.len() != 2 {
        eprintln!("{}", i18n::err_cli_arg_required(lang, "mv", "<old> <new>"));
        return 2;
    }
    let old = &positional[0];
    let new = &positional[1];

    let Some(root) = resolve_root(lang, config) else {
        return 1;
    };
    let path = match resolve_existing_under_root(&root, Some(old)) {
        Ok(p) => p,
        Err(e) => return cli_error(lang, "mv", &e),
    };
    let entry = match scratch::entry_from_path(&path) {
        Ok(e) => e,
        Err(e) => return cli_error(lang, "mv", &e),
    };
    match scratch::rename(&entry, new) {
        Ok(_) => 0,
        Err(e) => cli_error(lang, "mv", &e),
    }
}

fn cmd_path(lang: Lang, config: &Config, args: Vec<String>) -> i32 {
    let mut name: Option<String> = None;
    for a in args {
        match a.as_str() {
            other if other.starts_with('-') => {
                eprintln!("{}", i18n::err_cli_unknown_flag(lang, "path", other));
                return 2;
            }
            other => {
                if name.is_some() {
                    eprintln!("{}", i18n::err_cli_too_many_args(lang, "path"));
                    return 2;
                }
                name = Some(other.into());
            }
        }
    }
    let Some(root) = resolve_root(lang, config) else {
        return 1;
    };
    let target = match resolve_existing_under_root(&root, name.as_deref()) {
        Ok(p) => p,
        Err(e) => return cli_error(lang, "path", &e),
    };
    println!("{}", target.display());
    0
}

fn cmd_find(lang: Lang, config: &Config, args: Vec<String>) -> i32 {
    let mut long = false;
    let mut query: Option<String> = None;
    let mut path: Option<String> = None;
    for a in args {
        match a.as_str() {
            "-l" | "--long" => long = true,
            other if other.starts_with('-') => {
                eprintln!("{}", i18n::err_cli_unknown_flag(lang, "find", other));
                return 2;
            }
            other => {
                if query.is_none() {
                    query = Some(other.into());
                } else if path.is_none() {
                    path = Some(other.into());
                } else {
                    eprintln!("{}", i18n::err_cli_too_many_args(lang, "find"));
                    return 2;
                }
            }
        }
    }
    let Some(query) = query else {
        eprintln!("{}", i18n::err_cli_arg_required(lang, "find", "<query>"));
        return 2;
    };
    let Some(root) = resolve_root(lang, config) else {
        return 1;
    };
    let target = match resolve_existing_under_root(&root, path.as_deref()) {
        Ok(p) => p,
        Err(e) => return cli_error(lang, "find", &e),
    };
    let entries = match scratch::list(&target) {
        Ok(e) => e,
        Err(e) => return cli_error(lang, "find", &e),
    };

    let q = query.to_lowercase();
    let use_color = io::stdout().is_terminal();
    let stdout = io::stdout();
    let mut out = stdout.lock();
    for entry in entries {
        if !entry.name.to_lowercase().contains(&q) {
            continue;
        }
        let line = format_ls_line(&entry, long, use_color);
        let _ = writeln!(out, "{line}");
    }
    0
}

fn resolve_root(lang: Lang, config: &Config) -> Option<PathBuf> {
    match scratch::root(config.dir.as_deref()) {
        Ok(r) => Some(r),
        Err(e) => {
            eprintln!("{}", i18n::err_cli_root(lang, &e));
            None
        }
    }
}

/// 相対パス引数を root 配下の存在パスへ解決する。絶対パス・root escape は拒否。
fn resolve_existing_under_root(root: &Path, rel: Option<&str>) -> io::Result<PathBuf> {
    let target = match rel {
        None | Some("") => return scratch::ensure_under_root(root, root),
        Some(p) => {
            let path = Path::new(p);
            if path.is_absolute() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "absolute paths are not allowed",
                ));
            }
            root.join(p)
        }
    };
    scratch::ensure_under_root(root, &target)
}

fn cli_error(lang: Lang, sub: &str, e: &dyn std::fmt::Display) -> i32 {
    eprintln!("{}", i18n::err_cli_op(lang, sub, e));
    1
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU32, Ordering};

    use super::*;

    fn temp_root() -> PathBuf {
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("chira-cli-{}-{}", std::process::id(), n));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn is_subcommand_matches_known() {
        for s in [
            "ls", "tree", "new", "mkdir", "edit", "shell", "rm", "mv", "path", "find",
        ] {
            assert!(is_subcommand(s), "expected {s} to be a subcommand");
        }
        assert!(!is_subcommand(""));
        assert!(!is_subcommand("status"));
        assert!(!is_subcommand("--cd-file"));
    }

    #[test]
    fn resolve_existing_under_root_rejects_absolute_and_escape() {
        let root = temp_root();
        // 絶対パスは拒否
        assert!(resolve_existing_under_root(&root, Some("/etc")).is_err());
        // .. で root の外へ出るのも拒否 (canonicalize で見抜く)
        assert!(resolve_existing_under_root(&root, Some("..")).is_err());
        // 不在は IO エラー
        assert!(resolve_existing_under_root(&root, Some("missing")).is_err());
        // 引数 None は root 自身を返す
        let r = resolve_existing_under_root(&root, None).unwrap();
        assert_eq!(r, root.canonicalize().unwrap());
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn format_ls_line_default_and_long() {
        let root = temp_root();
        scratch::create_file(&root, "f.md").unwrap();
        let entry = scratch::list(&root)
            .unwrap()
            .into_iter()
            .find(|e| e.name == "f.md")
            .unwrap();
        // デフォルトは name のみ (非 TTY 想定の use_color=false)
        assert_eq!(format_ls_line(&entry, false, false), "f.md");
        // long は tab 区切り 3 カラム
        let long = format_ls_line(&entry, true, false);
        let cols: Vec<&str> = long.split('\t').collect();
        assert_eq!(cols.len(), 3);
        assert_eq!(cols[2], "f.md");
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn cmd_ls_lists_root_entries() {
        let root = temp_root();
        scratch::create_file(&root, "a.md").unwrap();
        scratch::create_dir(&root, "ws").unwrap();
        let config = Config {
            dir: Some(root.to_string_lossy().into_owned()),
            ..Default::default()
        };
        let code = cmd_ls(Lang::En, &config, vec![]);
        assert_eq!(code, 0);
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn cmd_new_no_edit_creates_file() {
        let root = temp_root();
        let config = Config {
            dir: Some(root.to_string_lossy().into_owned()),
            ..Default::default()
        };
        let code = cmd_new(
            Lang::En,
            &config,
            vec!["note.md".into(), "--no-edit".into()],
        );
        assert_eq!(code, 0);
        assert!(root.join("note.md").exists());
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn cmd_mkdir_creates_directory() {
        let root = temp_root();
        let config = Config {
            dir: Some(root.to_string_lossy().into_owned()),
            ..Default::default()
        };
        let code = cmd_mkdir(Lang::En, &config, vec!["ws".into()]);
        assert_eq!(code, 0);
        assert!(root.join("ws").is_dir());
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn cmd_rm_requires_r_for_directory() {
        let root = temp_root();
        scratch::create_dir(&root, "ws").unwrap();
        let config = Config {
            dir: Some(root.to_string_lossy().into_owned()),
            ..Default::default()
        };
        // -r なしのディレクトリ削除は失敗
        let code = cmd_rm(Lang::En, &config, vec!["ws".into(), "-f".into()]);
        assert_eq!(code, 1);
        assert!(root.join("ws").exists());
        // -rf で削除
        let code = cmd_rm(Lang::En, &config, vec!["ws".into(), "-rf".into()]);
        assert_eq!(code, 0);
        assert!(!root.join("ws").exists());
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn cmd_rm_force_removes_file_without_prompt() {
        let root = temp_root();
        scratch::create_file(&root, "a.md").unwrap();
        let config = Config {
            dir: Some(root.to_string_lossy().into_owned()),
            ..Default::default()
        };
        let code = cmd_rm(Lang::En, &config, vec!["a.md".into(), "-f".into()]);
        assert_eq!(code, 0);
        assert!(!root.join("a.md").exists());
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn cmd_mv_renames_entry() {
        let root = temp_root();
        scratch::create_file(&root, "a.md").unwrap();
        let config = Config {
            dir: Some(root.to_string_lossy().into_owned()),
            ..Default::default()
        };
        let code = cmd_mv(Lang::En, &config, vec!["a.md".into(), "b.md".into()]);
        assert_eq!(code, 0);
        assert!(!root.join("a.md").exists());
        assert!(root.join("b.md").exists());
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn cmd_path_prints_canonical_target() {
        let root = temp_root();
        scratch::create_file(&root, "a.md").unwrap();
        let config = Config {
            dir: Some(root.to_string_lossy().into_owned()),
            ..Default::default()
        };
        let code = cmd_path(Lang::En, &config, vec!["a.md".into()]);
        assert_eq!(code, 0);
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn cmd_find_filters_by_substring() {
        let root = temp_root();
        scratch::create_file(&root, "alpha.md").unwrap();
        scratch::create_file(&root, "beta.md").unwrap();
        let config = Config {
            dir: Some(root.to_string_lossy().into_owned()),
            ..Default::default()
        };
        // 失敗しないことだけ確認 (stdout の中身は手動検証)
        let code = cmd_find(Lang::En, &config, vec!["alp".into()]);
        assert_eq!(code, 0);
        std::fs::remove_dir_all(&root).unwrap();
    }
}
