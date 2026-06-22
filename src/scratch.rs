use std::env;
use std::fs;
use std::io;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

#[derive(Clone)]
pub struct Entry {
    pub path: PathBuf,
    pub name: String,
    pub is_dir: bool,
    pub modified: SystemTime,
}

/// scratch のルート: $CHIRA_DIR → config の dir → $XDG_DATA_HOME/chira → ~/.local/share/chira。
/// env > config の順は、既存の `CHIRA_DIR=... chira` を config 導入後も優先させるため。
/// macOS でも Apple の Application Support ではなく XDG 流に寄せ、ターミナルから扱いやすくする。
pub fn root(config_dir: Option<&str>) -> io::Result<PathBuf> {
    let home = env_path("HOME");
    let dir = resolve_root(
        env_path("CHIRA_DIR"),
        config_dir,
        env_path("XDG_DATA_HOME"),
        home.as_deref(),
    )?;
    fs::create_dir_all(&dir)?;
    Ok(dir)
}

fn resolve_root(
    chira_dir: Option<PathBuf>,
    config_dir: Option<&str>,
    xdg_data: Option<PathBuf>,
    home: Option<&Path>,
) -> io::Result<PathBuf> {
    if let Some(d) = chira_dir {
        Ok(d)
    } else if let Some(d) = config_dir.filter(|s| !s.is_empty()) {
        expand_tilde(d, home)
    } else if let Some(d) = xdg_data {
        Ok(d.join("chira"))
    } else {
        Ok(require_home(home)?.join(".local/share/chira"))
    }
}

/// config 由来のパスの先頭 `~` を $HOME へ展開する (env 由来のパスは shell が展開済み)。
fn expand_tilde(s: &str, home: Option<&Path>) -> io::Result<PathBuf> {
    if s == "~" {
        Ok(require_home(home)?.to_path_buf())
    } else if let Some(rest) = s.strip_prefix("~/") {
        Ok(require_home(home)?.join(rest))
    } else {
        Ok(PathBuf::from(s))
    }
}

fn require_home(home: Option<&Path>) -> io::Result<&Path> {
    home.ok_or_else(|| io::Error::other("HOME is not set"))
}

/// 環境変数を非空のパスとして取得する (非 UTF-8 値も保持。空文字は未設定扱い)。
pub(crate) fn env_path(key: &str) -> Option<PathBuf> {
    env::var_os(key)
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
}

/// dir 直下のエントリを返す (隠しファイルは除外、未ソート)。
fn read_entries(dir: &Path) -> io::Result<Vec<Entry>> {
    let mut entries = Vec::new();
    for dirent in fs::read_dir(dir)? {
        let dirent = dirent?;
        let name = dirent.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        // 壊れた symlink 等で metadata が失敗しても listing 全体を巻き込まないよう
        // エントリ単位でフォールバックする (`?` で一覧ごと空にしない)
        let meta = dirent.metadata();
        let is_dir = meta.as_ref().map(|m| m.is_dir()).unwrap_or(false);
        let modified = meta
            .and_then(|m| m.modified())
            .unwrap_or(SystemTime::UNIX_EPOCH);
        entries.push(Entry {
            path: dirent.path(),
            name,
            is_dir,
            modified,
        });
    }
    Ok(entries)
}

/// dir 直下のエントリを更新が新しい順に返す (一覧表示用)。
pub fn list(dir: &Path) -> io::Result<Vec<Entry>> {
    let mut entries = read_entries(dir)?;
    entries.sort_by_key(|e| std::cmp::Reverse(e.modified));
    Ok(entries)
}

/// dir をディレクトリ優先・名前順に並べたエントリ (tree 表示用)。
fn children_by_name(dir: &Path) -> io::Result<Vec<Entry>> {
    let mut entries = read_entries(dir)?;
    entries.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then_with(|| a.name.cmp(&b.name)));
    Ok(entries)
}

/// dir 配下を `tree` 風に描画する。深さ・行数を制限して巨大ディレクトリでも軽量に保つ。
pub fn tree(lang: crate::i18n::Lang, dir: &Path, max_depth: usize, max_lines: usize) -> String {
    let mut out = String::new();
    let mut lines = 0usize;
    build_tree(lang, dir, "", max_depth, max_lines, &mut lines, &mut out);
    if out.is_empty() {
        crate::i18n::empty_directory(lang).into()
    } else {
        out.truncate(out.trim_end().len());
        out
    }
}

fn build_tree(
    lang: crate::i18n::Lang,
    dir: &Path,
    prefix: &str,
    depth_left: usize,
    max_lines: usize,
    lines: &mut usize,
    out: &mut String,
) {
    let entries = match children_by_name(dir) {
        Ok(e) => e,
        Err(e) => {
            out.push_str(&format!(
                "{prefix}{}\n",
                crate::i18n::err_unreadable(lang, &e)
            ));
            return;
        }
    };
    let last_idx = entries.len().saturating_sub(1);
    for (i, entry) in entries.iter().enumerate() {
        if *lines >= max_lines {
            out.push_str(&format!("{prefix}…\n"));
            return;
        }
        let is_last = i == last_idx;
        let connector = if is_last { "└── " } else { "├── " };
        let suffix = if entry.is_dir { "/" } else { "" };
        out.push_str(&format!("{prefix}{connector}{}{suffix}\n", entry.name));
        *lines += 1;
        if entry.is_dir && depth_left > 1 {
            // symlink-to-dir には recurse しない (in-root symlink が外部 dir を指す場合、
            // tree が CHIRA_DIR 外の内容を露出させる経路を塞ぐ。symlink 自身は前行で表示済み)
            let is_symlink = entry
                .path
                .symlink_metadata()
                .map(|m| m.file_type().is_symlink())
                .unwrap_or(false);
            if is_symlink {
                continue;
            }
            let child_prefix = format!("{prefix}{}", if is_last { "    " } else { "│   " });
            build_tree(
                lang,
                &entry.path,
                &child_prefix,
                depth_left - 1,
                max_lines,
                lines,
                out,
            );
        }
    }
}

/// name の安全性。空・`/` 含み・先頭 `.` を拒否する。
/// 先頭 `.` 拒否は隠しファイル一覧除外との整合で、作成できるが一覧に出ないゴースト化も防ぐ。
fn validate_name(name: &str) -> io::Result<()> {
    if name.is_empty() || name.contains('/') || name.starts_with('.') {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "invalid name"));
    }
    Ok(())
}

pub fn create_file(dir: &Path, name: &str) -> io::Result<PathBuf> {
    validate_name(name)?;
    let path = dir.join(name);
    // 既存ファイルを上書きしないよう create_new で作る
    fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)?;
    Ok(path)
}

pub fn create_dir(dir: &Path, name: &str) -> io::Result<PathBuf> {
    validate_name(name)?;
    let path = dir.join(name);
    fs::create_dir(&path)?;
    Ok(path)
}

pub fn rename(entry: &Entry, new_name: &str) -> io::Result<PathBuf> {
    validate_name(new_name)?;
    let parent = entry
        .path
        .parent()
        .ok_or_else(|| io::Error::other("could not determine parent directory"))?;
    let dest = parent.join(new_name);
    if dest == entry.path {
        return Ok(dest); // 同名への改名は no-op
    }
    // 既存 directory entry (壊れた/別実体への symlink 含む) の無確認上書きを防ぐ。symlink を辿らない
    // lstat の dev+ino 比較なので、同一エントリへの case 変更 (note.md→Note.md) だけ許可し別 entry は衝突。
    if let Ok(dest_meta) = dest.symlink_metadata() {
        let same_entry = entry
            .path
            .symlink_metadata()
            .map(|m| (m.dev(), m.ino()) == (dest_meta.dev(), dest_meta.ino()))
            .unwrap_or(false);
        if !same_entry {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "an entry with the same name already exists",
            ));
        }
    }
    fs::rename(&entry.path, &dest)?;
    Ok(dest)
}

pub fn remove(entry: &Entry) -> io::Result<()> {
    if entry.is_dir {
        fs::remove_dir_all(&entry.path)
    } else {
        fs::remove_file(&entry.path)
    }
}

pub fn read_text(path: &Path) -> io::Result<String> {
    fs::read_to_string(path)
}

/// path を canonicalize して root 配下にあることを保証する。
/// 全 CLI subcommand の path 引数を root に拘束するガード (`..` や symlink 経由の escape を止める)。
/// path 自身を canonicalize するため target が存在しない / broken symlink だと IO エラーになる。
/// rm/mv のように symlink 自身を操作対象にしたい経路は `ensure_path_under_root` を使う。
pub fn ensure_under_root(root: &Path, path: &Path) -> io::Result<PathBuf> {
    let canonical_root = root.canonicalize()?;
    let canonical_target = path.canonicalize()?;
    if !canonical_target.starts_with(&canonical_root) {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "path escapes scratch root",
        ));
    }
    Ok(canonical_target)
}

/// path 自身は canonicalize せず、parent dir だけ canonicalize して root 配下を確認する。
/// `ensure_under_root` と異なり broken symlink (target 不在) も pass する (entry 自体の `symlink_metadata` で存在検査)。
/// rm/mv で symlink を unix 慣習どおり「symlink 自体」を対象にするための lexical path 用ガード。
pub fn ensure_path_under_root(root: &Path, path: &Path) -> io::Result<()> {
    let canonical_root = root.canonicalize()?;
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::other("path has no parent"))?;
    let canonical_parent = parent.canonicalize()?;
    if !canonical_parent.starts_with(&canonical_root) {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "path escapes scratch root",
        ));
    }
    path.symlink_metadata()?;
    Ok(())
}

/// 既存パスから Entry を組み立てる (CLI から rename / remove を呼ぶための adapter)。
/// is_dir は symlink を辿らず lstat で判定し、broken symlink を file 扱いで listing と整合させる。
pub fn entry_from_path(path: &Path) -> io::Result<Entry> {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let meta = path.symlink_metadata()?;
    let modified = meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);
    Ok(Entry {
        path: path.to_path_buf(),
        name,
        is_dir: meta.is_dir(),
        modified,
    })
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::time::Duration;

    use super::*;

    /// テスト用の一意なディレクトリを作る (並列テストでの衝突回避)
    fn temp_root() -> PathBuf {
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = env::temp_dir().join(format!("chira-test-{}-{}", std::process::id(), n));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn resolve_root_precedence() {
        let home = PathBuf::from("/home/u");
        // env CHIRA_DIR は config より優先される (AC: env > config)
        assert_eq!(
            resolve_root(
                Some(PathBuf::from("/env/dir")),
                Some("/cfg/dir"),
                Some(PathBuf::from("/xdg")),
                Some(&home),
            )
            .unwrap(),
            PathBuf::from("/env/dir")
        );
        // env 無し → config の dir を使い、先頭 ~ は $HOME へ展開する
        assert_eq!(
            resolve_root(
                None,
                Some("~/scratch"),
                Some(PathBuf::from("/xdg")),
                Some(&home)
            )
            .unwrap(),
            PathBuf::from("/home/u/scratch")
        );
        // config の絶対パスはそのまま
        assert_eq!(
            resolve_root(None, Some("/abs/dir"), None, Some(&home)).unwrap(),
            PathBuf::from("/abs/dir")
        );
        // env / config 無し → XDG_DATA_HOME/chira
        assert_eq!(
            resolve_root(None, None, Some(PathBuf::from("/xdg")), Some(&home)).unwrap(),
            PathBuf::from("/xdg/chira")
        );
        // 空文字の config dir は未設定扱いで次の候補へ送る
        assert_eq!(
            resolve_root(None, Some(""), Some(PathBuf::from("/xdg")), Some(&home)).unwrap(),
            PathBuf::from("/xdg/chira")
        );
        // すべて無し → ~/.local/share/chira
        assert_eq!(
            resolve_root(None, None, None, Some(&home)).unwrap(),
            PathBuf::from("/home/u/.local/share/chira")
        );
    }

    #[test]
    fn resolve_root_tilde_needs_home() {
        // config dir が ~ 始まりで $HOME 不明なときはエラー
        assert!(resolve_root(None, Some("~/scratch"), None, None).is_err());
        // ~ を含まない config dir は $HOME 不要
        assert_eq!(
            resolve_root(None, Some("/abs/dir"), None, None).unwrap(),
            PathBuf::from("/abs/dir")
        );
    }

    #[test]
    fn validate_rejects_traversal() {
        assert!(validate_name("").is_err());
        assert!(validate_name("a/b").is_err());
        assert!(validate_name("..").is_err());
        assert!(validate_name(".").is_err());
        // 先頭ドット名は read_entries の隠しファイル除外と整合させ拒否する
        assert!(validate_name(".hidden").is_err());
        assert!(validate_name(".notes.md").is_err());
        assert!(validate_name("ok.md").is_ok());
        assert!(validate_name("日本語メモ").is_ok());
    }

    #[test]
    fn rename_rejects_existing_dest() {
        let root = temp_root();
        create_file(&root, "a.md").unwrap();
        create_file(&root, "b.md").unwrap();
        let a = list(&root)
            .unwrap()
            .into_iter()
            .find(|e| e.name == "a.md")
            .unwrap();
        // 既存の b.md を無確認上書きせずエラーにする
        assert!(rename(&a, "b.md").is_err());
        assert!(root.join("a.md").exists());
        // 同名への改名は no-op で成功する
        assert!(rename(&a, "a.md").is_ok());
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn rename_rejects_broken_symlink_dest() {
        let root = temp_root();
        create_file(&root, "f.md").unwrap();
        std::os::unix::fs::symlink("/nonexistent/x", root.join("broken")).unwrap();
        let f = list(&root)
            .unwrap()
            .into_iter()
            .find(|e| e.name == "f.md")
            .unwrap();
        // 壊れた symlink も一覧に出る既存エントリなので無確認上書きしない (exists() はすり抜ける)
        assert!(rename(&f, "broken").is_err());
        assert!(root.join("f.md").exists());
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn rename_rejects_symlink_to_source_dest() {
        let root = temp_root();
        create_file(&root, "a.md").unwrap();
        // a.md を指す (非 broken) symlink。canonicalize なら同一実体だが別 directory entry
        std::os::unix::fs::symlink(root.join("a.md"), root.join("alias")).unwrap();
        let a = list(&root)
            .unwrap()
            .into_iter()
            .find(|e| e.name == "a.md")
            .unwrap();
        // a.md→alias は別エントリ (symlink) の上書きなので拒否し、a.md を消さない
        assert!(rename(&a, "alias").is_err());
        assert!(root.join("a.md").exists());
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn list_survives_broken_symlink() {
        let root = temp_root();
        create_file(&root, "ok.md").unwrap();
        std::os::unix::fs::symlink("/nonexistent/target", root.join("broken")).unwrap();
        // 壊れた symlink があっても一覧全体が空にならず、両エントリが出る
        let listed = list(&root).unwrap();
        assert_eq!(listed.len(), 2);
        assert!(listed.iter().any(|e| e.name == "ok.md"));
        assert!(listed.iter().any(|e| e.name == "broken" && !e.is_dir));
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn create_list_rename_remove() {
        let root = temp_root();
        let f = create_file(&root, "memo.md").unwrap();
        fs::write(&f, "本文").unwrap();
        let d = create_dir(&root, "ws").unwrap();

        let listed = list(&root).unwrap();
        assert_eq!(listed.len(), 2);
        assert!(listed.iter().any(|e| e.name == "memo.md" && !e.is_dir));
        assert!(listed.iter().any(|e| e.name == "ws" && e.is_dir));

        let file_entry = listed.into_iter().find(|e| e.name == "memo.md").unwrap();
        rename(&file_entry, "renamed.md").unwrap();
        assert!(root.join("renamed.md").exists());
        assert!(!root.join("memo.md").exists());
        assert_eq!(read_text(&root.join("renamed.md")).unwrap(), "本文");

        let dir_entry = list(&root).unwrap().into_iter().find(|e| e.is_dir).unwrap();
        remove(&dir_entry).unwrap();
        assert!(!d.exists());

        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn create_file_rejects_duplicate() {
        let root = temp_root();
        create_file(&root, "a.md").unwrap();
        assert!(create_file(&root, "a.md").is_err());
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn tree_renders_nested_structure() {
        let root = temp_root();
        let ws = create_dir(&root, "ws").unwrap();
        create_file(&ws, "a.txt").unwrap();
        create_file(&root, "b.md").unwrap();

        let t = tree(crate::i18n::Lang::En, &root, 4, 100);
        // ディレクトリ優先 + 名前順で ws/ が先、ネストした a.txt が枝付きで出る
        assert!(t.contains("├── ws/"), "tree:\n{t}");
        assert!(t.contains("│   └── a.txt"), "tree:\n{t}");
        assert!(t.contains("└── b.md"), "tree:\n{t}");

        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn tree_does_not_recurse_into_symlinked_directory() {
        let root = temp_root();
        // 外部 dir に "secret" を置き、root から symlink で参照させる。
        // tree が symlink を辿ると secret が出力に混入してしまう (security risk)
        let outside = env::temp_dir().join(format!("chira-tree-outside-{}", std::process::id()));
        fs::create_dir_all(&outside).unwrap();
        fs::write(outside.join("secret.txt"), b"do-not-leak").unwrap();
        std::os::unix::fs::symlink(&outside, root.join("escape")).unwrap();
        create_file(&root, "ok.md").unwrap();

        let t = tree(crate::i18n::Lang::En, &root, 4, 100);
        // symlink 自身 (`escape/`) は表示してよいが中身 (`secret.txt`) は出力に出てはならない
        assert!(
            t.contains("escape"),
            "tree should still show the symlink entry:\n{t}"
        );
        assert!(t.contains("ok.md"), "regular entry should appear:\n{t}");
        assert!(
            !t.contains("secret.txt"),
            "tree must not recurse into in-root symlinks:\n{t}"
        );
        fs::remove_dir_all(&root).unwrap();
        fs::remove_dir_all(&outside).unwrap();
    }

    #[test]
    fn tree_respects_line_cap() {
        let root = temp_root();
        for i in 0..10 {
            create_file(&root, &format!("f{i:02}.md")).unwrap();
        }
        let t = tree(crate::i18n::Lang::En, &root, 4, 3);
        assert!(t.contains('…'), "line cap should emit ellipsis: {t}");
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn list_excludes_hidden_and_sorts_newest_first() {
        let root = temp_root();
        // 隠しファイルは validate_name が作成を拒否するため、外部作成を模して fs で直接置く
        fs::write(root.join(".hidden"), b"").unwrap();
        let old = create_file(&root, "old.md").unwrap();
        // old を確実に過去の mtime にしてから new を作る
        std::thread::sleep(Duration::from_millis(10));
        create_file(&root, "new.md").unwrap();

        let listed = list(&root).unwrap();
        assert_eq!(listed.len(), 2, "隠しファイルは除外される");
        assert_eq!(listed[0].name, "new.md", "更新が新しい順");
        assert_eq!(listed[1].name, "old.md");
        assert!(old.exists());

        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn ensure_under_root_accepts_nested_path() {
        let root = temp_root();
        let nested = create_dir(&root, "ws").unwrap();
        create_file(&nested, "inner.md").unwrap();
        // root 配下のネストパスは通る
        assert!(ensure_under_root(&root, &nested).is_ok());
        assert!(ensure_under_root(&root, &nested.join("inner.md")).is_ok());
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn ensure_under_root_rejects_symlink_escape() {
        let root = temp_root();
        // 外部ディレクトリを root 配下の symlink で参照しても canonicalize で見抜く
        let outside = env::temp_dir().join(format!("chira-outside-{}", std::process::id()));
        fs::create_dir_all(&outside).unwrap();
        std::os::unix::fs::symlink(&outside, root.join("escape")).unwrap();
        assert!(ensure_under_root(&root, &root.join("escape")).is_err());
        fs::remove_dir_all(&root).unwrap();
        fs::remove_dir_all(&outside).unwrap();
    }

    #[test]
    fn entry_from_path_reports_dir_and_file() {
        let root = temp_root();
        let dir = create_dir(&root, "ws").unwrap();
        let file = create_file(&root, "note.md").unwrap();

        let e_dir = entry_from_path(&dir).unwrap();
        assert!(e_dir.is_dir);
        assert_eq!(e_dir.name, "ws");

        let e_file = entry_from_path(&file).unwrap();
        assert!(!e_file.is_dir);
        assert_eq!(e_file.name, "note.md");

        // 不在パスはエラー
        assert!(entry_from_path(&root.join("missing")).is_err());

        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn ensure_path_under_root_accepts_broken_symlink() {
        let root = temp_root();
        // broken symlink は ensure_under_root では canonicalize に失敗するが、
        // ensure_path_under_root では parent 検証 + symlink_metadata で受理する
        std::os::unix::fs::symlink("/nonexistent/target", root.join("broken")).unwrap();
        assert!(ensure_under_root(&root, &root.join("broken")).is_err());
        assert!(ensure_path_under_root(&root, &root.join("broken")).is_ok());
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn ensure_path_under_root_rejects_escape() {
        let root = temp_root();
        create_file(&root, "note.md").unwrap();
        assert!(ensure_path_under_root(&root, &root.join("note.md")).is_ok());
        let outside = env::temp_dir().join(format!("chira-outside-eppur-{}", std::process::id()));
        fs::create_dir_all(&outside).unwrap();
        std::os::unix::fs::symlink(&outside, root.join("escape")).unwrap();
        // entry 自体が root にある in-root symlink は target が外部でも OK (link target の安全は rm/edit 側責任)
        assert!(ensure_path_under_root(&root, &root.join("escape")).is_ok());
        // root 外の path 自体は parent canonicalize で reject
        assert!(ensure_path_under_root(&root, &outside.join("foo")).is_err());
        fs::remove_dir_all(&root).unwrap();
        fs::remove_dir_all(&outside).unwrap();
    }
}
