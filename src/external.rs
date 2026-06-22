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

/// アクションの `run` を新ディレクトリ内で `sh -c` 実行する。
/// editor/shell と違い `run` は `&&` / `|` / `~` / `$VAR` を含む shell コマンドラインなので、
/// argv 分割 (command_argv) ではなく shell に解釈させる。CHIRA_* 環境変数と cwd を渡す。
pub fn spawn_run(dir: &Path, root: &Path, command: &str) -> io::Result<ExitStatus> {
    let name = dir
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    Command::new("/bin/sh")
        .arg("-c")
        .arg(command)
        .current_dir(dir)
        .env("CHIRA_TARGET", dir)
        .env("CHIRA_TARGET_NAME", name)
        .env("CHIRA_ROOT", root)
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

/// ExitStatus を chira 全体の exit code 規約 (success=0, failure=非ゼロ) へマップする。
/// 通常終了は `code()` をそのまま返し、シグナル終了 (Ctrl+C 等で `code()` が `None`) は
/// unix 慣習に揃って 128 + signal を返す (script 連携で「成功」扱いされる事故を防ぐ)。
pub fn exit_code_from_status(status: ExitStatus) -> i32 {
    if let Some(code) = status.code() {
        return code;
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        if let Some(signal) = status.signal() {
            return 128 + signal;
        }
    }
    1
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

    #[test]
    #[cfg(unix)]
    fn spawn_run_executes_in_dir_with_env() {
        use std::sync::atomic::{AtomicU32, Ordering};
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("chira-run-{}-{}", std::process::id(), n));
        std::fs::create_dir_all(&dir).unwrap();
        let root = dir.parent().unwrap();

        // && チェイン + 3 つの CHIRA_* env 展開 + cwd 解決を一度に検証する
        let status = spawn_run(
            &dir,
            root,
            "printf '%s\\n%s\\n%s\\n' \"$CHIRA_TARGET_NAME\" \"$CHIRA_TARGET\" \"$CHIRA_ROOT\" > out.txt",
        )
        .unwrap();
        assert!(status.success());

        let content = std::fs::read_to_string(dir.join("out.txt")).unwrap();
        let name = dir.file_name().unwrap().to_string_lossy();
        let mut lines = content.lines();
        assert_eq!(lines.next().unwrap(), name);
        assert_eq!(lines.next().unwrap(), dir.to_str().unwrap());
        assert_eq!(lines.next().unwrap(), root.to_str().unwrap());

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    #[cfg(unix)]
    fn exit_code_from_status_handles_signal_termination() {
        use std::os::unix::process::ExitStatusExt;
        // 通常終了 (exit 7) → 7
        let normal = ExitStatus::from_raw(7 << 8);
        assert_eq!(exit_code_from_status(normal), 7);
        // SIGINT (signal 2) で kill → 128 + 2 = 130 (Ctrl+C で正常終了扱いになる事故を防ぐ)
        let killed = ExitStatus::from_raw(2);
        assert_eq!(exit_code_from_status(killed), 130);
    }
}
