use chrono::{DateTime, Local};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, List, ListItem, ListState, Paragraph, Wrap};

use crate::app::{App, InputKind, Mode};

pub fn render(frame: &mut Frame, app: &App) {
    let [header, body, footer] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(0),
        Constraint::Length(1),
    ])
    .areas(frame.area());

    render_header(frame, app, header);
    render_browse(frame, app, body);
    render_footer(frame, app, footer);

    if let Mode::Input(kind) = app.mode {
        render_input(frame, app, kind, body);
    } else if app.mode == Mode::ConfirmDelete {
        render_confirm(frame, app, body);
    } else if app.mode == Mode::Help {
        render_help(frame, body);
    }
}

fn render_header(frame: &mut Frame, app: &App, area: Rect) {
    let mut spans = vec![
        Span::styled(" scrap ", Style::new().bg(Color::Cyan).fg(Color::Black).bold()),
        Span::raw(format!(" {}  ", app.rel_path())),
        Span::styled(format!("{} 件", app.visible().len()), Style::new().fg(Color::Gray)),
    ];
    if app.mode == Mode::Search || !app.search.is_empty() {
        let cursor = if app.mode == Mode::Search { "▌" } else { "" };
        spans.push(Span::raw("   "));
        spans.push(Span::styled(
            format!("検索: {}{}", app.search, cursor),
            Style::new().fg(Color::Yellow),
        ));
    }
    frame.render_widget(Line::from(spans), area);
}

fn render_browse(frame: &mut Frame, app: &App, area: Rect) {
    let [left, right] =
        Layout::horizontal([Constraint::Percentage(45), Constraint::Percentage(55)]).areas(area);

    let visible = app.visible();
    let items: Vec<ListItem> = visible
        .iter()
        .map(|&i| {
            let e = &app.entries[i];
            let when: DateTime<Local> = e.modified.into();
            let name = if e.is_dir {
                Span::styled(format!("{}/", e.name), Style::new().fg(Color::Blue).bold())
            } else {
                Span::raw(e.name.clone())
            };
            ListItem::new(Line::from(vec![
                Span::styled(when.format("%m/%d %H:%M").to_string(), Style::new().fg(Color::Gray)),
                Span::raw("  "),
                name,
            ]))
        })
        .collect();

    let list = List::new(items)
        .block(Block::bordered().title(" 一覧 "))
        .highlight_style(Style::new().add_modifier(Modifier::REVERSED))
        .highlight_symbol("› ");
    let mut state = ListState::default();
    if !visible.is_empty() {
        state.select(Some(app.selected));
    }
    frame.render_stateful_widget(list, left, &mut state);

    let title = match app.selected_entry() {
        Some(e) if e.is_dir => " ディレクトリ内容 ",
        Some(_) => " プレビュー ",
        None => " プレビュー ",
    };
    let preview = if app.preview.is_empty() && app.selected_entry().is_none() {
        let hint = if app.search.is_empty() {
            "空です。n: ファイル作成  N: ディレクトリ作成"
        } else {
            "一致するエントリがありません。"
        };
        Paragraph::new(hint).style(Style::new().fg(Color::Gray))
    } else {
        Paragraph::new(app.preview.as_str()).wrap(Wrap { trim: false })
    };
    frame.render_widget(preview.block(Block::bordered().title(title)), right);
}

fn render_input(frame: &mut Frame, app: &App, kind: InputKind, area: Rect) {
    let title = match kind {
        InputKind::NewFile => " 新規ファイル名 ",
        InputKind::NewDir => " 新規ディレクトリ名 ",
        InputKind::Rename => " 名前を変更 ",
    };
    let popup = centered(area, 60, 3);
    frame.render_widget(Clear, popup);
    let text = Line::from(vec![
        Span::raw(&app.input),
        Span::styled("▌", Style::new().fg(Color::Cyan)),
    ]);
    frame.render_widget(
        Paragraph::new(text).block(Block::bordered().title(title).border_style(Color::Cyan)),
        popup,
    );
}

fn render_confirm(frame: &mut Frame, app: &App, area: Rect) {
    let (name, is_dir) = app
        .selected_entry()
        .map(|e| (e.name.clone(), e.is_dir))
        .unwrap_or_default();
    let popup = centered(area, 54, 4);
    frame.render_widget(Clear, popup);
    let what = if is_dir {
        format!("ディレクトリ「{}」を中身ごと削除しますか?", truncate(&name, 26))
    } else {
        format!("「{}」を削除しますか?", truncate(&name, 30))
    };
    let text = vec![
        Line::raw(what),
        Line::from(vec![
            Span::styled("y", Style::new().fg(Color::Red).bold()),
            Span::raw(": 削除   "),
            Span::styled("n/Esc", Style::new().fg(Color::Green).bold()),
            Span::raw(": キャンセル"),
        ]),
    ];
    frame.render_widget(
        Paragraph::new(text).block(Block::bordered().title(" 確認 ").border_style(Color::Red)),
        popup,
    );
}

fn render_help(frame: &mut Frame, area: Rect) {
    const KEYS: &[(&str, &str)] = &[
        ("j / k, ↓ / ↑", "カーソル移動"),
        ("g / G", "先頭 / 末尾"),
        ("l / → / Enter", "開く (ファイル→$EDITOR, dir→中へ)"),
        ("h / ← / Backspace", "親ディレクトリへ戻る"),
        ("e", "$EDITOR で開く"),
        ("s", "シェルを開く (実験・agent 実行)"),
        ("n / N", "新規ファイル / ディレクトリ"),
        ("r", "名前を変更"),
        ("d", "削除 (確認あり)"),
        ("/", "名前で絞り込み検索"),
        ("?", "このヘルプ"),
        ("q", "終了"),
    ];

    let lines: Vec<Line> = KEYS
        .iter()
        .map(|(k, desc)| {
            Line::from(vec![
                Span::styled(format!("  {k:<16}"), Style::new().fg(Color::Cyan).bold()),
                Span::raw(*desc),
            ])
        })
        .collect();

    let popup = centered(area, 56, KEYS.len() as u16 + 2);
    frame.render_widget(Clear, popup);
    frame.render_widget(
        Paragraph::new(lines).block(Block::bordered().title(" ヘルプ (vim-like) ")),
        popup,
    );
}

fn render_footer(frame: &mut Frame, app: &App, area: Rect) {
    let help = match app.mode {
        Mode::Browse => "j/k:移動  l:開く  h:親  s:シェル  n:新規  /:検索  ?:ヘルプ  q:終了",
        Mode::Search => "文字入力で絞り込み  Enter:確定  Esc:クリア",
        Mode::Input(_) => "Enter:決定  Esc:キャンセル",
        Mode::ConfirmDelete => "y:削除  n/Esc:キャンセル",
        Mode::Help => "何かキーを押すと閉じる",
    };
    let line = if app.status.is_empty() {
        Line::from(Span::styled(help, Style::new().fg(Color::Gray)))
    } else {
        Line::from(vec![
            Span::styled(&app.status, Style::new().fg(Color::Green)),
            Span::raw("   "),
            Span::styled(help, Style::new().fg(Color::Gray)),
        ])
    };
    frame.render_widget(line, area);
}

fn centered(area: Rect, width: u16, height: u16) -> Rect {
    let w = width.min(area.width.saturating_sub(2));
    let h = height.min(area.height);
    Rect {
        x: area.x + (area.width.saturating_sub(w)) / 2,
        y: area.y + (area.height.saturating_sub(h)) / 2,
        width: w,
        height: h,
    }
}

fn truncate(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        s.to_string()
    } else {
        let mut out: String = s.chars().take(max_chars).collect();
        out.push('…');
        out
    }
}
