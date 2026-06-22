use std::io::{self, BufRead, IsTerminal, Write};
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use chrono::{DateTime, Local};

use crate::archive;
use crate::config::Config;
use crate::external;
use crate::i18n::{self, Lang};
use crate::scratch::{self, Entry};

const SUBCOMMANDS: &[&str] = &[
    "ls", "tree", "new", "mkdir", "edit", "shell", "rm", "mv", "path", "find", "gc", "archive",
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
        "gc" | "archive" => cmd_gc(lang, config, args),
        _ => {
            eprint!("{}", i18n::err_unknown_subcommand(lang, sub));
            2
        }
    }
}

#[derive(Default)]
struct GcArgs {
    ttl: Option<String>,
    archive_dir: Option<PathBuf>,
    dry_run: bool,
}

fn parse_gc_args(lang: Lang, args: Vec<String>) -> Result<GcArgs, String> {
    let mut out = GcArgs::default();
    let mut iter = args.into_iter();
    // option の引数が次の `--xxx` flag を誤って消費する事故を防ぐガード
    // (例: `gc --ttl 30d --archive-dir --dry-run` で --dry-run が archive-dir 値になり、
    //  silent に dry-run が無効化されて実 archive されるのを防ぐ)
    let take_value = |iter: &mut std::vec::IntoIter<String>, opt: &str| -> Result<String, String> {
        let v = iter
            .next()
            .ok_or_else(|| i18n::err_option_needs_arg(lang, opt))?;
        if v.starts_with('-') {
            return Err(i18n::err_option_needs_arg(lang, opt));
        }
        Ok(v)
    };
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--dry-run" => out.dry_run = true,
            "--ttl" => out.ttl = Some(take_value(&mut iter, "--ttl")?),
            "--archive-dir" => {
                out.archive_dir = Some(PathBuf::from(take_value(&mut iter, "--archive-dir")?));
            }
            other => {
                if let Some(v) = other.strip_prefix("--ttl=") {
                    out.ttl = Some(v.to_string());
                } else if let Some(v) = other.strip_prefix("--archive-dir=") {
                    out.archive_dir = Some(PathBuf::from(v));
                } else {
                    return Err(i18n::err_cli_unknown_flag(lang, "gc", other));
                }
            }
        }
    }
    Ok(out)
}

fn cmd_gc(lang: Lang, config: &Config, args: Vec<String>) -> i32 {
    let parsed = match parse_gc_args(lang, args) {
        Ok(a) => a,
        Err(msg) => {
            eprint!("{msg}");
            return 2;
        }
    };
    // TTL は CLI > config の順で解決し、両方未設定/解析不能なら exit 2 で終了する
    let ttl = match parsed.ttl.as_deref() {
        Some(s) => match archive::parse_duration(s) {
            Ok(d) => d,
            Err(e) => {
                eprint!("{}", i18n::err_gc_ttl_invalid(lang, &e));
                return 2;
            }
        },
        None => match config.archive.ttl_days.filter(|d| *d > 0) {
            Some(d) => Duration::from_secs(d * 86_400),
            None => {
                eprint!("{}", i18n::err_gc_ttl_missing(lang));
                return 2;
            }
        },
    };

    let root = match scratch::root(config.dir.as_deref()) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("{}", i18n::err_gc_sweep(lang, &e));
            return 1;
        }
    };
    let archive_dir = match resolve_archive_dir(
        &root,
        config.archive.dir.as_deref(),
        parsed.archive_dir.as_deref(),
    ) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("{}", i18n::err_gc_sweep(lang, &e));
            return 1;
        }
    };
    let opts = archive::Options {
        root: &root,
        archive_dir,
        ttl,
        keep_patterns: &config.archive.keep,
        dry_run: parsed.dry_run,
        now: SystemTime::now(),
    };
    let report = match archive::sweep(lang, opts) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("{}", i18n::err_gc_sweep(lang, &e));
            return 1;
        }
    };

    if parsed.dry_run {
        if !report.archived.is_empty() {
            println!("{}", i18n::gc_dry_run_header(lang));
            for o in &report.archived {
                println!("{}", i18n::gc_dry_run_entry(&o.name, &o.dest.display()));
            }
        }
        println!(
            "{}",
            i18n::gc_summary_dry_run(
                lang,
                report.archived.len(),
                report.kept,
                report.errors.len()
            )
        );
    } else {
        for o in &report.archived {
            println!("{}", i18n::gc_archived(lang, &o.name, &o.dest.display()));
        }
        println!(
            "{}",
            i18n::gc_summary(
                lang,
                report.archived.len(),
                report.kept,
                report.errors.len()
            )
        );
    }
    for err in &report.errors {
        eprintln!("{err}");
    }
    if !report.errors.is_empty() { 1 } else { 0 }
}

/// archive 先の解決順: CLI flag > config dir > <CHIRA_DIR>/.archive。
/// CLI / config 双方 `~` は HOME へ展開 (シェル非経由起動 / quoted `'~/old'` を一様に扱う)。
/// 相対 path は current_dir で絶対化 (detect_archive_root_conflict の比較が機能するため)
fn resolve_archive_dir(
    root: &Path,
    config_dir: Option<&str>,
    cli: Option<&Path>,
) -> io::Result<PathBuf> {
    let home = std::env::var_os("HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from);
    let resolved = if let Some(p) = cli {
        scratch::expand_tilde(&p.to_string_lossy(), home.as_deref())?
    } else if let Some(s) = config_dir.filter(|s| !s.is_empty()) {
        scratch::expand_tilde(s, home.as_deref())?
    } else {
        return Ok(root.join(archive::DEFAULT_ARCHIVE_DIRNAME));
    };
    if resolved.is_absolute() {
        Ok(resolved)
    } else {
        Ok(std::env::current_dir()?.join(resolved))
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
        Ok(status) => external::exit_code_from_status(status),
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
        Ok(status) => external::exit_code_from_status(status),
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
        Ok(status) => external::exit_code_from_status(status),
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
    // rm は symlink を unix 慣習どおり symlink 自身に対して効かせるため lexical path を使う
    let path = match resolve_lexical_under_root(&root, &name) {
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
    // 大文字小文字を区別せず `y` / `yes` を受ける (`YES` / `Yes` 等の入力でも確認が成立)
    matches!(line.trim().to_lowercase().as_str(), "y" | "yes")
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
    // mv も rm と同様に lexical path を使い、symlink/broken を unix 慣習どおり扱う
    let path = match resolve_lexical_under_root(&root, old) {
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

/// 相対パス引数を root 配下の lexical (非 canonical) パスへ解決する。
/// `resolve_existing_under_root` と異なり symlink を辿らないので、`rm symlink` は unix 慣習どおり
/// symlink 自体を消す (broken symlink も操作可能)。root 自身の指名 (`.` / `""`) は CHIRA_DIR 全消し防止で reject。
fn resolve_lexical_under_root(root: &Path, rel: &str) -> io::Result<PathBuf> {
    let path = Path::new(rel);
    if path.is_absolute() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "absolute paths are not allowed",
        ));
    }
    if rel.is_empty() || rel == "." || rel == "./" {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "cannot operate on the scratch root itself",
        ));
    }
    // 末尾コンポーネントが `..` / `.` だと parent が root 内に収まっても resolved target は root 外を指す
    // (例: `subdir/../..` → resolved = root's parent)。fs::remove_dir_all がその path を walk して CHIRA_DIR
    // 外を消す経路を塞ぐため、最終コンポーネントは通常の name に限定する
    if let Some(file_name) = target_file_name(path)
        && (file_name == ".." || file_name == ".")
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "path component '..' / '.' is not allowed in the last component",
        ));
    }
    let target = root.join(rel);
    scratch::ensure_path_under_root(root, &target)?;
    // 中間 `..` 等で間接的に root 自身を指したケースも拒否 (`subdir/..` 等)
    let canonical_root = root.canonicalize()?;
    let canonical_target = target.canonicalize().unwrap_or_else(|_| target.clone());
    if canonical_target == canonical_root {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "cannot operate on the scratch root itself",
        ));
    }
    Ok(target)
}

/// path の最終 component (file_name) を文字列で返す。`..` / `.` の検査用。
fn target_file_name(path: &Path) -> Option<&std::ffi::OsStr> {
    path.components().next_back().and_then(|c| match c {
        std::path::Component::Normal(s) => Some(s),
        std::path::Component::ParentDir => Some(std::ffi::OsStr::new("..")),
        std::path::Component::CurDir => Some(std::ffi::OsStr::new(".")),
        _ => None,
    })
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
            "ls", "tree", "new", "mkdir", "edit", "shell", "rm", "mv", "path", "find", "gc",
            "archive",
        ] {
            assert!(is_subcommand(s), "expected {s} to be a subcommand");
        }
        assert!(!is_subcommand(""));
        assert!(!is_subcommand("status"));
        assert!(!is_subcommand("--cd-file"));
    }

    #[test]
    fn cmd_gc_returns_2_when_ttl_missing() {
        // TTL は --ttl も config.archive.ttl_days も無いと exit 2 (誤って全消えを防ぐ契約)
        // env::set_var("CHIRA_DIR", ...) でなく config.dir 経由で root を渡す: test runner は
        // 並列実行で env を共有するため set_var は他テストの scratch::root() (env > config 優先)
        // と race し、削除済み dir を canonicalize して "No such file or directory" で flake する。
        let root = temp_root();
        let config = Config {
            dir: Some(root.to_string_lossy().into_owned()),
            ..Default::default() // archive.ttl_days = None
        };
        let code = cmd_gc(Lang::En, &config, vec![]);
        assert_eq!(code, 2);
        std::fs::remove_dir_all(&root).unwrap();
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

    /// 非対話 stdin での confirm スキップを契約として固定する (PR description の安全前提)
    #[test]
    fn cmd_rm_without_force_cancels_when_stdin_not_tty() {
        let root = temp_root();
        scratch::create_file(&root, "a.md").unwrap();
        let config = Config {
            dir: Some(root.to_string_lossy().into_owned()),
            ..Default::default()
        };
        // cargo test の stdin は TTY ではない → confirm_delete が即座に false を返し exit 1
        let code = cmd_rm(Lang::En, &config, vec!["a.md".into()]);
        assert_eq!(code, 1);
        assert!(
            root.join("a.md").exists(),
            "non-TTY 確認スキップで file を消してはならない"
        );
        std::fs::remove_dir_all(&root).unwrap();
    }

    /// `chira rm .` や `chira rm ""` で scratch root を消そうとしても reject する
    #[test]
    fn cmd_rm_rejects_scratch_root_targeting() {
        let root = temp_root();
        scratch::create_file(&root, "keep.md").unwrap();
        let config = Config {
            dir: Some(root.to_string_lossy().into_owned()),
            ..Default::default()
        };
        // `.` で root を指名 → reject (CHIRA_DIR 全消し防止)
        let code = cmd_rm(Lang::En, &config, vec![".".into(), "-rf".into()]);
        assert_eq!(code, 1);
        assert!(root.exists() && root.join("keep.md").exists());
        // 空文字も同様に reject
        let code = cmd_rm(Lang::En, &config, vec!["".into(), "-rf".into()]);
        assert_eq!(code, 1);
        assert!(root.exists() && root.join("keep.md").exists());
        std::fs::remove_dir_all(&root).unwrap();
    }

    /// `subdir/../..` 等の末尾 `..` で CHIRA_DIR の外を指すパス traversal を reject する
    /// (これを許すと fs::remove_dir_all が CHIRA_DIR の親 dir を削除しうる security bug)
    #[test]
    fn cmd_rm_rejects_parent_traversal_to_outside_root() {
        let root = temp_root();
        let subdir = scratch::create_dir(&root, "ws").unwrap();
        scratch::create_file(&subdir, "inner.md").unwrap();
        // root と並ぶ sibling dir を作って、攻撃が成功した場合に消える対象を用意する
        let sibling = root.parent().unwrap().join(format!(
            "chira-sibling-{}-{}",
            std::process::id(),
            "rm-traversal"
        ));
        std::fs::create_dir_all(&sibling).unwrap();
        std::fs::write(sibling.join("important.txt"), b"keep").unwrap();
        let config = Config {
            dir: Some(root.to_string_lossy().into_owned()),
            ..Default::default()
        };
        // ws/../.. は最終 component が `..` で root の外を指す → reject
        let code = cmd_rm(Lang::En, &config, vec!["ws/../..".into(), "-rf".into()]);
        assert_eq!(code, 1);
        assert!(root.exists() && subdir.exists());
        assert!(sibling.join("important.txt").exists(), "sibling は無事");
        std::fs::remove_dir_all(&root).unwrap();
        std::fs::remove_dir_all(&sibling).unwrap();
    }

    /// `subdir/..` のように間接的に root 自身を指すパスも root targeting として reject する
    #[test]
    fn cmd_rm_rejects_indirect_root_via_dotdot() {
        let root = temp_root();
        scratch::create_dir(&root, "ws").unwrap();
        let config = Config {
            dir: Some(root.to_string_lossy().into_owned()),
            ..Default::default()
        };
        let code = cmd_rm(Lang::En, &config, vec!["ws/..".into(), "-rf".into()]);
        assert_eq!(code, 1);
        assert!(root.exists());
        std::fs::remove_dir_all(&root).unwrap();
    }

    /// `chira mv . newname` も同様に scratch root のリネームを reject する
    #[test]
    fn cmd_mv_rejects_scratch_root_targeting() {
        let root = temp_root();
        scratch::create_file(&root, "keep.md").unwrap();
        let config = Config {
            dir: Some(root.to_string_lossy().into_owned()),
            ..Default::default()
        };
        let code = cmd_mv(Lang::En, &config, vec![".".into(), "renamed".into()]);
        assert_eq!(code, 1);
        assert!(root.exists() && root.join("keep.md").exists());
        std::fs::remove_dir_all(&root).unwrap();
    }

    /// rm が symlink 自体を削除する (target を辿らない、unix 慣習)
    #[test]
    fn cmd_rm_removes_symlink_not_target() {
        let root = temp_root();
        let target = scratch::create_file(&root, "target.md").unwrap();
        std::fs::write(&target, "本文").unwrap();
        std::os::unix::fs::symlink(&target, root.join("link")).unwrap();
        let config = Config {
            dir: Some(root.to_string_lossy().into_owned()),
            ..Default::default()
        };
        let code = cmd_rm(Lang::En, &config, vec!["link".into(), "-f".into()]);
        assert_eq!(code, 0);
        assert!(!root.join("link").exists(), "symlink は消える");
        assert!(target.exists(), "target は残る");
        std::fs::remove_dir_all(&root).unwrap();
    }

    /// broken symlink も listing と整合して操作可能 (`chira ls` で見えるなら `chira rm` できる)
    #[test]
    fn cmd_rm_removes_broken_symlink() {
        let root = temp_root();
        std::os::unix::fs::symlink("/nonexistent/target", root.join("broken")).unwrap();
        let config = Config {
            dir: Some(root.to_string_lossy().into_owned()),
            ..Default::default()
        };
        let code = cmd_rm(Lang::En, &config, vec!["broken".into(), "-f".into()]);
        assert_eq!(code, 0);
        assert!(
            root.join("broken").symlink_metadata().is_err(),
            "broken symlink は消える"
        );
        std::fs::remove_dir_all(&root).unwrap();
    }
}
