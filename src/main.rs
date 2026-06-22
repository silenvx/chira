mod app;
mod archive;
mod cli;
mod config;
mod external;
mod i18n;
mod scratch;
mod ui;

use std::env;
use std::fs;
use std::io;
use std::os::unix::ffi::OsStrExt;
use std::path::PathBuf;
use std::time::Duration;

use ratatui::DefaultTerminal;
use ratatui::crossterm::event::{self, Event};
use ratatui::crossterm::execute;
use ratatui::crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};

use app::{App, Pending};
use config::Config;

fn main() -> io::Result<()> {
    let lang = i18n::lang();
    let argv: Vec<String> = env::args().skip(1).collect();

    // README wrapper が `--cd-file <tmp>` を前置するため、argv 中の最初の非フラグが subcommand なら CLI ディスパッチ。
    // CLI モードでは --cd-file は無視する (wrapper の cd は empty file で no-op になる)。
    if let Some(sub_idx) = first_non_flag_index(&argv) {
        let first = &argv[sub_idx];
        if cli::is_subcommand(first) {
            let sub = first.clone();
            let sub_args = argv[sub_idx + 1..].to_vec();
            let config = config::load(lang);
            let code = cli::run(lang, &config, &sub, sub_args);
            std::process::exit(code);
        } else {
            eprint!("{}", i18n::err_unknown_arg(lang, first));
            std::process::exit(2);
        }
    }

    let cd_file = match parse_args(&argv, lang) {
        Ok(v) => v,
        Err(msg) => {
            eprint!("{msg}");
            std::process::exit(2);
        }
    };

    let config = config::load(lang);
    // 起動時 sweep (opt-in)。失敗しても TUI 起動は止めず、stderr に warning が残る
    if config.archive.on_startup
        && let Err(e) = run_startup_sweep(lang, &config)
    {
        eprintln!("{}", i18n::err_gc_sweep(lang, &e));
    }
    let mut app = App::new(config.dir.as_deref())?;
    app.actions = config.actions.clone();
    app.default_action = config.default_action.clone();
    let mut terminal = ratatui::init();
    let result = run(&mut terminal, &mut app, &config);
    ratatui::restore();
    result?;

    // 起動元シェルが cd するための最終ディレクトリを書き出す
    if let Some(path) = cd_file {
        fs::write(path, app.cwd.as_os_str().as_bytes())?;
    }
    Ok(())
}

fn run_startup_sweep(lang: i18n::Lang, config: &Config) -> io::Result<()> {
    // ttl_days 未設定なら on_startup を立てていても no-op (誤って全消えを防ぐ)
    let Some(ttl_days) = config.archive.ttl_days.filter(|d| *d > 0) else {
        return Ok(());
    };
    let root = scratch::root(config.dir.as_deref())?;
    let archive_dir_str = config.archive.dir.as_deref();
    let home = env::var_os("HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from);
    let archive_dir = match archive_dir_str.filter(|s| !s.is_empty()) {
        Some(s) => {
            let resolved = scratch::expand_tilde(s, home.as_deref())?;
            // 相対 path は detect_archive_root_conflict の比較が機能するよう絶対化
            if resolved.is_absolute() {
                resolved
            } else {
                env::current_dir()?.join(resolved)
            }
        }
        None => root.join(archive::DEFAULT_ARCHIVE_DIRNAME),
    };
    let opts = archive::Options {
        root: &root,
        archive_dir,
        ttl: std::time::Duration::from_secs(ttl_days * 86_400),
        keep_patterns: &config.archive.keep,
        dry_run: false,
        now: std::time::SystemTime::now(),
    };
    let report = archive::sweep(lang, opts)?;
    for err in &report.errors {
        eprintln!("{err}");
    }
    Ok(())
}

/// argv 中の最初の「-」始まりでないトークンの index を返す。
/// シェル wrapper が `--cd-file <tmp>` を前置するため、サブコマンドは必ずしも先頭ではない。
/// `--cd-file` 以外のフラグが先に出てきた場合は CLI ディスパッチを諦め None を返す
/// (TUI 経路の parse_args に委ねて未知フラグを `exit 2` で reject させるため。
/// `--help` / `--bogus` 等のフラグを silent に skip すると subcommand が誤起動する)。
fn first_non_flag_index(argv: &[String]) -> Option<usize> {
    let mut i = 0;
    while i < argv.len() {
        let arg = &argv[i];
        if !arg.starts_with('-') {
            return Some(i);
        }
        if arg == "--cd-file" {
            i += 2;
        } else if arg.starts_with("--cd-file=") {
            i += 1;
        } else {
            return None;
        }
    }
    None
}

/// `--cd-file <path>` を取り出す。`--help` / `--version` は表示して終了する。
fn parse_args(argv: &[String], lang: i18n::Lang) -> Result<Option<PathBuf>, String> {
    let mut cd_file = None;
    let mut iter = argv.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "-h" | "--help" => {
                print!("{}", i18n::usage(lang));
                std::process::exit(0);
            }
            "-V" | "--version" => {
                println!("chira {}", env!("CARGO_PKG_VERSION"));
                std::process::exit(0);
            }
            "--cd-file" => {
                let path = iter
                    .next()
                    .ok_or_else(|| i18n::err_cd_file_needs_arg(lang).to_string())?;
                cd_file = Some(PathBuf::from(path));
            }
            other => {
                if let Some(path) = other.strip_prefix("--cd-file=") {
                    cd_file = Some(PathBuf::from(path));
                } else {
                    return Err(i18n::err_unknown_arg(lang, other));
                }
            }
        }
    }
    Ok(cd_file)
}

fn run(terminal: &mut DefaultTerminal, app: &mut App, config: &Config) -> io::Result<()> {
    while !app.should_quit {
        terminal.draw(|frame| ui::render(frame, app))?;
        if event::poll(Duration::from_millis(250))?
            && let Event::Key(key) = event::read()?
        {
            app.on_key(key);
        }
        if let Some(pending) = app.pending.take() {
            // 起動失敗 ($EDITOR/$SHELL 不在・対象ディレクトリ消失等) は回復可能なので
            // TUI を落とさず status に出して継続する
            if let Err(e) = run_external(terminal, app.lang, &pending, config) {
                app.status = i18n::err_external_launch(app.lang, &e);
            }
            // 外部プロセス (shell での agent 実行等) が作ったファイルを取り込む
            app.refresh();
        }
    }
    Ok(())
}

/// TUI から一旦抜けて外部プロセスを前面で実行し、終了後に TUI へ復帰する
fn run_external(
    terminal: &mut DefaultTerminal,
    lang: i18n::Lang,
    pending: &Pending,
    config: &Config,
) -> io::Result<()> {
    disable_raw_mode()?;
    execute!(io::stdout(), LeaveAlternateScreen)?;

    let result = match pending {
        Pending::Editor(path) => {
            external::spawn_editor(lang, path, config.editor.as_deref()).map(|_| ())
        }
        Pending::Shell(dir) => {
            external::spawn_shell(lang, dir, config.shell.as_deref()).map(|_| ())
        }
        Pending::Run {
            dir,
            root,
            command,
            action_name,
        } => external::spawn_run(dir, root, command).map(|status| {
            // 失敗 (exit != 0、シグナル終了含む) は sentinel を書いて一覧で `[!]` 表示する。
            // 成功なら既存の sentinel を消して retry のクリアを行う。失敗 dir を残す方針 (auto-rollback しない)
            // の下で「半端な状態」を一覧から見分けるための装置。sentinel I/O 自体の失敗は best-effort で握り潰す。
            let code = external::exit_code_from_status(status);
            if code == 0 {
                let _ = scratch::clear_bootstrap_failed(dir);
            } else {
                let _ = scratch::write_bootstrap_failed(dir, action_name, code);
            }
        }),
    };

    enable_raw_mode()?;
    execute!(io::stdout(), EnterAlternateScreen)?;
    terminal.clear()?;
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| (*s).into()).collect()
    }

    #[test]
    fn parse_args_extracts_cd_file() {
        let l = i18n::Lang::En;
        assert_eq!(parse_args(&args(&[]), l).unwrap(), None);
        assert_eq!(
            parse_args(&args(&["--cd-file", "/tmp/x"]), l).unwrap(),
            Some(PathBuf::from("/tmp/x"))
        );
        assert_eq!(
            parse_args(&args(&["--cd-file=/tmp/y"]), l).unwrap(),
            Some(PathBuf::from("/tmp/y"))
        );
        assert!(parse_args(&args(&["--cd-file"]), l).is_err());
        assert!(parse_args(&args(&["--bogus"]), l).is_err());
    }

    /// README のシェル wrapper 経由でも CLI subcommand が動く (`--cd-file <tmp>` を飛ばして検出)
    #[test]
    fn first_non_flag_index_skips_cd_file_pair() {
        // 引数なし → None (TUI モード)
        assert_eq!(first_non_flag_index(&args(&[])), None);
        // 先頭サブコマンド (素の `chira ls`)
        assert_eq!(first_non_flag_index(&args(&["ls"])), Some(0));
        // wrapper 経由 (`chira --cd-file /tmp/x ls`) は index 2 を返す
        assert_eq!(
            first_non_flag_index(&args(&["--cd-file", "/tmp/x", "ls"])),
            Some(2)
        );
        // 既存の `--cd-file=/tmp/y` 形 (value 同 token) も次の token がサブコマンド
        assert_eq!(
            first_non_flag_index(&args(&["--cd-file=/tmp/y", "ls"])),
            Some(1)
        );
        // フラグのみ (`chira --cd-file /tmp/x`) → None (TUI モード継続)
        assert_eq!(first_non_flag_index(&args(&["--cd-file", "/tmp/x"])), None);
        // --cd-file 以外のフラグが先にあると CLI ディスパッチを諦め None を返す
        // (parse_args に委ねて未知フラグを exit 2 で reject させる。silent skip は subcommand 誤起動の原因)
        assert_eq!(first_non_flag_index(&args(&["--unknown", "ls"])), None);
        assert_eq!(first_non_flag_index(&args(&["--help", "ls"])), None);
    }
}
