use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::i18n::{self, Lang};
use crate::scratch;

/// archive 先のデフォルトディレクトリ名 (CHIRA_DIR 直下)。
/// `.` 始まりは scratch::list の隠しエントリ除外で一覧から外れる。
pub const DEFAULT_ARCHIVE_DIRNAME: &str = ".archive";

/// `.keep` マーカー (ディレクトリ直下にあると archive 対象外)。lf / nnn 慣習を踏襲。
pub const KEEP_MARKER: &str = ".keep";

/// 1 件の archive 結果。dry-run 時の表示と実行後の status 集計に共通で使う。
#[derive(Debug, Clone)]
pub struct Outcome {
    pub name: String,
    pub dest: PathBuf,
}

/// sweep の集計。Display は呼び出し側 (main) で i18n に通す。
#[derive(Debug, Default)]
pub struct Report {
    pub archived: Vec<Outcome>,
    pub kept: usize,
    pub errors: Vec<String>,
}

/// sweep の入力パラメータ。CLI / config から組み立てる。
pub struct Options<'a> {
    pub root: &'a Path,
    pub archive_dir: PathBuf,
    pub ttl: Duration,
    pub keep_patterns: &'a [String],
    pub dry_run: bool,
    pub now: SystemTime,
}

/// TTL を超えたエントリを archive_dir へ move する。
/// errors にはスキップ理由 (mtime 取得不可・rename 失敗等) を i18n 済み文字列で積む。
pub fn sweep(lang: Lang, opts: Options<'_>) -> io::Result<Report> {
    // archive_dir が root と同一/祖先のとき全エントリが「archive 配下」扱いになり silent no-op
    // (`--archive-dir .` 等の設定ミス) を早期に検出してエラー化
    if let Some(err) = detect_archive_root_conflict(opts.root, &opts.archive_dir) {
        return Err(err);
    }
    let entries = scratch::list(opts.root)?;
    let mut report = Report::default();
    // archive_dir 自身を再帰対象から除く判定用。basename ではなく canonical path で
    // 比較するため、外部 archive_dir と root 直下の同名エントリが衝突しない
    let archive_canonical = opts.archive_dir.canonicalize().ok();
    // 同一 sweep で複数回 create_dir_all を呼ばずに済むようフラグ化 (syscall 削減)
    let mut archive_dir_ensured = false;

    for entry in entries {
        if is_under_archive(&entry.path, &opts.archive_dir, archive_canonical.as_deref()) {
            report.kept += 1;
            continue;
        }
        if is_keep_match(opts.keep_patterns, &entry.name, entry.is_dir) {
            report.kept += 1;
            continue;
        }
        // symlink_metadata でエントリ自身の mtime を取る (metadata() の follow ではリンク先の mtime になり寿命判定が崩れる)
        if entry.path.is_symlink() && !entry.path.exists() {
            let broken_err = io::Error::new(io::ErrorKind::NotFound, "broken symlink");
            report
                .errors
                .push(i18n::warn_archive_mtime(lang, &entry.name, &broken_err));
            continue;
        }
        let modified = match entry.path.symlink_metadata().and_then(|m| m.modified()) {
            Ok(m) => m,
            Err(e) => {
                report
                    .errors
                    .push(i18n::warn_archive_mtime(lang, &entry.name, &e));
                continue;
            }
        };
        let elapsed = match opts.now.duration_since(modified) {
            Ok(d) => d,
            // 未来 mtime (clock skew 等) は old とみなさない
            Err(_) => {
                report.kept += 1;
                continue;
            }
        };
        if elapsed <= opts.ttl {
            report.kept += 1;
            continue;
        }
        // .keep probe は TTL gate の後に実行する。TTL 内の fresh dir で permission deny の
        // probe が走ると不要な errors push + exit 1 になり cron/startup sweep を壊すため。
        // 実 dir のみ probe (symlink-to-dir は entry.is_dir = true でも対象外: target の .keep を follow すると
        // 外部 dir の .keep で symlink 自身が誤って保護されるため)
        if entry.is_dir && !entry.path.is_symlink() {
            match entry.path.join(KEEP_MARKER).try_exists() {
                Ok(true) => {
                    report.kept += 1;
                    continue;
                }
                Ok(false) => {}
                Err(e) => {
                    report
                        .errors
                        .push(i18n::warn_archive_keep_probe(lang, &entry.name, &e));
                    continue;
                }
            }
        }

        if opts.dry_run {
            let dest = resolve_dest(&opts.archive_dir, &entry.name, opts.now);
            report.archived.push(Outcome {
                name: entry.name,
                dest,
            });
            continue;
        }
        if !archive_dir_ensured {
            if let Err(e) = ensure_dir(&opts.archive_dir) {
                report.errors.push(i18n::warn_archive_mkdir(
                    lang,
                    &opts.archive_dir.display(),
                    &e,
                ));
                // 1 件失敗で全体を止めず、後続も同じエラーで個別に積む
                continue;
            }
            archive_dir_ensured = true;
        }
        let dest = resolve_dest(&opts.archive_dir, &entry.name, opts.now);
        match fs::rename(&entry.path, &dest) {
            Ok(()) => report.archived.push(Outcome {
                name: entry.name,
                dest,
            }),
            Err(e) => report.errors.push(i18n::warn_archive_move(
                lang,
                &entry.name,
                &dest.display(),
                &e,
            )),
        }
    }
    Ok(report)
}

/// archive_dir が root と同一/祖先のとき sweep を abort するガード (canonical 比較、失敗時は
/// lexical normalize 後の path prefix fallback)。lexical normalize は `..` を解決して
/// `<root>/missing/..` を `<root>` と認識させる (canonicalize 失敗時の silent no-op 防止)
fn detect_archive_root_conflict(root: &Path, archive_dir: &Path) -> Option<io::Error> {
    let root_canon = root.canonicalize().ok();
    let arch_canon = archive_dir.canonicalize().ok();
    let conflict = match (root_canon.as_deref(), arch_canon.as_deref()) {
        (Some(r), Some(a)) => r == a || r.starts_with(a),
        _ => {
            let r = lexical_normalize(root);
            let a = lexical_normalize(archive_dir);
            r == a || r.starts_with(&a)
        }
    };
    if conflict {
        Some(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "archive_dir ({}) is the same as or an ancestor of root ({}); sweep would no-op",
                archive_dir.display(),
                root.display()
            ),
        ))
    } else {
        None
    }
}

/// `..` を lexical に解決 (symlink follow せず、純粋なパス component 処理)。
/// canonicalize 失敗時の fallback で「missing component + `..` で root を指す path」を見抜く
fn lexical_normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in path.components() {
        match c {
            std::path::Component::ParentDir => {
                out.pop();
            }
            std::path::Component::CurDir => {}
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// エントリが archive_dir 自身/その配下、または祖先かを判定する (祖先 = entry が archive_dir を内包する場合)。
/// 祖先扱いも skip するのは、root 直下の dir 配下に archive_dir がある構成で、その親 dir を archive すると
/// archive_dir も巻き込まれてしまう経路を防ぐため。canonical path 比較、失敗時は path prefix fallback
fn is_under_archive(entry: &Path, archive_dir: &Path, archive_canonical: Option<&Path>) -> bool {
    if let Some(canon) = archive_canonical
        && let Ok(entry_canon) = entry.canonicalize()
    {
        return entry_canon.starts_with(canon) || canon.starts_with(&entry_canon);
    }
    entry.starts_with(archive_dir) || archive_dir.starts_with(entry)
}

/// `<archive_dir>/<name>`、衝突時は `<name>.<unix_ts>` または `<name>.<unix_ts>_<N>` を付与。
/// suffix も衝突する場合 (異なる sweep 間の二重衝突) は連番でユニーク化する。
fn resolve_dest(archive_dir: &Path, name: &str, now: SystemTime) -> PathBuf {
    let primary = archive_dir.join(name);
    if !path_entry_exists(&primary) {
        return primary;
    }
    let ts = now
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let with_ts = archive_dir.join(format!("{name}.{ts}"));
    if !path_entry_exists(&with_ts) {
        return with_ts;
    }
    for counter in 1u32.. {
        let candidate = archive_dir.join(format!("{name}.{ts}_{counter}"));
        if !path_entry_exists(&candidate) {
            return candidate;
        }
    }
    // u32::MAX 件の衝突は事実上到達しないため、unreachable 相当
    archive_dir.join(format!("{name}.{ts}_overflow"))
}

/// Path::exists は dangling symlink (target が消えた link) を「存在しない」と判定するため、
/// resolve_dest で broken link を上書き対象とみなして既存 link を fs::rename で破壊しうる。
/// symlink_metadata でエントリ自身の有無を確認し、broken link も「存在する」として suffix 付与
fn path_entry_exists(path: &Path) -> bool {
    path.symlink_metadata().is_ok()
}

fn ensure_dir(path: &Path) -> io::Result<()> {
    fs::create_dir_all(path)
}

/// keep glob にマッチするか。末尾 `/` 付きパターンはディレクトリのみマッチ。
/// 末尾 `/` 抜きの `*` / `?` 展開はファイル/ディレクトリ問わずマッチ。
pub(crate) fn is_keep_match(patterns: &[String], name: &str, is_dir: bool) -> bool {
    patterns
        .iter()
        .any(|p| glob_match_one(p.as_str(), name, is_dir))
}

fn glob_match_one(pattern: &str, name: &str, is_dir: bool) -> bool {
    // 末尾 `/` は「ディレクトリのみ」を意味する慣習 (gitignore / `tree` 等と一致)
    if let Some(rest) = pattern.strip_suffix('/') {
        return is_dir && glob_match_raw(rest, name);
    }
    glob_match_raw(pattern, name)
}

/// `*` (任意の連続) と `?` (任意の 1 文字) のみサポートする最小 glob。
/// `[]` クラスや `**` は使わない (issue のサンプル `pinned-*` / `longterm/` で十分)。
fn glob_match_raw(pattern: &str, name: &str) -> bool {
    let p: Vec<char> = pattern.chars().collect();
    let n: Vec<char> = name.chars().collect();
    match_chars(&p, 0, &n, 0)
}

fn match_chars(p: &[char], pi: usize, n: &[char], ni: usize) -> bool {
    if pi == p.len() {
        return ni == n.len();
    }
    match p[pi] {
        '*' => {
            // 0 文字以上消費。1 段の greedy backtrack で十分 (短い pattern 前提)。
            if match_chars(p, pi + 1, n, ni) {
                return true;
            }
            if ni < n.len() {
                return match_chars(p, pi, n, ni + 1);
            }
            false
        }
        '?' => ni < n.len() && match_chars(p, pi + 1, n, ni + 1),
        c => ni < n.len() && n[ni] == c && match_chars(p, pi + 1, n, ni + 1),
    }
}

/// `30d` / `12h` / `45m` / `60s` / `2w` / 単位なし (= 秒) をパース。
/// 単位の大文字は受理。0 / 負値はエラー (誤って「全消し」を防ぐ)。
pub fn parse_duration(s: &str) -> Result<Duration, String> {
    let s = s.trim();
    if s.is_empty() {
        return Err("empty duration".into());
    }
    let pos = s.find(|c: char| !c.is_ascii_digit()).unwrap_or(s.len());
    let (num_str, unit) = s.split_at(pos);
    let n: u64 = num_str
        .parse()
        .map_err(|_| format!("invalid duration: {s}"))?;
    if n == 0 {
        return Err("duration must be > 0".into());
    }
    let secs = match unit.to_ascii_lowercase().as_str() {
        "" | "s" => n,
        "m" => n.checked_mul(60).ok_or("duration overflow")?,
        "h" => n.checked_mul(3600).ok_or("duration overflow")?,
        "d" => n.checked_mul(86_400).ok_or("duration overflow")?,
        "w" => n.checked_mul(604_800).ok_or("duration overflow")?,
        other => return Err(format!("unknown duration unit: {other}")),
    };
    Ok(Duration::from_secs(secs))
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::time::Duration;

    use super::*;

    fn temp_root() -> PathBuf {
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("chira-archive-{}-{}", std::process::id(), n));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn parse_duration_units() {
        assert_eq!(parse_duration("30").unwrap(), Duration::from_secs(30));
        assert_eq!(parse_duration("60s").unwrap(), Duration::from_secs(60));
        assert_eq!(parse_duration("45m").unwrap(), Duration::from_secs(2700));
        assert_eq!(parse_duration("12h").unwrap(), Duration::from_secs(43200));
        assert_eq!(
            parse_duration("30d").unwrap(),
            Duration::from_secs(2_592_000)
        );
        assert_eq!(
            parse_duration("2w").unwrap(),
            Duration::from_secs(1_209_600)
        );
        // 大文字単位も受理
        assert_eq!(
            parse_duration("30D").unwrap(),
            Duration::from_secs(2_592_000)
        );
        // 前後の空白を吸収
        assert_eq!(
            parse_duration("  30d  ").unwrap(),
            Duration::from_secs(2_592_000)
        );
    }

    #[test]
    fn parse_duration_rejects_zero_and_garbage() {
        assert!(parse_duration("").is_err());
        assert!(parse_duration("0").is_err());
        assert!(parse_duration("0d").is_err());
        assert!(parse_duration("abc").is_err());
        assert!(parse_duration("12x").is_err());
        // 浮動小数は受理しない (秒未満の TTL に意味がない)
        assert!(parse_duration("1.5d").is_err());
    }

    #[test]
    fn glob_match_basic() {
        assert!(glob_match_one("pinned-*", "pinned-2026", false));
        assert!(glob_match_one("pinned-*", "pinned-", false));
        assert!(!glob_match_one("pinned-*", "other", false));
        // 末尾 / はディレクトリ限定
        assert!(glob_match_one("longterm/", "longterm", true));
        assert!(!glob_match_one("longterm/", "longterm", false));
        // ? は 1 文字
        assert!(glob_match_one("a?c", "abc", false));
        assert!(!glob_match_one("a?c", "ac", false));
        // 完全一致
        assert!(glob_match_one("exact", "exact", false));
        assert!(!glob_match_one("exact", "exact2", false));
    }

    #[test]
    fn resolve_dest_collision_suffix() {
        let root = temp_root();
        let arch = root.join(".archive");
        fs::create_dir_all(&arch).unwrap();
        // 一意の dest
        let d1 = resolve_dest(&arch, "foo", SystemTime::UNIX_EPOCH);
        assert_eq!(d1, arch.join("foo"));
        // 衝突時は <name>.<ts>
        fs::write(&d1, b"").unwrap();
        let ts = SystemTime::UNIX_EPOCH + Duration::from_secs(1700000000);
        let d2 = resolve_dest(&arch, "foo", ts);
        assert_eq!(d2, arch.join("foo.1700000000"));
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn sweep_moves_old_entries() {
        let root = temp_root();
        // mtime 操作の代わりに `now` を未来にずらして TTL 超過を作る
        scratch::create_file(&root, "old.md").unwrap();
        scratch::create_dir(&root, "olddir").unwrap();
        let now = SystemTime::now() + Duration::from_secs(86_400 * 31);
        let report = sweep(
            Lang::En,
            Options {
                root: &root,
                archive_dir: root.join(".archive"),
                ttl: Duration::from_secs(86_400 * 30),
                keep_patterns: &[],
                dry_run: false,
                now,
            },
        )
        .unwrap();
        assert_eq!(report.archived.len(), 2, "report: {report:?}");
        assert!(report.errors.is_empty(), "errors: {:?}", report.errors);
        assert!(root.join(".archive/old.md").exists());
        assert!(root.join(".archive/olddir").is_dir());
        assert!(!root.join("old.md").exists());
        assert!(!root.join("olddir").exists());
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn sweep_keeps_fresh_entries() {
        let root = temp_root();
        scratch::create_file(&root, "fresh.md").unwrap();
        let now = SystemTime::now() + Duration::from_secs(1);
        let report = sweep(
            Lang::En,
            Options {
                root: &root,
                archive_dir: root.join(".archive"),
                ttl: Duration::from_secs(86_400 * 30),
                keep_patterns: &[],
                dry_run: false,
                now,
            },
        )
        .unwrap();
        assert!(report.archived.is_empty());
        assert!(root.join("fresh.md").exists());
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn sweep_dry_run_does_not_move() {
        let root = temp_root();
        scratch::create_file(&root, "old.md").unwrap();
        let now = SystemTime::now() + Duration::from_secs(86_400 * 31);
        let report = sweep(
            Lang::En,
            Options {
                root: &root,
                archive_dir: root.join(".archive"),
                ttl: Duration::from_secs(86_400 * 30),
                keep_patterns: &[],
                dry_run: true,
                now,
            },
        )
        .unwrap();
        assert_eq!(report.archived.len(), 1);
        assert!(root.join("old.md").exists(), "dry-run should not move");
        assert!(!root.join(".archive/old.md").exists());
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn sweep_respects_keep_marker_in_dir() {
        let root = temp_root();
        let d = scratch::create_dir(&root, "olddir").unwrap();
        // .keep を内部に置いたディレクトリは TTL 超過でも対象外
        fs::write(d.join(KEEP_MARKER), b"").unwrap();
        let now = SystemTime::now() + Duration::from_secs(86_400 * 31);
        let report = sweep(
            Lang::En,
            Options {
                root: &root,
                archive_dir: root.join(".archive"),
                ttl: Duration::from_secs(86_400 * 30),
                keep_patterns: &[],
                dry_run: false,
                now,
            },
        )
        .unwrap();
        assert!(report.archived.is_empty());
        assert!(root.join("olddir").exists());
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn sweep_respects_keep_glob() {
        let root = temp_root();
        scratch::create_file(&root, "pinned-todo.md").unwrap();
        scratch::create_dir(&root, "longterm").unwrap();
        scratch::create_file(&root, "scratch.md").unwrap();
        let now = SystemTime::now() + Duration::from_secs(86_400 * 31);
        let patterns = vec!["pinned-*".to_string(), "longterm/".to_string()];
        let report = sweep(
            Lang::En,
            Options {
                root: &root,
                archive_dir: root.join(".archive"),
                ttl: Duration::from_secs(86_400 * 30),
                keep_patterns: &patterns,
                dry_run: false,
                now,
            },
        )
        .unwrap();
        // pinned-todo.md と longterm/ は keep、scratch.md だけ archive される
        assert_eq!(report.archived.len(), 1);
        assert_eq!(report.archived[0].name, "scratch.md");
        assert!(root.join("pinned-todo.md").exists());
        assert!(root.join("longterm").is_dir());
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn sweep_skips_broken_symlink_with_warning() {
        let root = temp_root();
        // 壊れた symlink: 指す先が存在しないため exists() が false、warning + skip 経路に入る
        std::os::unix::fs::symlink("/nonexistent/x", root.join("broken")).unwrap();
        scratch::create_file(&root, "old.md").unwrap();
        let now = SystemTime::now() + Duration::from_secs(86_400 * 31);
        let report = sweep(
            Lang::En,
            Options {
                root: &root,
                archive_dir: root.join(".archive"),
                ttl: Duration::from_secs(86_400 * 30),
                keep_patterns: &[],
                dry_run: false,
                now,
            },
        )
        .unwrap();
        // broken symlink は archive されず、old.md のみ archive される
        assert_eq!(report.archived.len(), 1);
        assert_eq!(report.archived[0].name, "old.md");
        assert!(
            root.join("broken").symlink_metadata().is_ok(),
            "broken symlink should remain in root"
        );
        assert!(!root.join(".archive/broken").exists());
        // README 契約「mtime 取れない → skip + stderr warning」の warning 部分を assert
        assert_eq!(report.errors.len(), 1, "broken symlink should emit warning");
        assert!(
            report.errors[0].contains("mtime"),
            "warning should mention mtime: {}",
            report.errors[0]
        );
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn sweep_rejects_archive_dir_with_dotdot_to_root() {
        let root = temp_root();
        scratch::create_file(&root, "old.md").unwrap();
        let now = SystemTime::now() + Duration::from_secs(86_400 * 31);
        // archive_dir = "<root>/missing/.." (canonicalize 失敗、lexical normalize で root に等しい)
        let archive_dir = root.join("missing").join("..");
        let err = sweep(
            Lang::En,
            Options {
                root: &root,
                archive_dir,
                ttl: Duration::from_secs(86_400 * 30),
                keep_patterns: &[],
                dry_run: false,
                now,
            },
        )
        .unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidInput);
        // missing ディレクトリは作られない、old.md も remain
        assert!(!root.join("missing").exists());
        assert!(root.join("old.md").exists());
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn sweep_rejects_archive_dir_equal_to_root() {
        let root = temp_root();
        scratch::create_file(&root, "old.md").unwrap();
        let now = SystemTime::now() + Duration::from_secs(86_400 * 31);
        // archive_dir == root: silent no-op を防ぐため abort 必須
        let err = sweep(
            Lang::En,
            Options {
                root: &root,
                archive_dir: root.clone(),
                ttl: Duration::from_secs(86_400 * 30),
                keep_patterns: &[],
                dry_run: false,
                now,
            },
        )
        .unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidInput);
        // old.md は move されず残存
        assert!(root.join("old.md").exists());
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn resolve_dest_double_collision_uses_counter() {
        let root = temp_root();
        let arch = root.join(".archive");
        fs::create_dir_all(&arch).unwrap();
        // primary と <name>.<ts> の両方を既存にして連番付与を強制する
        fs::write(arch.join("foo"), b"primary").unwrap();
        let ts = SystemTime::UNIX_EPOCH + Duration::from_secs(1700000000);
        fs::write(arch.join("foo.1700000000"), b"with_ts").unwrap();
        let dest = resolve_dest(&arch, "foo", ts);
        assert_eq!(dest, arch.join("foo.1700000000_1"));
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn resolve_dest_treats_dangling_symlink_as_collision() {
        let root = temp_root();
        let arch = root.join(".archive");
        fs::create_dir_all(&arch).unwrap();
        // archive に既存の dangling symlink (target 不在) が同名でいるケース
        std::os::unix::fs::symlink("/nonexistent/x", arch.join("foo")).unwrap();
        // Path::exists() は dangling symlink で false を返すため、修正前は primary が選ばれ既存 link が上書きされていた
        let dest = resolve_dest(&arch, "foo", SystemTime::UNIX_EPOCH);
        assert_ne!(
            dest,
            arch.join("foo"),
            "primary should be skipped due to dangling symlink presence"
        );
        // 既存 link は残存
        assert!(arch.join("foo").symlink_metadata().is_ok());
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn sweep_warns_when_keep_probe_fails() {
        // POSIX で `.keep` 確認が permission denied になる状況: root 所有・mode 100 のディレクトリで
        // search 権限がないと内部の `.keep` を stat できない。test 環境では search 権限を除いた
        // dir 内に `.keep` を作ってから permission を落とすことで再現する
        let root = temp_root();
        let d = scratch::create_dir(&root, "guarded").unwrap();
        // 内部に .keep を置いてから dir の search 権限を除く
        fs::write(d.join(KEEP_MARKER), b"").unwrap();
        // owner search 権限のみ除去 (rwx --> rw-)
        let mut perms = fs::metadata(&d).unwrap().permissions();
        use std::os::unix::fs::PermissionsExt;
        perms.set_mode(0o600);
        fs::set_permissions(&d, perms).unwrap();
        let now = SystemTime::now() + Duration::from_secs(86_400 * 31);
        let report = sweep(
            Lang::En,
            Options {
                root: &root,
                archive_dir: root.join(".archive"),
                ttl: Duration::from_secs(86_400 * 30),
                keep_patterns: &[],
                dry_run: false,
                now,
            },
        )
        .unwrap();
        // 権限を戻して片付けに備える
        let mut perms = fs::metadata(&d).unwrap().permissions();
        perms.set_mode(0o700);
        fs::set_permissions(&d, perms).unwrap();
        // root で実行された場合は permission 制限が効かないため、その場合は skip
        if !report.errors.is_empty() {
            assert!(
                report.errors[0].contains("keep"),
                ".keep probe warning expected: {}",
                report.errors[0]
            );
            assert!(
                report.archived.is_empty(),
                "protected dir should not be archived"
            );
        }
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn sweep_handles_name_collision() {
        let root = temp_root();
        let arch = root.join(".archive");
        fs::create_dir_all(&arch).unwrap();
        // 既に同名が archive にある状態で新たに old.md を archive する
        fs::write(arch.join("old.md"), b"old archived").unwrap();
        scratch::create_file(&root, "old.md").unwrap();
        let now = SystemTime::now() + Duration::from_secs(86_400 * 31);
        let report = sweep(
            Lang::En,
            Options {
                root: &root,
                archive_dir: arch.clone(),
                ttl: Duration::from_secs(86_400 * 30),
                keep_patterns: &[],
                dry_run: false,
                now,
            },
        )
        .unwrap();
        assert_eq!(report.archived.len(), 1);
        // suffix .<ts> が付いた dest になっている
        let archived = &report.archived[0];
        assert!(
            archived
                .dest
                .file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("old.md.")
        );
        // 元の archive (old.md) は上書きされていない
        assert_eq!(
            fs::read_to_string(arch.join("old.md")).unwrap(),
            "old archived"
        );
        fs::remove_dir_all(&root).unwrap();
    }
}
