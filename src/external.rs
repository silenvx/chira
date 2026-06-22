use std::env;
use std::io;
use std::path::Path;
use std::process::{Command, ExitStatus};

use crate::i18n::{self, Lang};

/// $EDITOR / $SHELL / config 値を shell の語分割規則 (shell-words) で argv に分解する。
/// 引数付き (`code --wait` / `zsh -l`) と quote 済みスペース入りパス (`'/My Apps/subl' -w`) の両方を扱う (whitespace split は後者を壊す)。
pub fn command_argv(lang: Lang, command: &str) -> io::Result<Vec<String>> {
    let argv = shell_words::split(command).map_err(|e| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            i18n::err_command_parse(lang, &e),
        )
    })?;
    if argv.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            i18n::err_command_empty(lang),
        ));
    }
    Ok(argv)
}

pub fn spawn_editor(
    lang: Lang,
    path: &Path,
    config_editor: Option<&str>,
) -> io::Result<ExitStatus> {
    let env_editor = env::var("EDITOR").ok();
    let editor = resolve_external(env_editor.as_deref(), config_editor, "vi");
    let argv = command_argv(lang, &editor)?;
    Command::new(&argv[0]).args(&argv[1..]).arg(path).status()
}

pub fn spawn_shell(lang: Lang, dir: &Path, config_shell: Option<&str>) -> io::Result<ExitStatus> {
    let env_shell = env::var("SHELL").ok();
    let shell = resolve_external(env_shell.as_deref(), config_shell, "/bin/sh");
    // editor と同じく引数付き ($SHELL="zsh -l" や config shell="bash -l") を許容する
    let argv = command_argv(lang, &shell)?;
    Command::new(&argv[0])
        .args(&argv[1..])
        .current_dir(dir)
        .status()
}

/// 外部プロセス名の解決: env > config > ハードコード default。空文字は未設定扱い。
pub fn resolve_external(env_val: Option<&str>, config_val: Option<&str>, default: &str) -> String {
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
    fn command_argv_handles_args_and_quoted_paths() {
        let l = Lang::En;
        assert_eq!(command_argv(l, "vi").unwrap(), ["vi"]);
        assert_eq!(command_argv(l, "code --wait").unwrap(), ["code", "--wait"]);
        // shell も同じ分割を通すため引数付き shell が argv に分かれる
        assert_eq!(command_argv(l, "zsh -l").unwrap(), ["zsh", "-l"]);
        // quote 済みのスペース入りパスは 1 引数として保たれる
        assert_eq!(
            command_argv(l, "'/My Apps/subl' -w").unwrap(),
            ["/My Apps/subl", "-w"]
        );
        assert!(command_argv(l, "").is_err());
    }
}
