use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

#[derive(Clone)]
pub struct Entry {
    pub path: PathBuf,
    pub name: String,
    pub is_dir: bool,
    pub modified: SystemTime,
}

/// scratch のルート: $SCRAP_DIR → $XDG_DATA_HOME/scrap → ~/.local/share/scrap。
/// macOS でも Apple の Application Support ではなく XDG 流に寄せ、ターミナルから扱いやすくする。
pub fn root() -> io::Result<PathBuf> {
    let dir = if let Some(d) = env_path("SCRAP_DIR") {
        d
    } else if let Some(d) = env_path("XDG_DATA_HOME") {
        d.join("scrap")
    } else {
        home()?.join(".local/share/scrap")
    };
    fs::create_dir_all(&dir)?;
    Ok(dir)
}

fn env_path(key: &str) -> Option<PathBuf> {
    env::var_os(key)
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
}

fn home() -> io::Result<PathBuf> {
    env_path("HOME").ok_or_else(|| io::Error::other("HOME が設定されていません"))
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
    entries.sort_by(|a, b| b.modified.cmp(&a.modified));
    Ok(entries)
}

/// dir をディレクトリ優先・名前順に並べたエントリ (tree 表示用)。
fn children_by_name(dir: &Path) -> io::Result<Vec<Entry>> {
    let mut entries = read_entries(dir)?;
    entries.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then_with(|| a.name.cmp(&b.name)));
    Ok(entries)
}

/// dir 配下を `tree` 風に描画する。深さ・行数を制限して巨大ディレクトリでも軽量に保つ。
pub fn tree(dir: &Path, max_depth: usize, max_lines: usize) -> String {
    let mut out = String::new();
    let mut lines = 0usize;
    build_tree(dir, "", max_depth, max_lines, &mut lines, &mut out);
    if out.is_empty() {
        "(空のディレクトリ)".into()
    } else {
        out.truncate(out.trim_end().len());
        out
    }
}

fn build_tree(
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
            out.push_str(&format!("{prefix}(読み取り不可: {e})\n"));
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
            let child_prefix = format!("{prefix}{}", if is_last { "    " } else { "│   " });
            build_tree(&entry.path, &child_prefix, depth_left - 1, max_lines, lines, out);
        }
    }
}

/// name の安全性。空・`/` 含み・先頭 `.` を拒否する。
/// 先頭 `.` 拒否は隠しファイル一覧除外との整合で、作成できるが一覧に出ないゴースト化も防ぐ。
fn validate_name(name: &str) -> io::Result<()> {
    if name.is_empty() || name.contains('/') || name.starts_with('.') {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "不正な名前です",
        ));
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
        .ok_or_else(|| io::Error::other("親ディレクトリを特定できません"))?;
    let dest = parent.join(new_name);
    if dest == entry.path {
        return Ok(dest); // 同名への改名は no-op
    }
    // 既存エントリを無確認で上書きしないよう存在チェックする
    if dest.exists() {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "同名のエントリが既に存在します",
        ));
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

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::time::Duration;

    use super::*;

    /// テスト用の一意なディレクトリを作る (並列テストでの衝突回避)
    fn temp_root() -> PathBuf {
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = env::temp_dir().join(format!("scrap-test-{}-{}", std::process::id(), n));
        fs::create_dir_all(&dir).unwrap();
        dir
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
        let a = list(&root).unwrap().into_iter().find(|e| e.name == "a.md").unwrap();
        // 既存の b.md を無確認上書きせずエラーにする
        assert!(rename(&a, "b.md").is_err());
        assert!(root.join("a.md").exists());
        // 同名への改名は no-op で成功する
        assert!(rename(&a, "a.md").is_ok());
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

        let t = tree(&root, 4, 100);
        // ディレクトリ優先 + 名前順で ws/ が先、ネストした a.txt が枝付きで出る
        assert!(t.contains("├── ws/"), "tree:\n{t}");
        assert!(t.contains("│   └── a.txt"), "tree:\n{t}");
        assert!(t.contains("└── b.md"), "tree:\n{t}");

        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn tree_respects_line_cap() {
        let root = temp_root();
        for i in 0..10 {
            create_file(&root, &format!("f{i:02}.md")).unwrap();
        }
        let t = tree(&root, 4, 3);
        assert!(t.contains('…'), "行数上限で省略マークが出る: {t}");
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
}
