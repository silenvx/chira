use std::io;
use std::path::{Path, PathBuf};

use chrono::Local;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use crate::config::Action;
use crate::i18n::{self, Lang};
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
    /// アクション選択ピッカー (`t`)
    ActionPick,
    /// 選択アクションの `run` を実行する前の確認 (信頼ゲート: コマンド全文を表示)
    ConfirmAction,
    Help,
}

/// TUI を一旦抜けて外部プロセスを動かす要求 (main のループが処理する)
pub enum Pending {
    Editor(PathBuf),
    Shell(PathBuf),
    /// アクションの `run` を新ディレクトリ (dir) 内で実行する。root は CHIRA_ROOT。
    Run {
        dir: PathBuf,
        root: PathBuf,
        command: String,
    },
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
    pub lang: Lang,
    /// config.toml の `[actions.*]` (名前順)。`t` ピッカーで選ぶ。
    pub actions: Vec<Action>,
    /// ActionPick 中のカーソル位置 (actions のインデックス)。
    pub action_cursor: usize,
    /// 選択中のアクション。name 入力 → ConfirmAction まで持ち回す。
    selected_action: Option<usize>,
    /// ConfirmAction で実行する新ディレクトリ名 (name 入力から持ち回す)。
    pending_name: String,
}

impl App {
    pub fn new(config_dir: Option<&str>) -> io::Result<Self> {
        Ok(Self::with_root(scratch::root(config_dir)?, i18n::lang()))
    }

    pub fn with_root(root: PathBuf, lang: Lang) -> Self {
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
            lang,
            actions: Vec::new(),
            action_cursor: 0,
            selected_action: None,
            pending_name: String::new(),
        };
        app.refresh();
        app
    }

    /// ConfirmAction で表示・実行するアクション (選択中のもの)。
    pub fn pending_action(&self) -> Option<&Action> {
        self.selected_action.and_then(|i| self.actions.get(i))
    }

    /// ConfirmAction で作成しようとしている新ディレクトリ名。
    pub fn pending_name(&self) -> &str {
        &self.pending_name
    }

    /// cwd を相対表示用に root からの相対パスへ変換する
    pub fn rel_path(&self) -> String {
        match self.cwd.strip_prefix(&self.root) {
            Ok(p) if p.as_os_str().is_empty() => "chira".into(),
            Ok(p) => format!("chira/{}", p.display()),
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
            Some(e) if e.is_dir => scratch::tree(self.lang, &e.path, 4, 100),
            Some(e) => preview_file(self.lang, &e.path),
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
            Mode::ActionPick => self.on_key_action_pick(key),
            Mode::ConfirmAction => self.on_key_confirm_action(key),
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
            KeyCode::Char('t') => {
                if self.actions.is_empty() {
                    self.status = i18n::status_no_actions(self.lang).into();
                } else {
                    self.action_cursor = 0;
                    self.status.clear();
                    self.mode = Mode::ActionPick;
                }
            }
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
            KeyCode::Esc if !self.search.is_empty() => {
                self.search.clear();
                self.selected = 0;
                self.update_preview();
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
                // アクション経路の name 入力を中断した場合は選択も解除する
                // (残すと次の素の N で誤って ConfirmAction に入る)
                self.selected_action = None;
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
            // 削除確定は明示的な y のみ (Enter は誤削除防止のためキャンセル扱い)
            KeyCode::Char('y') => {
                if let Some(entry) = self.selected_entry().cloned() {
                    match scratch::remove(&entry) {
                        Ok(()) => self.status = i18n::status_deleted(self.lang, &entry.name),
                        Err(e) => self.status = i18n::status_delete_failed(self.lang, &e),
                    }
                    self.refresh();
                }
                self.mode = Mode::Browse;
            }
            _ => self.mode = Mode::Browse,
        }
    }

    fn on_key_action_pick(&mut self, key: KeyEvent) {
        let len = self.actions.len();
        match key.code {
            KeyCode::Esc => self.mode = Mode::Browse,
            KeyCode::Char('j') | KeyCode::Down => {
                if len > 0 {
                    self.action_cursor = (self.action_cursor + 1).min(len - 1);
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.action_cursor = self.action_cursor.saturating_sub(1);
            }
            KeyCode::Char('g') | KeyCode::Home => self.action_cursor = 0,
            KeyCode::Char('G') | KeyCode::End => self.action_cursor = len.saturating_sub(1),
            KeyCode::Enter | KeyCode::Char('l') | KeyCode::Right if self.action_cursor < len => {
                self.selected_action = Some(self.action_cursor);
                self.begin_input(InputKind::NewDir);
            }
            _ => {}
        }
    }

    fn on_key_confirm_action(&mut self, key: KeyEvent) {
        match key.code {
            // 実行確定は明示的な y のみ (Enter 等はキャンセル扱い: 任意コマンド実行の誤発火防止)
            KeyCode::Char('y') => self.run_pending_action(),
            _ => {
                self.selected_action = None;
                self.pending_name.clear();
                self.mode = Mode::Browse;
            }
        }
    }

    /// ConfirmAction で y を押したとき: 新ディレクトリを作成し、`run` を Pending::Run に積む。
    fn run_pending_action(&mut self) {
        self.mode = Mode::Browse;
        let Some(action) = self
            .selected_action
            .and_then(|i| self.actions.get(i))
            .cloned()
        else {
            self.selected_action = None;
            self.pending_name.clear();
            return;
        };
        let name = std::mem::take(&mut self.pending_name);
        self.selected_action = None;
        match scratch::create_dir(&self.cwd, &name) {
            Ok(path) => {
                self.search.clear();
                self.refresh();
                self.select_by_name(&name);
                self.status = i18n::status_run_action(self.lang, &action.name);
                // CHIRA_TARGET / CHIRA_ROOT は絶対パス契約。CHIRA_DIR / config dir が相対だと
                // root も dir も相対になりうるため、symlink を辿らない lexical 絶対化で揃える
                let dir = std::path::absolute(&path).unwrap_or(path);
                let root = std::path::absolute(&self.root).unwrap_or_else(|_| self.root.clone());
                self.pending = Some(Pending::Run {
                    dir,
                    root,
                    command: action.run,
                });
            }
            Err(e) => self.status = i18n::status_create_failed(self.lang, &e),
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
        // 検索フィルタ中でも作成/改名した項目を選択できるよう、成功時は検索を解除する
        // (select_by_name は visible() を見るため、フィルタが残ると新項目を選べない)
        match kind {
            InputKind::NewFile => match scratch::create_file(&self.cwd, &name) {
                Ok(path) => {
                    self.search.clear();
                    self.refresh();
                    self.select_by_name(&name);
                    // 作成直後にそのまま $EDITOR で開く
                    self.pending = Some(Pending::Editor(path));
                }
                Err(e) => self.status = i18n::status_create_failed(self.lang, &e),
            },
            InputKind::NewDir => {
                if self.selected_action.is_some() {
                    // アクション経路: ここでは作らず、コマンド全文を確認させてから実行する
                    self.pending_name = name;
                    self.mode = Mode::ConfirmAction;
                } else {
                    match scratch::create_dir(&self.cwd, &name) {
                        Ok(_) => {
                            self.search.clear();
                            self.refresh();
                            self.select_by_name(&name);
                            self.status = i18n::status_created_dir(self.lang, &name);
                        }
                        Err(e) => self.status = i18n::status_create_failed(self.lang, &e),
                    }
                }
            }
            InputKind::Rename => {
                if let Some(entry) = self.selected_entry().cloned() {
                    match scratch::rename(&entry, &name) {
                        Ok(_) => {
                            self.search.clear();
                            self.refresh();
                            self.select_by_name(&name);
                            self.status = i18n::status_renamed(self.lang).into();
                        }
                        Err(e) => self.status = i18n::status_rename_failed(self.lang, &e),
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

fn preview_file(lang: Lang, path: &Path) -> String {
    match std::fs::metadata(path) {
        // 通常ファイル以外 (FIFO はメインスレッドの read をブロックし、
        // キャラクタデバイスは len==0 で無限読み→OOM になる) は読まない
        Ok(m) if !m.is_file() => i18n::preview_special_file(lang).into(),
        Ok(m) if m.len() > PREVIEW_MAX_BYTES => i18n::preview_large_file(lang, m.len()),
        Ok(_) => match scratch::read_text(path) {
            Ok(text) => text,
            Err(_) => i18n::preview_binary(lang).into(),
        },
        Err(e) => i18n::preview_unreadable(lang, &e),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU32, Ordering};

    use super::*;

    fn temp_root() -> PathBuf {
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("chira-app-{}-{}", std::process::id(), n));
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
        let mut app = App::with_root(root.clone(), Lang::En);

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
            _ => panic!("creating a new file should request $EDITOR"),
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
        let mut app = App::with_root(root.clone(), Lang::En);
        app.on_key(special(KeyCode::Enter));
        assert!(matches!(app.pending, Some(Pending::Editor(_))));
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn create_dir_descend_ascend() {
        let root = temp_root();
        let mut app = App::with_root(root.clone(), Lang::En);

        app.on_key(key('N'));
        app.input.clear();
        typed(&mut app, "ws");
        app.on_key(special(KeyCode::Enter));
        assert!(root.join("ws").is_dir());
        assert!(
            app.pending.is_none(),
            "creating a directory should not request $EDITOR"
        );

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
        let mut app = App::with_root(root.clone(), Lang::En);

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
        let mut app = App::with_root(root.clone(), Lang::En);
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
        let mut app = App::with_root(root.clone(), Lang::En);
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
        let mut app = App::with_root(root.clone(), Lang::En);
        assert_eq!(app.visible().len(), 2);

        app.on_key(key('/'));
        typed(&mut app, "alp");
        assert_eq!(app.visible().len(), 1);
        assert_eq!(app.entries[app.visible()[0]].name, "alpha.md");

        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn create_while_filtering_clears_search_and_selects_new() {
        let root = temp_root();
        scratch::create_file(&root, "alpha.md").unwrap();
        let mut app = App::with_root(root.clone(), Lang::En);

        // "alp" で絞り込み確定 (Enter で Browse に戻るがフィルタは残る) → 一致しない名前で新規作成
        app.on_key(key('/'));
        typed(&mut app, "alp");
        app.on_key(special(KeyCode::Enter));
        assert_eq!(app.search, "alp");
        app.on_key(key('n'));
        app.input.clear();
        typed(&mut app, "report.md");
        app.on_key(special(KeyCode::Enter));

        assert!(app.search.is_empty());
        assert_eq!(
            app.selected_entry().map(|e| e.name.as_str()),
            Some("report.md")
        );

        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn enter_cancels_delete_confirm() {
        let root = temp_root();
        scratch::create_file(&root, "keep.md").unwrap();
        let mut app = App::with_root(root.clone(), Lang::En);

        app.on_key(key('d'));
        assert_eq!(app.mode, Mode::ConfirmDelete);
        // Enter は削除せずキャンセル (誤削除防止)
        app.on_key(special(KeyCode::Enter));
        assert_eq!(app.mode, Mode::Browse);
        assert!(root.join("keep.md").exists());

        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn preview_skips_non_regular_file() {
        let root = temp_root();
        let fifo = root.join("pipe");
        // FIFO を作る (mkfifo 不在の環境ではスキップ)。read すると main thread がブロックするため
        // preview_file は通常ファイル以外を read しないことを確認する
        let Ok(status) = std::process::Command::new("mkfifo").arg(&fifo).status() else {
            std::fs::remove_dir_all(&root).unwrap();
            return;
        };
        if !status.success() {
            std::fs::remove_dir_all(&root).unwrap();
            return;
        }
        assert!(preview_file(Lang::En, &fifo).contains("special file"));
        std::fs::remove_dir_all(&root).unwrap();
    }

    fn demo_action(run: &str) -> Action {
        Action {
            name: "demo".into(),
            description: Some("demo action".into()),
            run: run.into(),
        }
    }

    #[test]
    fn t_with_no_actions_shows_status_and_stays_browse() {
        let root = temp_root();
        let mut app = App::with_root(root.clone(), Lang::En);
        // actions 未設定なら t はピッカーを開かず status を出すだけ
        app.on_key(key('t'));
        assert_eq!(app.mode, Mode::Browse);
        assert!(!app.status.is_empty());
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn action_flow_creates_dir_and_requests_run_on_confirm() {
        let root = temp_root();
        let mut app = App::with_root(root.clone(), Lang::En);
        app.actions = vec![demo_action("git init -q")];

        app.on_key(key('t'));
        assert_eq!(app.mode, Mode::ActionPick);
        // アクション選択 → 名前入力へ
        app.on_key(special(KeyCode::Enter));
        assert!(matches!(app.mode, Mode::Input(InputKind::NewDir)));
        app.input.clear();
        typed(&mut app, "ws");
        // 名前確定 → 確認 (この時点ではまだ作らない)
        app.on_key(special(KeyCode::Enter));
        assert_eq!(app.mode, Mode::ConfirmAction);
        assert!(
            !root.join("ws").exists(),
            "confirm 前にディレクトリを作らない"
        );
        assert_eq!(app.pending_name(), "ws");
        assert_eq!(
            app.pending_action().map(|a| a.run.as_str()),
            Some("git init -q")
        );

        // y で作成 + run を Pending に積む
        app.on_key(key('y'));
        assert_eq!(app.mode, Mode::Browse);
        assert!(root.join("ws").is_dir());
        match app.pending.take() {
            Some(Pending::Run {
                dir,
                root: r,
                command,
            }) => {
                assert!(dir.ends_with("ws"));
                assert!(dir.is_absolute(), "CHIRA_TARGET 契約: dir は絶対パス");
                assert!(r.is_absolute(), "CHIRA_ROOT 契約: root は絶対パス");
                assert_eq!(command, "git init -q");
            }
            _ => panic!("confirm の y で Pending::Run を要求するはず"),
        }
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn action_confirm_cancel_creates_nothing() {
        let root = temp_root();
        let mut app = App::with_root(root.clone(), Lang::En);
        app.actions = vec![demo_action("git init -q")];

        app.on_key(key('t'));
        app.on_key(special(KeyCode::Enter));
        app.input.clear();
        typed(&mut app, "ws");
        app.on_key(special(KeyCode::Enter));
        assert_eq!(app.mode, Mode::ConfirmAction);
        // Esc でキャンセル → 何も作らない・Pending も積まない
        app.on_key(special(KeyCode::Esc));
        assert_eq!(app.mode, Mode::Browse);
        assert!(!root.join("ws").exists());
        assert!(app.pending.is_none());
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn input_esc_clears_action_so_plain_n_makes_empty_dir() {
        let root = temp_root();
        let mut app = App::with_root(root.clone(), Lang::En);
        app.actions = vec![demo_action("true")];

        // t → 選択 → name 入力中に Esc
        app.on_key(key('t'));
        app.on_key(special(KeyCode::Enter));
        assert!(matches!(app.mode, Mode::Input(InputKind::NewDir)));
        app.on_key(special(KeyCode::Esc));
        assert_eq!(app.mode, Mode::Browse);

        // 素の N は ConfirmAction に入らず空ディレクトリを作る (選択が残っていない証拠)
        app.on_key(key('N'));
        app.input.clear();
        typed(&mut app, "plain");
        app.on_key(special(KeyCode::Enter));
        assert_eq!(app.mode, Mode::Browse);
        assert!(root.join("plain").is_dir());
        assert!(app.pending.is_none());
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn renders_action_modes_without_panic() {
        use ratatui::Terminal;
        use ratatui::backend::TestBackend;

        let root = temp_root();
        let mut app = App::with_root(root.clone(), Lang::En);
        app.actions = vec![demo_action("git init -q")];
        let mut term = Terminal::new(TestBackend::new(80, 24)).unwrap();

        app.on_key(key('t'));
        term.draw(|f| crate::ui::render(f, &app)).unwrap(); // ActionPick
        app.on_key(special(KeyCode::Enter));
        app.input.clear();
        typed(&mut app, "ws");
        app.on_key(special(KeyCode::Enter));
        assert_eq!(app.mode, Mode::ConfirmAction);
        term.draw(|f| crate::ui::render(f, &app)).unwrap(); // ConfirmAction

        std::fs::remove_dir_all(&root).unwrap();
    }
}
