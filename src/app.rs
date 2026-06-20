use std::io;
use std::path::{Path, PathBuf};

use chrono::Local;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use crate::scratch::{self, Entry};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum InputKind {
    NewFile,
    NewDir,
    Rename,
}

#[derive(PartialEq, Eq, Debug)]
pub enum Mode {
    Browse,
    Search,
    Input(InputKind),
    ConfirmDelete,
    Help,
}

/// TUI を一旦抜けて外部プロセスを動かす要求 (main のループが処理する)
pub enum Pending {
    Editor(PathBuf),
    Shell(PathBuf),
}

const PREVIEW_MAX_BYTES: u64 = 1 << 20; // 1 MiB

pub struct App {
    pub root: PathBuf,
    pub cwd: PathBuf,
    pub entries: Vec<Entry>,
    pub selected: usize,
    pub mode: Mode,
    pub search: String,
    pub input: String,
    pub preview: String,
    pub status: String,
    pub pending: Option<Pending>,
    pub should_quit: bool,
}

impl App {
    pub fn new() -> io::Result<Self> {
        Ok(Self::with_root(scratch::root()?))
    }

    pub fn with_root(root: PathBuf) -> Self {
        let mut app = Self {
            cwd: root.clone(),
            root,
            entries: Vec::new(),
            selected: 0,
            mode: Mode::Browse,
            search: String::new(),
            input: String::new(),
            preview: String::new(),
            status: String::new(),
            pending: None,
            should_quit: false,
        };
        app.refresh();
        app
    }

    /// cwd を相対表示用に root からの相対パスへ変換する
    pub fn rel_path(&self) -> String {
        match self.cwd.strip_prefix(&self.root) {
            Ok(p) if p.as_os_str().is_empty() => "scrap".into(),
            Ok(p) => format!("scrap/{}", p.display()),
            Err(_) => self.cwd.display().to_string(),
        }
    }

    /// 検索でフィルタ済みの entries インデックス列
    pub fn visible(&self) -> Vec<usize> {
        let q = self.search.to_lowercase();
        self.entries
            .iter()
            .enumerate()
            .filter(|(_, e)| q.is_empty() || e.name.to_lowercase().contains(&q))
            .map(|(i, _)| i)
            .collect()
    }

    pub fn selected_entry(&self) -> Option<&Entry> {
        self.visible().get(self.selected).map(|&i| &self.entries[i])
    }

    pub fn refresh(&mut self) {
        self.entries = scratch::list(&self.cwd).unwrap_or_default();
        let len = self.visible().len();
        if self.selected >= len {
            self.selected = len.saturating_sub(1);
        }
        self.update_preview();
    }

    fn update_preview(&mut self) {
        self.preview = match self.selected_entry() {
            None => String::new(),
            Some(e) if e.is_dir => scratch::tree(&e.path, 4, 100),
            Some(e) => preview_file(&e.path),
        };
    }

    pub fn on_key(&mut self, key: KeyEvent) {
        if key.kind != KeyEventKind::Press {
            return;
        }
        match self.mode {
            Mode::Browse => self.on_key_browse(key),
            Mode::Search => self.on_key_search(key),
            Mode::Input(_) => self.on_key_input(key),
            Mode::ConfirmDelete => self.on_key_confirm(key),
            // ヘルプは何かキーを押せば閉じる
            Mode::Help => self.mode = Mode::Browse,
        }
    }

    fn on_key_browse(&mut self, key: KeyEvent) {
        let len = self.visible().len();
        match key.code {
            KeyCode::Char('q') => self.should_quit = true,
            KeyCode::Char('j') | KeyCode::Down => {
                if len > 0 {
                    self.selected = (self.selected + 1).min(len - 1);
                    self.update_preview();
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.selected = self.selected.saturating_sub(1);
                self.update_preview();
            }
            KeyCode::Char('g') | KeyCode::Home => {
                self.selected = 0;
                self.update_preview();
            }
            KeyCode::Char('G') | KeyCode::End => {
                self.selected = len.saturating_sub(1);
                self.update_preview();
            }
            KeyCode::Enter | KeyCode::Char('l') | KeyCode::Right => self.open_selected(),
            KeyCode::Char('h') | KeyCode::Backspace | KeyCode::Left => self.ascend(),
            KeyCode::Char('e') => self.edit_selected(),
            KeyCode::Char('s') => {
                let target = match self.selected_entry() {
                    Some(e) if e.is_dir => e.path.clone(),
                    _ => self.cwd.clone(),
                };
                self.pending = Some(Pending::Shell(target));
            }
            KeyCode::Char('n') => self.begin_input(InputKind::NewFile),
            KeyCode::Char('N') => self.begin_input(InputKind::NewDir),
            KeyCode::Char('r') => {
                if self.selected_entry().is_some() {
                    self.begin_input(InputKind::Rename);
                }
            }
            KeyCode::Char('d') => {
                if self.selected_entry().is_some() {
                    self.mode = Mode::ConfirmDelete;
                }
            }
            KeyCode::Char('/') => {
                self.mode = Mode::Search;
                self.status.clear();
            }
            KeyCode::Char('?') => self.mode = Mode::Help,
            KeyCode::Esc => {
                if !self.search.is_empty() {
                    self.search.clear();
                    self.selected = 0;
                    self.update_preview();
                }
            }
            _ => {}
        }
    }

    fn on_key_search(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => {
                self.search.clear();
                self.selected = 0;
                self.mode = Mode::Browse;
                self.update_preview();
            }
            KeyCode::Enter | KeyCode::Down | KeyCode::Up => {
                self.mode = Mode::Browse;
            }
            KeyCode::Backspace => {
                self.search.pop();
                self.selected = 0;
                self.update_preview();
            }
            KeyCode::Char(c) => {
                self.search.push(c);
                self.selected = 0;
                self.update_preview();
            }
            _ => {}
        }
    }

    fn on_key_input(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => {
                self.input.clear();
                self.mode = Mode::Browse;
            }
            KeyCode::Enter => self.commit_input(),
            KeyCode::Backspace => {
                self.input.pop();
            }
            KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.input.push(c)
            }
            _ => {}
        }
    }

    fn on_key_confirm(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('y') | KeyCode::Enter => {
                if let Some(entry) = self.selected_entry().cloned() {
                    match scratch::remove(&entry) {
                        Ok(()) => self.status = format!("削除しました: {}", entry.name),
                        Err(e) => self.status = format!("削除に失敗: {e}"),
                    }
                    self.refresh();
                }
                self.mode = Mode::Browse;
            }
            _ => self.mode = Mode::Browse,
        }
    }

    fn open_selected(&mut self) {
        match self.selected_entry() {
            Some(e) if e.is_dir => {
                let dest = e.path.clone();
                self.set_cwd(dest);
            }
            Some(_) => self.edit_selected(),
            None => {}
        }
    }

    fn ascend(&mut self) {
        if self.cwd != self.root
            && let Some(parent) = self.cwd.parent()
        {
            let dest = parent.to_path_buf();
            self.set_cwd(dest);
        }
    }

    fn set_cwd(&mut self, dest: PathBuf) {
        self.cwd = dest;
        self.selected = 0;
        self.search.clear();
        self.refresh();
    }

    fn edit_selected(&mut self) {
        if let Some(e) = self.selected_entry()
            && !e.is_dir
        {
            self.pending = Some(Pending::Editor(e.path.clone()));
        }
    }

    fn begin_input(&mut self, kind: InputKind) {
        self.input = match kind {
            InputKind::NewFile => Local::now().format("scratch-%Y%m%d-%H%M%S.md").to_string(),
            InputKind::NewDir => Local::now().format("scratch-%Y%m%d-%H%M%S").to_string(),
            InputKind::Rename => self
                .selected_entry()
                .map(|e| e.name.clone())
                .unwrap_or_default(),
        };
        self.mode = Mode::Input(kind);
        self.status.clear();
    }

    fn commit_input(&mut self) {
        let Mode::Input(kind) = self.mode else {
            return;
        };
        let name = self.input.trim().to_string();
        self.input.clear();
        self.mode = Mode::Browse;
        match kind {
            InputKind::NewFile => match scratch::create_file(&self.cwd, &name) {
                Ok(path) => {
                    self.refresh();
                    self.select_by_name(&name);
                    // 作成直後にそのまま $EDITOR で開く
                    self.pending = Some(Pending::Editor(path));
                }
                Err(e) => self.status = format!("作成に失敗: {e}"),
            },
            InputKind::NewDir => match scratch::create_dir(&self.cwd, &name) {
                Ok(_) => {
                    self.refresh();
                    self.select_by_name(&name);
                    self.status = format!("作成しました: {name}/");
                }
                Err(e) => self.status = format!("作成に失敗: {e}"),
            },
            InputKind::Rename => {
                if let Some(entry) = self.selected_entry().cloned() {
                    match scratch::rename(&entry, &name) {
                        Ok(_) => {
                            self.refresh();
                            self.select_by_name(&name);
                            self.status = "名前を変更しました".into();
                        }
                        Err(e) => self.status = format!("変更に失敗: {e}"),
                    }
                }
            }
        }
    }

    fn select_by_name(&mut self, name: &str) {
        if let Some(pos) = self
            .visible()
            .iter()
            .position(|&i| self.entries[i].name == name)
        {
            self.selected = pos;
            self.update_preview();
        }
    }
}

fn preview_file(path: &Path) -> String {
    match std::fs::metadata(path) {
        Ok(m) if m.len() > PREVIEW_MAX_BYTES => format!("(大きいファイル: {} bytes)", m.len()),
        Ok(_) => match scratch::read_text(path) {
            Ok(text) => text,
            Err(_) => "(バイナリ/読み取り不可)".into(),
        },
        Err(e) => format!("(読み取り不可: {e})"),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU32, Ordering};

    use super::*;

    fn temp_root() -> PathBuf {
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("scrap-app-{}-{}", std::process::id(), n));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn key(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE)
    }

    fn special(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn typed(app: &mut App, s: &str) {
        for c in s.chars() {
            app.on_key(key(c));
        }
    }

    #[test]
    fn new_file_requests_editor_then_delete() {
        let root = temp_root();
        let mut app = App::with_root(root.clone());

        // n で新規ファイル → 既定名を消して明示名 → Enter で作成し $EDITOR 要求
        app.on_key(key('n'));
        assert!(matches!(app.mode, Mode::Input(InputKind::NewFile)));
        app.input.clear();
        typed(&mut app, "note.md");
        app.on_key(special(KeyCode::Enter));
        assert_eq!(app.mode, Mode::Browse);
        assert!(root.join("note.md").exists());
        match app.pending.take() {
            Some(Pending::Editor(p)) => assert!(p.ends_with("note.md")),
            _ => panic!("作成直後は $EDITOR 起動が要求されるはず"),
        }

        // 外部エディタの代わりに本文を書いておく
        std::fs::write(root.join("note.md"), "本文あ").unwrap();
        app.refresh();
        assert_eq!(scratch::read_text(&root.join("note.md")).unwrap(), "本文あ");

        // d → y で削除
        app.on_key(key('d'));
        assert_eq!(app.mode, Mode::ConfirmDelete);
        app.on_key(key('y'));
        assert!(!root.join("note.md").exists());

        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn enter_on_file_requests_editor() {
        let root = temp_root();
        scratch::create_file(&root, "a.md").unwrap();
        let mut app = App::with_root(root.clone());
        app.on_key(special(KeyCode::Enter));
        assert!(matches!(app.pending, Some(Pending::Editor(_))));
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn create_dir_descend_ascend() {
        let root = temp_root();
        let mut app = App::with_root(root.clone());

        app.on_key(key('N'));
        app.input.clear();
        typed(&mut app, "ws");
        app.on_key(special(KeyCode::Enter));
        assert!(root.join("ws").is_dir());
        assert!(app.pending.is_none(), "ディレクトリ作成では editor を起動しない");

        // 作成した ws が選択されている → Enter で降下
        app.on_key(special(KeyCode::Enter));
        assert_eq!(app.cwd, root.join("ws"));
        // h で親へ戻る
        app.on_key(key('h'));
        assert_eq!(app.cwd, root);

        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn arrow_keys_navigate_like_hl() {
        let root = temp_root();
        scratch::create_dir(&root, "ws").unwrap();
        let mut app = App::with_root(root.clone());

        // → で降下、← で親へ
        app.on_key(special(KeyCode::Right));
        assert_eq!(app.cwd, root.join("ws"));
        app.on_key(special(KeyCode::Left));
        assert_eq!(app.cwd, root);

        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn renders_each_mode_without_panic() {
        use ratatui::Terminal;
        use ratatui::backend::TestBackend;

        let root = temp_root();
        scratch::create_file(&root, "a.md").unwrap();
        scratch::create_dir(&root, "ws").unwrap();
        let mut app = App::with_root(root.clone());
        let mut term = Terminal::new(TestBackend::new(80, 24)).unwrap();

        term.draw(|f| crate::ui::render(f, &app)).unwrap(); // Browse
        app.on_key(key('n'));
        term.draw(|f| crate::ui::render(f, &app)).unwrap(); // Input popup
        app.on_key(special(KeyCode::Esc));
        app.on_key(key('d'));
        term.draw(|f| crate::ui::render(f, &app)).unwrap(); // ConfirmDelete
        app.on_key(special(KeyCode::Esc));
        app.on_key(key('?'));
        term.draw(|f| crate::ui::render(f, &app)).unwrap(); // Help

        // 極小サイズでもレイアウト計算が panic しないこと
        let mut tiny = Terminal::new(TestBackend::new(4, 2)).unwrap();
        tiny.draw(|f| crate::ui::render(f, &app)).unwrap();

        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn help_opens_and_any_key_closes() {
        let root = temp_root();
        let mut app = App::with_root(root.clone());
        app.on_key(key('?'));
        assert_eq!(app.mode, Mode::Help);
        // h は Help 中はナビゲーションせず閉じるだけ
        app.on_key(key('h'));
        assert_eq!(app.mode, Mode::Browse);
        assert_eq!(app.cwd, root);
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn search_filters_entries() {
        let root = temp_root();
        scratch::create_file(&root, "alpha.md").unwrap();
        scratch::create_file(&root, "beta.md").unwrap();
        let mut app = App::with_root(root.clone());
        assert_eq!(app.visible().len(), 2);

        app.on_key(key('/'));
        typed(&mut app, "alp");
        assert_eq!(app.visible().len(), 1);
        assert_eq!(app.entries[app.visible()[0]].name, "alpha.md");

        std::fs::remove_dir_all(&root).unwrap();
    }
}
