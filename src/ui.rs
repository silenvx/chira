use chrono::{DateTime, Local};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, List, ListItem, ListState, Paragraph, Wrap};

use crate::app::{App, InputKind, Mode};
use crate::i18n;

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
    } else if app.mode == Mode::ActionPick {
        render_action_pick(frame, app, body);
    } else if app.mode == Mode::ConfirmAction {
        render_confirm_action(frame, app, body);
    } else if app.mode == Mode::Help {
        render_help(frame, app, body);
    }
}

fn render_header(frame: &mut Frame, app: &App, area: Rect) {
    let mut spans = vec![
        Span::styled(
            " chira ",
            Style::new().bg(Color::Cyan).fg(Color::Black).bold(),
        ),
        Span::raw(format!(" {}  ", app.rel_path())),
        Span::styled(
            i18n::header_count(app.lang, app.visible().len()),
            Style::new().fg(Color::Gray),
        ),
    ];
    if app.mode == Mode::Search || !app.search.is_empty() {
        let cursor = if app.mode == Mode::Search { "▌" } else { "" };
        spans.push(Span::raw("   "));
        spans.push(Span::styled(
            i18n::header_search(app.lang, &app.search, cursor),
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
                Span::styled(
                    when.format("%m/%d %H:%M").to_string(),
                    Style::new().fg(Color::Gray),
                ),
                Span::raw("  "),
                name,
            ]))
        })
        .collect();

    let list = List::new(items)
        .block(Block::bordered().title(i18n::list_title(app.lang)))
        .highlight_style(Style::new().add_modifier(Modifier::REVERSED))
        .highlight_symbol("› ");
    let mut state = ListState::default();
    if !visible.is_empty() {
        state.select(Some(app.selected));
    }
    frame.render_stateful_widget(list, left, &mut state);

    let title = match app.selected_entry() {
        Some(e) if e.is_dir => i18n::preview_dir_title(app.lang),
        Some(_) => i18n::preview_file_title(app.lang),
        None => i18n::preview_file_title(app.lang),
    };
    let preview = if app.preview.is_empty() && app.selected_entry().is_none() {
        let hint = if app.search.is_empty() {
            i18n::empty_hint(app.lang)
        } else {
            i18n::empty_search_hint(app.lang)
        };
        Paragraph::new(hint).style(Style::new().fg(Color::Gray))
    } else {
        Paragraph::new(app.preview.as_str()).wrap(Wrap { trim: false })
    };
    frame.render_widget(preview.block(Block::bordered().title(title)), right);
}

fn render_input(frame: &mut Frame, app: &App, kind: InputKind, area: Rect) {
    let title = match kind {
        InputKind::NewFile => i18n::input_title_new_file(app.lang),
        InputKind::NewDir => i18n::input_title_new_dir(app.lang),
        InputKind::Rename => i18n::input_title_rename(app.lang),
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
        i18n::confirm_delete_dir(app.lang, &truncate(&name, 26))
    } else {
        i18n::confirm_delete_file(app.lang, &truncate(&name, 30))
    };
    let text = vec![
        Line::raw(what),
        Line::from(vec![
            Span::styled("y", Style::new().fg(Color::Red).bold()),
            Span::raw(i18n::confirm_delete_label(app.lang)),
            Span::styled("n/Esc", Style::new().fg(Color::Green).bold()),
            Span::raw(i18n::confirm_cancel_label(app.lang)),
        ]),
    ];
    frame.render_widget(
        Paragraph::new(text).block(
            Block::bordered()
                .title(i18n::confirm_title(app.lang))
                .border_style(Color::Red),
        ),
        popup,
    );
}

fn render_action_pick(frame: &mut Frame, app: &App, area: Rect) {
    let items: Vec<ListItem> = app
        .actions
        .iter()
        .map(|a| {
            let mut spans = vec![Span::styled(
                a.name.clone(),
                Style::new().fg(Color::Blue).bold(),
            )];
            if let Some(desc) = &a.description {
                spans.push(Span::raw("  "));
                spans.push(Span::styled(desc.clone(), Style::new().fg(Color::Gray)));
            }
            ListItem::new(Line::from(spans))
        })
        .collect();

    let popup = centered(area, 60, app.actions.len() as u16 + 2);
    frame.render_widget(Clear, popup);
    let list = List::new(items)
        .block(
            Block::bordered()
                .title(i18n::action_pick_title(app.lang))
                .border_style(Color::Cyan),
        )
        .highlight_style(Style::new().add_modifier(Modifier::REVERSED))
        .highlight_symbol("› ");
    let mut state = ListState::default();
    if !app.actions.is_empty() {
        state.select(Some(app.action_cursor));
    }
    frame.render_stateful_widget(list, popup, &mut state);
}

/// s を inner_w 桁で折返したときの各行を返す (空行は 1 行として保持)。
/// 概算は char 数ベース (全角は実表示幅で更に折れうるが、ASCII コマンドが主のため許容)。
fn wrap_rows(s: &str, inner_w: usize) -> Vec<String> {
    let mut out = Vec::new();
    for logical in s.split('\n') {
        let chars: Vec<char> = logical.chars().collect();
        if chars.is_empty() {
            out.push(String::new());
        } else {
            for chunk in chars.chunks(inner_w.max(1)) {
                out.push(chunk.iter().collect());
            }
        }
    }
    out
}

fn render_confirm_action(frame: &mut Frame, app: &App, area: Rect) {
    let command = app
        .pending_action()
        .map(|a| a.run.clone())
        .unwrap_or_default();
    let name = app.pending_name();

    // 信頼ゲートはコマンド全文と操作行を必ず見せるのが要件。固定高だと長い dir 名で折返す
    // prompt 行や長い / 複数行 run でコマンド・操作行がクリップされるため、prompt と command
    // 両方の折返し行数からポップアップ高さを算出する。
    let width = 72.min(area.width.saturating_sub(2)).max(20);
    let inner_w = (width as usize).saturating_sub(2).max(1);
    let prompt_lines = wrap_rows(&i18n::confirm_action_prompt(app.lang, name), inner_w);
    let cmd_lines = wrap_rows(&command, inner_w);
    // 内側 = prompt N + command M + 空行 1 + 操作行 1、border 上下 2 を足し area に cap する
    let height = ((prompt_lines.len() + cmd_lines.len()) as u16 + 4).min(area.height);

    let popup = centered(area, width, height);
    frame.render_widget(Clear, popup);

    let mut text: Vec<Line> = prompt_lines.into_iter().map(Line::raw).collect();
    for l in cmd_lines {
        text.push(Line::from(Span::styled(l, Style::new().fg(Color::Yellow))));
    }
    text.push(Line::raw(""));
    text.push(Line::from(vec![
        Span::styled("y", Style::new().fg(Color::Red).bold()),
        Span::raw(i18n::confirm_action_run_label(app.lang)),
        Span::styled("n/Esc", Style::new().fg(Color::Green).bold()),
        Span::raw(i18n::confirm_cancel_label(app.lang)),
    ]));

    frame.render_widget(
        Paragraph::new(text).wrap(Wrap { trim: false }).block(
            Block::bordered()
                .title(i18n::confirm_title(app.lang))
                .border_style(Color::Red),
        ),
        popup,
    );
}

fn render_help(frame: &mut Frame, app: &App, area: Rect) {
    let keys = i18n::help_rows(app.lang);
    let lines: Vec<Line> = keys
        .iter()
        .map(|(k, desc)| {
            Line::from(vec![
                Span::styled(format!("  {k:<16}"), Style::new().fg(Color::Cyan).bold()),
                Span::raw(*desc),
            ])
        })
        .collect();

    let popup = centered(area, 56, keys.len() as u16 + 2);
    frame.render_widget(Clear, popup);
    frame.render_widget(
        Paragraph::new(lines).block(Block::bordered().title(i18n::help_title(app.lang))),
        popup,
    );
}

fn render_footer(frame: &mut Frame, app: &App, area: Rect) {
    let help = match app.mode {
        Mode::Browse => i18n::footer_browse(app.lang),
        Mode::Search => i18n::footer_search(app.lang),
        Mode::Input(_) => i18n::footer_input(app.lang),
        Mode::ConfirmDelete => i18n::footer_confirm(app.lang),
        Mode::ActionPick => i18n::footer_action_pick(app.lang),
        Mode::ConfirmAction => i18n::footer_confirm_action(app.lang),
        Mode::Help => i18n::footer_help_close(app.lang),
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
