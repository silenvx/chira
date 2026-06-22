mod app;
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
    let mut app = App::new(config.dir.as_deref())?;
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

/// argv 中の最初の「-」始まりでないトークンの index を返す。
/// シェル wrapper が `--cd-file <tmp>` を前置するため、サブコマンドは必ずしも先頭ではない。
fn first_non_flag_index(argv: &[String]) -> Option<usize> {
    let mut i = 0;
    while i < argv.len() {
        let arg = &argv[i];
        if !arg.starts_with('-') {
            return Some(i);
        }
        // `--cd-file <value>` のように引数を取る flag は value 部をスキップする
        if arg == "--cd-file" {
            i += 2;
        } else {
            i += 1;
        }
    }
    None
}

/// `--cd-file <path>` を取り出す。`--help` は usage を表示して終了する。
fn parse_args(argv: &[String], lang: i18n::Lang) -> Result<Option<PathBuf>, String> {
    let mut cd_file = None;
    let mut iter = argv.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "-h" | "--help" => {
                print!("{}", i18n::usage(lang));
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
        // 不明 flag は skip しない (parse_args 側で reject される)
        assert_eq!(first_non_flag_index(&args(&["--unknown", "foo"])), Some(1));
    }
}
