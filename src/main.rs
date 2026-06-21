mod app;
mod config;
mod i18n;
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
use config::Config;

fn main() -> io::Result<()> {
    let lang = i18n::lang();
    let cd_file = match parse_args(lang) {
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

/// `--cd-file <path>` を取り出す。`--help` は usage を表示して終了する。
fn parse_args(lang: i18n::Lang) -> Result<Option<PathBuf>, String> {
    let mut cd_file = None;
    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-h" | "--help" => {
                print!("{}", i18n::usage(lang));
                std::process::exit(0);
            }
            "--cd-file" => {
                let path = args
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
        Pending::Editor(path) => spawn_editor(lang, path, config.editor.as_deref()),
        Pending::Shell(dir) => spawn_shell(dir, config.shell.as_deref()),
    };

    enable_raw_mode()?;
    execute!(io::stdout(), EnterAlternateScreen)?;
    terminal.clear()?;
    result
}

/// $EDITOR を shell の語分割規則 (shell-words) で argv に分解する。
/// 引数付き (`code --wait`) と quote 済みスペース入りパス (`'/My Apps/subl' -w`) の両方を扱う (whitespace split は後者を壊す)。
fn editor_argv(lang: i18n::Lang, editor: &str) -> io::Result<Vec<String>> {
    let argv = shell_words::split(editor).map_err(|e| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            i18n::err_editor_parse(lang, &e),
        )
    })?;
    if argv.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            i18n::err_editor_empty(lang),
        ));
    }
    Ok(argv)
}

fn spawn_editor(lang: i18n::Lang, path: &Path, config_editor: Option<&str>) -> io::Result<()> {
    let env_editor = env::var("EDITOR").ok();
    let editor = resolve_external(env_editor.as_deref(), config_editor, "vi");
    let argv = editor_argv(lang, &editor)?;
    Command::new(&argv[0]).args(&argv[1..]).arg(path).status()?;
    Ok(())
}

fn spawn_shell(dir: &Path, config_shell: Option<&str>) -> io::Result<()> {
    let env_shell = env::var("SHELL").ok();
    let shell = resolve_external(env_shell.as_deref(), config_shell, "/bin/sh");
    Command::new(shell).current_dir(dir).status()?;
    Ok(())
}

/// 外部プロセス名の解決: env > config > ハードコード default。空文字は未設定扱い。
fn resolve_external(env_val: Option<&str>, config_val: Option<&str>, default: &str) -> String {
    env_val
        .filter(|s| !s.is_empty())
        .or(config_val.filter(|s| !s.is_empty()))
        .unwrap_or(default)
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_external_precedence() {
        // env > config > default
        assert_eq!(resolve_external(Some("nvim"), Some("emacs"), "vi"), "nvim");
        assert_eq!(resolve_external(None, Some("emacs"), "vi"), "emacs");
        assert_eq!(resolve_external(None, None, "vi"), "vi");
        // 空文字は未設定扱いで次の候補へ送る
        assert_eq!(resolve_external(Some(""), Some("emacs"), "vi"), "emacs");
        assert_eq!(resolve_external(Some(""), Some(""), "vi"), "vi");
    }

    #[test]
    fn editor_argv_handles_args_and_quoted_paths() {
        let l = i18n::Lang::En;
        assert_eq!(editor_argv(l, "vi").unwrap(), ["vi"]);
        assert_eq!(editor_argv(l, "code --wait").unwrap(), ["code", "--wait"]);
        // quote 済みのスペース入りパスは 1 引数として保たれる
        assert_eq!(
            editor_argv(l, "'/My Apps/subl' -w").unwrap(),
            ["/My Apps/subl", "-w"]
        );
        assert!(editor_argv(l, "").is_err());
    }
}
