mod app;
mod scratch;
mod ui;

use std::env;
use std::fs;
use std::io;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use ratatui::DefaultTerminal;
use ratatui::crossterm::event::{self, Event};
use ratatui::crossterm::execute;
use ratatui::crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};

use app::{App, Pending};

const USAGE: &str = "\
chira — 一時的な scratch ディレクトリを管理する TUI

usage: chira [--cd-file <path>]

  --cd-file <path>   終了時に最終ディレクトリを <path> へ書き出す
                     (シェル関数で cd するための連携用。README 参照)
  -h, --help         このヘルプを表示
";

fn main() -> io::Result<()> {
    let cd_file = match parse_args() {
        Ok(v) => v,
        Err(msg) => {
            eprint!("{msg}");
            std::process::exit(2);
        }
    };

    let mut app = App::new()?;
    let mut terminal = ratatui::init();
    let result = run(&mut terminal, &mut app);
    ratatui::restore();
    result?;

    // 起動元シェルが cd するための最終ディレクトリを書き出す
    if let Some(path) = cd_file {
        fs::write(path, app.cwd.as_os_str().as_bytes())?;
    }
    Ok(())
}

/// `--cd-file <path>` を取り出す。`--help` は usage を表示して終了する。
fn parse_args() -> Result<Option<PathBuf>, String> {
    let mut cd_file = None;
    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-h" | "--help" => {
                print!("{USAGE}");
                std::process::exit(0);
            }
            "--cd-file" => {
                let path = args
                    .next()
                    .ok_or_else(|| "--cd-file には引数が必要です\n".to_string())?;
                cd_file = Some(PathBuf::from(path));
            }
            other => {
                if let Some(path) = other.strip_prefix("--cd-file=") {
                    cd_file = Some(PathBuf::from(path));
                } else {
                    return Err(format!("不明な引数: {other}\n{USAGE}"));
                }
            }
        }
    }
    Ok(cd_file)
}

fn run(terminal: &mut DefaultTerminal, app: &mut App) -> io::Result<()> {
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
            if let Err(e) = run_external(terminal, &pending) {
                app.status = format!("外部プロセスの起動に失敗: {e}");
            }
            // 外部プロセス (shell での agent 実行等) が作ったファイルを取り込む
            app.refresh();
        }
    }
    Ok(())
}

/// TUI から一旦抜けて外部プロセスを前面で実行し、終了後に TUI へ復帰する
fn run_external(terminal: &mut DefaultTerminal, pending: &Pending) -> io::Result<()> {
    disable_raw_mode()?;
    execute!(io::stdout(), LeaveAlternateScreen)?;

    let result = match pending {
        Pending::Editor(path) => spawn_editor(path),
        Pending::Shell(dir) => spawn_shell(dir),
    };

    enable_raw_mode()?;
    execute!(io::stdout(), EnterAlternateScreen)?;
    terminal.clear()?;
    result
}

/// $EDITOR を shell の語分割規則 (shell-words) で argv に分解する。
/// 引数付き (`code --wait`) と quote 済みスペース入りパス (`'/My Apps/subl' -w`) の両方を扱う (whitespace split は後者を壊す)。
fn editor_argv(editor: &str) -> io::Result<Vec<String>> {
    let argv = shell_words::split(editor)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, format!("$EDITOR の解析に失敗: {e}")))?;
    if argv.is_empty() {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "$EDITOR が空です"));
    }
    Ok(argv)
}

fn spawn_editor(path: &Path) -> io::Result<()> {
    let editor = env::var("EDITOR").unwrap_or_else(|_| "vi".into());
    let argv = editor_argv(&editor)?;
    Command::new(&argv[0]).args(&argv[1..]).arg(path).status()?;
    Ok(())
}

fn spawn_shell(dir: &Path) -> io::Result<()> {
    let shell = env::var("SHELL").unwrap_or_else(|_| "/bin/sh".into());
    Command::new(shell).current_dir(dir).status()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn editor_argv_handles_args_and_quoted_paths() {
        assert_eq!(editor_argv("vi").unwrap(), ["vi"]);
        assert_eq!(editor_argv("code --wait").unwrap(), ["code", "--wait"]);
        // quote 済みのスペース入りパスは 1 引数として保たれる
        assert_eq!(editor_argv("'/My Apps/subl' -w").unwrap(), ["/My Apps/subl", "-w"]);
        assert!(editor_argv("").is_err());
    }
}
