use crate::app::{AccountEntry, App, NavigationLevel, PreviewContent};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, List, ListItem, Paragraph, Wrap},
    Frame,
};
use std::sync::atomic::Ordering;
use syntect::easy::HighlightLines;
use syntect::highlighting::ThemeSet;
use syntect::parsing::SyntaxSet;

// ─── Exact Fabest / Varwiz "Ink" Preset Theme ──────────────────────────────
pub const BG: Color = Color::Rgb(0, 0, 0);                 // Pure black
pub const FG: Color = Color::Rgb(240, 240, 240);           // Crisp white
pub const DIM: Color = Color::Rgb(130, 130, 130);          // Neutral grey
pub const PANEL: Color = Color::Rgb(28, 28, 28);           // Panel background
pub const ACCENT: Color = Color::Rgb(255, 255, 255);       // Pure white accent
pub const BORDER_TYPE: BorderType = BorderType::Double;     // Border::Double

pub struct SyntaxHighlighter {
    ps: SyntaxSet,
    ts: ThemeSet,
}

impl SyntaxHighlighter {
    pub fn new() -> Self {
        Self {
            ps: SyntaxSet::load_defaults_newlines(),
            ts: ThemeSet::load_defaults(),
        }
    }

    pub fn highlight<'a>(&self, filename: &str, content: &'a str) -> Vec<Line<'a>> {
        let ext = filename.rsplit('.').next().unwrap_or("");
        let syntax = self
            .ps
            .find_syntax_by_extension(ext)
            .or_else(|| self.ps.find_syntax_by_first_line(content))
            .unwrap_or_else(|| self.ps.find_syntax_plain_text());

        let theme = &self.ts.themes["base16-ocean.dark"];
        let mut h = HighlightLines::new(syntax, theme);

        let mut lines = Vec::new();
        for line in content.lines() {
            let ranges = h.highlight_line(line, &self.ps).unwrap_or_default();
            let mut spans = Vec::new();
            for (style, text) in ranges {
                let color = Color::Rgb(style.foreground.r, style.foreground.g, style.foreground.b);
                spans.push(Span::styled(text.to_string(), Style::default().fg(color)));
            }
            lines.push(Line::from(spans));
        }
        lines
    }
}

// ─── Native Markdown & Byte Format Helpers ───────────────────────────────
fn format_bytes(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{} B", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{:.2} MB", bytes as f64 / (1024.0 * 1024.0))
    }
}

fn render_markdown(text: &str) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    let mut in_code_block = false;

    for raw_line in text.lines() {
        let trimmed = raw_line.trim_start();
        if trimmed.starts_with("```") {
            in_code_block = !in_code_block;
            let lang = trimmed.trim_start_matches('`').trim();
            let label = if in_code_block {
                if lang.is_empty() {
                    " ┌── [code] ".to_string()
                } else {
                    format!(" ┌── [{}] ", lang)
                }
            } else {
                " └──".to_string()
            };
            lines.push(Line::from(Span::styled(label, Style::default().fg(DIM))));
            continue;
        }

        if in_code_block {
            lines.push(Line::from(vec![
                Span::styled(" │ ", Style::default().fg(DIM)),
                Span::styled(raw_line.to_string(), Style::default().fg(FG)),
            ]));
            continue;
        }

        if let Some(h) = trimmed.strip_prefix("# ") {
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                h.to_string(),
                Style::default().fg(FG).add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
            )));
        } else if let Some(h) = trimmed.strip_prefix("## ") {
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                h.to_string(),
                Style::default().fg(FG).add_modifier(Modifier::BOLD),
            )));
        } else if let Some(h) = trimmed.strip_prefix("### ") {
            lines.push(Line::from(Span::styled(
                format!("▶ {}", h),
                Style::default().fg(FG).add_modifier(Modifier::BOLD),
            )));
        } else if let Some(h) = trimmed.strip_prefix("#### ") {
            lines.push(Line::from(Span::styled(
                format!("• {}", h),
                Style::default().fg(DIM).add_modifier(Modifier::BOLD),
            )));
        } else if trimmed.starts_with("- ") || trimmed.starts_with("* ") {
            let item = &trimmed[2..];
            lines.push(Line::from(vec![
                Span::styled("  • ", Style::default().fg(FG).add_modifier(Modifier::BOLD)),
                Span::styled(item.to_string(), Style::default().fg(FG)),
            ]));
        } else if let Some(quote) = trimmed.strip_prefix("> ") {
            lines.push(Line::from(vec![
                Span::styled("  │ ", Style::default().fg(DIM)),
                Span::styled(quote.to_string(), Style::default().fg(DIM).add_modifier(Modifier::ITALIC)),
            ]));
        } else if trimmed.starts_with("---") || trimmed.starts_with("***") {
            lines.push(Line::from(Span::styled("────────────────────────────", Style::default().fg(DIM))));
        } else {
            lines.push(Line::from(Span::styled(raw_line.to_string(), Style::default().fg(FG))));
        }
    }

    lines
}

pub fn render_ui(f: &mut Frame, app: &mut App, highlighter: &SyntaxHighlighter) {
    let size = f.area();

    // Paint full screen pure black
    f.render_widget(Block::new().style(Style::default().bg(BG).fg(FG)), size);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(3),
            Constraint::Length(1),
        ])
        .split(size);

    render_header(f, app, chunks[0]);
    render_columns(f, app, highlighter, chunks[1]);
    render_status_line(f, app, chunks[2]);

    if app.is_adding_user {
        render_user_search_modal(f, app, size);
    }

    if app.is_switching_branch {
        render_branch_modal(f, app, size);
    }

    if app.show_help {
        render_help_modal(f, size);
    }
}

fn render_header(f: &mut Frame, app: &App, area: Rect) {
    let path_str = match &app.level {
        NavigationLevel::Accounts => " 🦆 pato  /  [Accounts]".to_string(),
        NavigationLevel::Repos { owner, .. } => format!(" 🦆 pato  /  {}", owner),
        NavigationLevel::Directory { owner, repo, path } => {
            let branch = app.get_current_branch(owner, repo);
            let b_tag = branch.map(|b| format!(" ({})", b)).unwrap_or_default();
            if path.is_empty() {
                format!(" 🦆 pato  /  {}  /  {}{}", owner, repo, b_tag)
            } else {
                format!(" 🦆 pato  /  {}  /  {}{}  /  {}", owner, repo, b_tag, path)
            }
        }
    };

    let sort_indicator = if matches!(app.level, NavigationLevel::Repos { .. }) {
        format!("  [Sort: {} (s)]", app.repo_sort.label())
    } else {
        "".to_string()
    };

    // Rate-limit display
    let remaining = app.client.rate_limit.remaining.load(Ordering::Relaxed);
    let limit = app.client.rate_limit.limit.load(Ordering::Relaxed);
    let is_low = limit > 0 && (remaining as f64 / limit as f64) < 0.10;

    let auth_status = if !app.client.is_authenticated() {
        Span::styled(" [Unauthenticated: 60/h] ", Style::default().fg(DIM))
    } else {
        Span::raw("")
    };

    let rate_style = if is_low {
        Style::default().fg(FG).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(DIM)
    };

    let rate_span = Span::styled(format!("  API {}/{} ", remaining, limit), rate_style);

    let title_line = Line::from(vec![
        Span::styled(path_str, Style::default().fg(FG).add_modifier(Modifier::BOLD)),
        Span::styled(sort_indicator, Style::default().fg(DIM)),
        auth_status,
        rate_span,
        Span::styled("  (? help, R refresh)", Style::default().fg(DIM)),
    ]);

    let p = Paragraph::new(title_line).style(Style::default().bg(PANEL));
    f.render_widget(p, area);
}

fn render_columns(f: &mut Frame, app: &mut App, highlighter: &SyntaxHighlighter, area: Rect) {
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(20),
            Constraint::Percentage(35),
            Constraint::Percentage(45),
        ])
        .split(area);

    render_left_pane(f, app, cols[0]);
    render_middle_pane(f, app, cols[1]);
    render_right_pane(f, app, highlighter, cols[2]);
}

fn render_left_pane(f: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BORDER_TYPE)
        .border_style(Style::default().fg(DIM))
        .style(Style::default().bg(BG))
        .title(Span::styled(" Context ", Style::default().fg(DIM)));

    let items: Vec<ListItem> = match &app.level {
        NavigationLevel::Accounts => {
            vec![ListItem::new(Span::styled("~ (Root Accounts)", Style::default().fg(DIM)))]
        }
        NavigationLevel::Repos { owner, .. } => {
            app.accounts
                .iter()
                .filter(|a| !matches!(a, AccountEntry::AddNew | AccountEntry::SavedReposSection))
                .map(|acc| {
                    let is_active = acc.name() == owner;
                    let icon = match acc {
                        AccountEntry::User(_) => "👤",
                        AccountEntry::Org(_) => "🏢",
                        AccountEntry::Custom(_) => "🌐",
                        AccountEntry::SavedRepo(_, _) => "⭐",
                        AccountEntry::SavedReposSection | AccountEntry::AddNew => "+",
                    };
                    let st = if is_active {
                        Style::default().fg(FG).add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(DIM)
                    };
                    ListItem::new(Span::styled(format!("{} {}", icon, acc.name()), st))
                })
                .collect()
        }
        NavigationLevel::Directory { owner, repo, path } => {
            if path.is_empty() {
                let sorted_repos = app.get_sorted_and_filtered_repos(owner);
                sorted_repos
                    .iter()
                    .map(|(_, r)| {
                        let is_active = r.name == *repo;
                        let st = if is_active {
                            Style::default().fg(FG).add_modifier(Modifier::BOLD)
                        } else {
                            Style::default().fg(DIM)
                        };
                        ListItem::new(Span::styled(format!("📦 {}", r.name), st))
                    })
                    .collect()
            } else {
                let parent_path = match path.rfind('/') {
                    Some(idx) => &path[..idx],
                    None => "",
                };
                let branch = app.get_current_branch(owner, repo);
                let b_str = branch.as_deref().unwrap_or("");
                let pkey = format!("{}/{}:{}:{}", owner, repo, parent_path, b_str);
                let empty = Vec::new();
                let parent_items = app.dir_cache.get(&pkey).unwrap_or(&empty);
                parent_items
                    .iter()
                    .map(|it| {
                        ListItem::new(Span::styled(format!("📁 {}", it.name), Style::default().fg(DIM)))
                    })
                    .collect()
            }
        }
    };

    let list = List::new(items).block(block);
    f.render_widget(list, area);
}

fn render_middle_pane(f: &mut Frame, app: &mut App, area: Rect) {
    let title = match &app.level {
        NavigationLevel::Accounts => " Accounts & Orgs ".to_string(),
        NavigationLevel::Repos { owner, .. } => format!(" {}'s Repositories ", owner),
        NavigationLevel::Directory { repo, path, .. } => {
            if path.is_empty() {
                format!(" {} / ", repo)
            } else {
                format!(" {} / {}/ ", repo, path)
            }
        }
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BORDER_TYPE)
        .border_style(Style::default().fg(ACCENT))
        .style(Style::default().bg(BG))
        .title(Span::styled(title, Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)));

    let mut list_items = Vec::new();
    let mut is_loading = false;
    let mut pane_error_opt: Option<String> = None;

    match &app.level {
        NavigationLevel::Accounts => {
            let indices = app.filtered_account_indices();
            for &idx in &indices {
                if let Some(acc) = app.accounts.get(idx) {
                    match acc {
                        AccountEntry::User(u) => {
                            list_items.push(ListItem::new(Line::from(vec![
                                Span::styled("👤 ", Style::default().fg(DIM)),
                                Span::styled(&u.login, Style::default().fg(FG)),
                            ])));
                        }
                        AccountEntry::Org(o) => {
                            list_items.push(ListItem::new(Line::from(vec![
                                Span::styled("🏢 ", Style::default().fg(DIM)),
                                Span::styled(&o.login, Style::default().fg(FG)),
                            ])));
                        }
                        AccountEntry::SavedReposSection => {
                            list_items.push(ListItem::new(Line::from(vec![
                                Span::styled("⭐ SAVED REPOSITORIES", Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)),
                            ])));
                        }
                        AccountEntry::SavedRepo(owner, repo) => {
                            list_items.push(ListItem::new(Line::from(vec![
                                Span::styled("⭐ ", Style::default().fg(FG)),
                                Span::styled(format!("{}/{}", owner, repo), Style::default().fg(FG).add_modifier(Modifier::BOLD)),
                                Span::styled(" (saved repo)", Style::default().fg(DIM)),
                            ])));
                        }
                        AccountEntry::Custom(name) => {
                            list_items.push(ListItem::new(Line::from(vec![
                                Span::styled("🌐 ", Style::default().fg(DIM)),
                                Span::styled(name, Style::default().fg(FG)),
                                Span::styled(" (saved)", Style::default().fg(DIM)),
                            ])));
                        }
                        AccountEntry::AddNew => {
                            list_items.push(ListItem::new(Line::from(vec![
                                Span::styled("+ Other user/org/repo...", Style::default().fg(DIM).add_modifier(Modifier::ITALIC)),
                            ])));
                        }
                    }
                }
            }
        }
        NavigationLevel::Repos { owner, .. } => {
            let err_key = format!("repos:{}", owner);
            if let Some(err) = app.pane_errors.get(&err_key) {
                pane_error_opt = Some(err.clone());
            } else {
                let is_fetching = app.loading_keys.get(&err_key).copied().unwrap_or(false);
                let has_cache = app.repos_cache.contains_key(owner);

                if is_fetching && !has_cache {
                    is_loading = true;
                } else {
                    let sorted_repos = app.get_sorted_and_filtered_repos(owner);
                    for (_, repo) in sorted_repos {
                        let priv_icon = if repo.private { "🔒" } else { "  " };
                        let arch_icon = if repo.archived { "📦" } else { "  " };
                        let fork_icon = if repo.fork { "🔱" } else { "  " };
                        let repo_icon = "📁";

                        let lang = repo.language.as_deref().unwrap_or("");
                        let lang_tag = if !lang.is_empty() {
                            format!(" [{}]", lang)
                        } else {
                            "".to_string()
                        };

                        let active_br = app.get_current_branch(owner, &repo.name);
                        let br_tag = active_br.map(|b| format!(" ({})", b)).unwrap_or_default();

                        let line = Line::from(vec![
                            Span::styled(format!("{} ", priv_icon), Style::default().fg(DIM)),
                            Span::styled(format!("{} ", arch_icon), Style::default().fg(DIM)),
                            Span::styled(format!("{} ", fork_icon), Style::default().fg(DIM)),
                            Span::styled(format!("{} ", repo_icon), Style::default().fg(FG)),
                            Span::styled(repo.name.clone(), Style::default().fg(FG)),
                            Span::styled(br_tag, Style::default().fg(DIM)),
                            Span::styled(lang_tag, Style::default().fg(DIM)),
                        ]);
                        list_items.push(ListItem::new(line));
                    }
                }
            }
        }
        NavigationLevel::Directory { owner, repo, path } => {
            let branch = app.get_current_branch(owner, repo);
            let b_str = branch.as_deref().unwrap_or("");
            let key = format!("{}/{}:{}:{}", owner, repo, path, b_str);
            let err_key = format!("dir:{}", key);

            if let Some(err) = app.pane_errors.get(&err_key) {
                pane_error_opt = Some(err.clone());
            } else {
                let is_fetching = app.loading_keys.get(&err_key).copied().unwrap_or(false);
                let has_cache = app.dir_cache.contains_key(&key);

                if is_fetching && !has_cache {
                    is_loading = true;
                } else {
                    let indices = app.filtered_content_indices(&key);
                    if let Some(items) = app.dir_cache.get(&key) {
                        for &idx in &indices {
                            if let Some(item) = items.get(idx) {
                                if item.item_type == "dir" {
                                    let line = Line::from(vec![
                                        Span::styled(format!("📁 {}/", item.name), Style::default().fg(FG).add_modifier(Modifier::BOLD)),
                                    ]);
                                    list_items.push(ListItem::new(line));
                                } else {
                                    let icon = match item.name.rsplit('.').next().unwrap_or("").to_lowercase().as_str() {
                                        "rs" => "🦀",
                                        "py" => "🐍",
                                        "js" | "ts" => "📜",
                                        "md" => "📝",
                                        "json" | "toml" | "yaml" | "yml" => "⚙️",
                                        "png" | "jpg" | "jpeg" | "webp" | "gif" => "🖼️",
                                        _ => "📄",
                                    };
                                    let line = Line::from(vec![
                                        Span::raw(format!("{} ", icon)),
                                        Span::styled(item.name.clone(), Style::default().fg(FG)),
                                    ]);
                                    list_items.push(ListItem::new(line));
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    if let Some(err) = pane_error_opt {
        list_items.clear();
        list_items.push(ListItem::new(Line::from(vec![
            Span::styled("  ⚠️ ", Style::default().fg(FG)),
            Span::styled(err, Style::default().fg(DIM)),
        ])));
        list_items.push(ListItem::new(Span::styled("  (Press 'R' to retry fetch)", Style::default().fg(DIM).add_modifier(Modifier::ITALIC))));
    } else if list_items.is_empty() {
        if is_loading {
            list_items.push(ListItem::new(Span::styled(
                "  ⏳ Loading...",
                Style::default().fg(DIM).add_modifier(Modifier::ITALIC),
            )));
        } else {
            list_items.push(ListItem::new(Span::styled(
                "  (Empty)",
                Style::default().fg(DIM).add_modifier(Modifier::ITALIC),
            )));
        }
    }

    let list = List::new(list_items)
        .block(block)
        .highlight_style(
            Style::default()
                .bg(FG)
                .fg(BG)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("▶ ");

    f.render_stateful_widget(list, area, &mut app.list_state);
}

fn render_right_pane(f: &mut Frame, app: &App, highlighter: &SyntaxHighlighter, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BORDER_TYPE)
        .border_style(Style::default().fg(DIM))
        .style(Style::default().bg(BG))
        .title(Span::styled(" Preview ", Style::default().fg(DIM)));

    let preview_key = match &app.level {
        NavigationLevel::Accounts => {
            let indices = app.filtered_account_indices();
            indices
                .get(app.selected_index())
                .and_then(|&idx| app.accounts.get(idx))
                .and_then(|acc| match acc {
                    AccountEntry::AddNew | AccountEntry::SavedReposSection => None,
                    AccountEntry::SavedRepo(owner, repo) => Some(format!("repo:{}/{}", owner, repo)),
                    _ => Some(format!("acc:{}", acc.name())),
                })
        }
        NavigationLevel::Repos { owner, .. } => {
            let sorted_repos = app.get_sorted_and_filtered_repos(owner);
            sorted_repos
                .get(app.selected_index())
                .map(|(_, r)| {
                    let branch = app.get_current_branch(owner, &r.name);
                    let b_str = branch.as_deref().unwrap_or("");
                    format!("repo:{}/{}:{}", owner, r.name, b_str)
                })
        }
        NavigationLevel::Directory { owner, repo, path } => {
            let branch = app.get_current_branch(owner, repo);
            let b_str = branch.as_deref().unwrap_or("");
            let key = format!("{}/{}:{}:{}", owner, repo, path, b_str);
            let indices = app.filtered_content_indices(&key);
            indices
                .get(app.selected_index())
                .and_then(|&idx| app.dir_cache.get(&key).and_then(|items| items.get(idx)))
                .map(|item| format!("file:{}/{}:{}:{}", owner, repo, item.path, b_str))
        }
    };

    let content = preview_key.as_ref().and_then(|k| app.preview_cache.get(k));

    match content {
        Some(PreviewContent::User(u)) => {
            let mut lines = vec![
                Line::from(vec![
                    Span::styled(&u.login, Style::default().fg(FG).add_modifier(Modifier::BOLD)),
                    Span::styled(format!(" ({})", u.name.as_deref().unwrap_or("No Name")), Style::default().fg(DIM)),
                ]),
                Line::from(""),
            ];

            if let Some(ref bio) = u.bio {
                let clean_bio = bio.replace("\r\n", "\n").replace('\r', "\n");
                let mut first = true;
                for l in clean_bio.lines() {
                    if first {
                        lines.push(Line::from(vec![
                            Span::styled("Bio: ", Style::default().fg(DIM)),
                            Span::styled(l.to_string(), Style::default().fg(FG)),
                        ]));
                        first = false;
                    } else {
                        lines.push(Line::from(vec![
                            Span::raw("     "),
                            Span::styled(l.to_string(), Style::default().fg(FG)),
                        ]));
                    }
                }
            } else {
                lines.push(Line::from(vec![
                    Span::styled("Bio: ", Style::default().fg(DIM)),
                    Span::styled("None", Style::default().fg(FG)),
                ]));
            }

            lines.push(Line::from(""));
            lines.push(Line::from(vec![
                Span::styled("Public Repos: ", Style::default().fg(DIM)),
                Span::styled(u.public_repos.unwrap_or(0).to_string(), Style::default().fg(FG)),
            ]));
            lines.push(Line::from(vec![
                Span::styled("Followers: ", Style::default().fg(DIM)),
                Span::styled(u.followers.unwrap_or(0).to_string(), Style::default().fg(FG)),
                Span::raw(" | "),
                Span::styled("Following: ", Style::default().fg(DIM)),
                Span::styled(u.following.unwrap_or(0).to_string(), Style::default().fg(FG)),
            ]));
            lines.push(Line::from(""));
            lines.push(Line::from(vec![
                Span::styled("URL: ", Style::default().fg(DIM)),
                Span::styled(&u.html_url, Style::default().fg(FG)),
            ]));
            let p = Paragraph::new(lines).block(block).wrap(Wrap { trim: false });
            f.render_widget(p, area);
        }
        Some(PreviewContent::Org(o)) => {
            let role_badge = o.role.as_ref().map(|r| format!("  [Role: {}]", r)).unwrap_or_default();
            let mut lines = vec![
                Line::from(vec![
                    Span::styled("🏢 Organization: ", Style::default().fg(DIM)),
                    Span::styled(&o.login, Style::default().fg(FG).add_modifier(Modifier::BOLD)),
                    Span::styled(role_badge, Style::default().fg(DIM)),
                ]),
                Line::from(""),
            ];

            if let Some(ref name) = o.name {
                lines.push(Line::from(vec![
                    Span::styled("Full Name: ", Style::default().fg(DIM)),
                    Span::styled(name, Style::default().fg(FG)),
                ]));
            }

            lines.push(Line::from(vec![
                Span::styled("Description: ", Style::default().fg(DIM)),
                Span::styled(o.description.as_deref().unwrap_or("No description"), Style::default().fg(FG)),
            ]));

            if let Some(ref loc) = o.location {
                lines.push(Line::from(vec![
                    Span::styled("Location: ", Style::default().fg(DIM)),
                    Span::styled(loc, Style::default().fg(FG)),
                ]));
            }

            if let Some(ref blog) = o.blog {
                lines.push(Line::from(vec![
                    Span::styled("Website: ", Style::default().fg(DIM)),
                    Span::styled(blog, Style::default().fg(FG)),
                ]));
            }

            lines.push(Line::from(vec![
                Span::styled("Public Repos: ", Style::default().fg(DIM)),
                Span::styled(o.public_repos.unwrap_or(0).to_string(), Style::default().fg(FG)),
                Span::raw("  |  "),
                Span::styled("Followers: ", Style::default().fg(DIM)),
                Span::styled(o.followers.unwrap_or(0).to_string(), Style::default().fg(FG)),
            ]));

            if let Some(ref created) = o.created_at {
                lines.push(Line::from(vec![
                    Span::styled("Created At: ", Style::default().fg(DIM)),
                    Span::styled(created, Style::default().fg(FG)),
                ]));
            }

            if let Some(ref url) = o.html_url {
                lines.push(Line::from(vec![
                    Span::styled("URL: ", Style::default().fg(DIM)),
                    Span::styled(url, Style::default().fg(FG)),
                ]));
            }

            let p = Paragraph::new(lines).block(block).wrap(Wrap { trim: false });
            f.render_widget(p, area);
        }
        Some(PreviewContent::Repo { repo, readme }) => {
            let cur_branch = app.get_current_branch(&repo.full_name.split('/').next().unwrap_or(""), &repo.name);
            let br_display = cur_branch.unwrap_or_else(|| repo.default_branch.clone().unwrap_or_else(|| "main".into()));

            let mut lines = vec![
                Line::from(vec![
                    Span::styled("📁 ", Style::default()),
                    Span::styled(&repo.full_name, Style::default().fg(FG).add_modifier(Modifier::BOLD)),
                    Span::styled(format!("  [Branch: {}] (b to switch)", br_display), Style::default().fg(DIM)),
                ]),
                Line::from(""),
                Line::from(vec![
                    Span::styled("Description: ", Style::default().fg(DIM)),
                    Span::styled(repo.description.as_deref().unwrap_or("None"), Style::default().fg(FG)),
                ]),
                Line::from(vec![
                    Span::styled("Language: ", Style::default().fg(DIM)),
                    Span::styled(repo.language.as_deref().unwrap_or("None"), Style::default().fg(FG)),
                    Span::raw("   ⭐ "),
                    Span::styled(repo.stargazers_count.to_string(), Style::default().fg(DIM)),
                    Span::raw("   🔱 "),
                    Span::styled(repo.forks_count.to_string(), Style::default().fg(DIM)),
                ]),
                Line::from(vec![
                    Span::styled("Clone Target: ", Style::default().fg(DIM)),
                    Span::styled(format!("{}/{}", app.start_dir.display(), repo.name), Style::default().fg(FG)),
                ]),
                Line::from("─".repeat(area.width.saturating_sub(2) as usize)),
            ];

            if let Some(rm) = readme {
                lines.push(Line::from(Span::styled("📖 README.md:", Style::default().fg(FG).add_modifier(Modifier::BOLD))));
                lines.push(Line::from(""));
                lines.extend(render_markdown(rm));
            } else {
                lines.push(Line::from(Span::styled("No README.md found.", Style::default().fg(DIM))));
            }

            let p = Paragraph::new(lines)
                .block(block)
                .scroll((app.preview_scroll as u16, 0))
                .wrap(Wrap { trim: false });
            f.render_widget(p, area);
        }
        Some(PreviewContent::Directory(sub_items)) => {
            let mut lines = vec![
                Line::from(Span::styled("📁 Directory Contents:", Style::default().fg(FG).add_modifier(Modifier::BOLD))),
                Line::from(""),
            ];
            for it in sub_items {
                let icon = if it.item_type == "dir" { "📁 " } else { "📄 " };
                let suffix = if it.item_type == "dir" { "/" } else { "" };
                lines.push(Line::from(Span::styled(format!("  {}{}{}", icon, it.name, suffix), Style::default().fg(FG))));
            }
            let p = Paragraph::new(lines)
                .block(block)
                .scroll((app.preview_scroll as u16, 0));
            f.render_widget(p, area);
        }
        Some(PreviewContent::Image { name, width, height, half_block }) => {
            let mut lines = vec![
                Line::from(vec![
                    Span::styled("🖼️ Image: ", Style::default().fg(FG).add_modifier(Modifier::BOLD)),
                    Span::styled(name, Style::default().fg(FG)),
                    Span::styled(format!(" ({}x{} px)", width, height), Style::default().fg(DIM)),
                ]),
                Line::from(""),
            ];

            if let Some(hb) = half_block {
                for row in 0..hb.height {
                    let mut spans = Vec::with_capacity(hb.width as usize);
                    for col in 0..hb.width {
                        let idx = (row as usize) * (hb.width as usize) + (col as usize);
                        if let Some(&(top, bot)) = hb.pixels.get(idx) {
                            spans.push(Span::styled("▀", Style::default().fg(top).bg(bot)));
                        }
                    }
                    lines.push(Line::from(spans));
                }
            }

            let p = Paragraph::new(lines)
                .block(block)
                .scroll((app.preview_scroll as u16, 0));
            f.render_widget(p, area);
        }
        Some(PreviewContent::FileOversized { name, size, raw_url, .. }) => {
            let kb = (*size as f64) / 1024.0;
            let lines = vec![
                Line::from(vec![
                    Span::styled("⚠️ Large File Notice: ", Style::default().fg(FG).add_modifier(Modifier::BOLD)),
                    Span::styled(name, Style::default().fg(FG)),
                ]),
                Line::from(""),
                Line::from(vec![
                    Span::styled("File size: ", Style::default().fg(DIM)),
                    Span::styled(format!("{:.1} KB (exceeds preview cap of 100 KB)", kb), Style::default().fg(FG)),
                ]),
                Line::from(""),
                Line::from(vec![
                    Span::styled("Actions available:", Style::default().fg(FG).add_modifier(Modifier::BOLD)),
                ]),
                Line::from(vec![
                    Span::styled("  • Press 'd' ", Style::default().fg(FG).add_modifier(Modifier::BOLD)),
                    Span::styled(format!("to download directly into {}", app.start_dir.display()), Style::default().fg(DIM)),
                ]),
                Line::from(vec![
                    Span::styled("  • Press 'o' ", Style::default().fg(FG).add_modifier(Modifier::BOLD)),
                    Span::styled("to open and view on GitHub in your browser", Style::default().fg(DIM)),
                ]),
                Line::from(""),
                Line::from(vec![
                    Span::styled("Download URL: ", Style::default().fg(DIM)),
                    Span::styled(raw_url.as_deref().unwrap_or("N/A"), Style::default().fg(DIM)),
                ]),
            ];
            let p = Paragraph::new(lines).block(block).wrap(Wrap { trim: true });
            f.render_widget(p, area);
        }
        Some(PreviewContent::File {
            name,
            content,
            is_binary,
            lines_count,
            size_bytes,
            syntax_name,
        }) => {
            if *is_binary {
                let lines = vec![
                    Line::from(vec![
                        Span::styled("⚠️ Binary File: ", Style::default().fg(FG).add_modifier(Modifier::BOLD)),
                        Span::styled(name, Style::default().fg(FG)),
                    ]),
                    Line::from(""),
                    Line::from(vec![
                        Span::styled("Size: ", Style::default().fg(DIM)),
                        Span::styled(format_bytes(*size_bytes), Style::default().fg(FG)),
                    ]),
                    Line::from(""),
                    Line::from(Span::styled("Binary file preview is omitted.", Style::default().fg(DIM))),
                    Line::from(vec![
                        Span::styled("Press 'd' ", Style::default().fg(FG).add_modifier(Modifier::BOLD)),
                        Span::styled(format!("to download directly into {}", app.start_dir.display()), Style::default().fg(DIM)),
                    ]),
                ];
                let p = Paragraph::new(lines).block(block).wrap(Wrap { trim: true });
                f.render_widget(p, area);
            } else {
                let is_markdown = name.to_lowercase().ends_with(".md");
                let mut lines = Vec::new();

                // Top file metadata summary bar
                let meta_line = Line::from(vec![
                    Span::styled(format!(" 📄 {} ", name), Style::default().fg(FG).add_modifier(Modifier::BOLD).bg(PANEL)),
                    Span::styled("│ ", Style::default().fg(DIM).bg(PANEL)),
                    Span::styled(format!("{} lines ", lines_count), Style::default().fg(DIM).bg(PANEL)),
                    Span::styled("│ ", Style::default().fg(DIM).bg(PANEL)),
                    Span::styled(format!("{} ", format_bytes(*size_bytes)), Style::default().fg(DIM).bg(PANEL)),
                    Span::styled("│ ", Style::default().fg(DIM).bg(PANEL)),
                    Span::styled(format!("{} ", syntax_name), Style::default().fg(FG).bg(PANEL)),
                    Span::styled(
                        if app.show_line_numbers { " [# lines: ON] " } else { " [# lines: OFF] " },
                        Style::default().fg(DIM).bg(PANEL),
                    ),
                ]);
                lines.push(meta_line);
                lines.push(Line::from(""));

                if is_markdown && !app.show_line_numbers {
                    lines.extend(render_markdown(content));
                } else {
                    let highlighted = highlighter.highlight(name, content);
                    let gutter_width = format!("{}", lines_count).len().max(3);

                    for (idx, h_line) in highlighted.into_iter().enumerate() {
                        if app.show_line_numbers {
                            let mut spans = Vec::with_capacity(h_line.spans.len() + 1);
                            spans.push(Span::styled(
                                format!("{:>width$} │ ", idx + 1, width = gutter_width),
                                Style::default().fg(DIM),
                            ));
                            spans.extend(h_line.spans);
                            lines.push(Line::from(spans));
                        } else {
                            lines.push(h_line);
                        }
                    }
                }

                let p = Paragraph::new(lines)
                    .block(block)
                    .scroll((app.preview_scroll as u16, 0));
                f.render_widget(p, area);
            }
        }
        Some(PreviewContent::Error(e)) => {
            let p = Paragraph::new(format!("Preview Error: {}", e))
                .block(block)
                .style(Style::default().fg(DIM));
            f.render_widget(p, area);
        }
        _ => {
            let p = Paragraph::new("⏳ Loading preview...")
                .block(block)
                .style(Style::default().fg(DIM).add_modifier(Modifier::ITALIC));
            f.render_widget(p, area);
        }
    }
}

fn render_status_line(f: &mut Frame, app: &App, area: Rect) {
    let line = if let Some(msg) = app.get_status() {
        Line::from(Span::styled(format!(" 📢 {}", msg), Style::default().fg(FG).add_modifier(Modifier::BOLD)))
    } else if app.is_filtering {
        Line::from(vec![
            Span::styled(" / Filter: ", Style::default().fg(FG).add_modifier(Modifier::BOLD)),
            Span::styled(&app.filter_query, Style::default().fg(FG)),
            Span::styled("█", Style::default().fg(DIM)),
            Span::styled("  (Enter to confirm, Esc to clear)", Style::default().fg(DIM)),
        ])
    } else {
        Line::from(vec![
            Span::styled(" ? ", Style::default().fg(BG).bg(FG).add_modifier(Modifier::BOLD)),
            Span::styled(" help", Style::default().fg(DIM)),
            Span::raw("   "),
            Span::styled("arrows/hjkl", Style::default().fg(FG)),
            Span::styled(":nav ", Style::default().fg(DIM)),
            Span::styled("m", Style::default().fg(FG)),
            Span::styled(":bookmark ", Style::default().fg(DIM)),
            Span::styled("#", Style::default().fg(FG)),
            Span::styled(":lines ", Style::default().fg(DIM)),
            Span::styled("b", Style::default().fg(FG)),
            Span::styled(":branch ", Style::default().fg(DIM)),
            Span::styled("c", Style::default().fg(FG)),
            Span::styled(":clone ", Style::default().fg(DIM)),
            Span::styled("C", Style::default().fg(FG)),
            Span::styled(":copy ", Style::default().fg(DIM)),
            Span::styled("d", Style::default().fg(FG)),
            Span::styled(":dl ", Style::default().fg(DIM)),
            Span::styled("D", Style::default().fg(FG)),
            Span::styled(":url ", Style::default().fg(DIM)),
            Span::styled("r", Style::default().fg(FG)),
            Span::styled(":rm ", Style::default().fg(DIM)),
            Span::styled("R", Style::default().fg(FG)),
            Span::styled(":sync ", Style::default().fg(DIM)),
            Span::styled("s", Style::default().fg(FG)),
            Span::styled(":sort ", Style::default().fg(DIM)),
            Span::styled("u", Style::default().fg(FG)),
            Span::styled(":add ", Style::default().fg(DIM)),
            Span::styled("/", Style::default().fg(FG)),
            Span::styled(":find ", Style::default().fg(DIM)),
            Span::styled("q", Style::default().fg(FG)),
            Span::styled(":quit", Style::default().fg(DIM)),
        ])
    };

    let p = Paragraph::new(line).style(Style::default().bg(PANEL));
    f.render_widget(p, area);
}

fn render_user_search_modal(f: &mut Frame, app: &App, area: Rect) {
    let popup_area = centered_rect(60, 48, area);
    f.render_widget(Clear, popup_area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BORDER_TYPE)
        .border_style(Style::default().fg(ACCENT))
        .style(Style::default().bg(PANEL))
        .title(Span::styled(" Explore & Remember User / Org / Repo ", Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)));

    let is_repo_input = app.add_user_input.contains('/');

    let mut lines = vec![
        Line::from("Type exact username/org, or 'owner/repo' to save a repository directly:"),
        Line::from(""),
        Line::from(vec![
            Span::styled("> ", Style::default().fg(FG).add_modifier(Modifier::BOLD)),
            Span::styled(&app.add_user_input, Style::default().fg(FG).add_modifier(Modifier::BOLD)),
            Span::styled("█", Style::default().fg(DIM)),
        ]),
        Line::from(""),
    ];

    if is_repo_input {
        lines.push(Line::from(vec![
            Span::styled("⭐ Press Enter to save bookmark for repository: ", Style::default().fg(FG)),
            Span::styled(app.add_user_input.trim(), Style::default().fg(FG).add_modifier(Modifier::BOLD)),
        ]));
    } else if !app.search_results.is_empty() {
        lines.push(Line::from(Span::styled("Search suggestions (Tab/Up/Down to select, Enter to load):", Style::default().fg(DIM))));
        for (i, item) in app.search_results.iter().enumerate().take(6) {
            let is_sel = i == app.search_selected;
            let type_tag = item.user_type.as_deref().unwrap_or("User");
            let line = if is_sel {
                Line::from(vec![
                    Span::styled(" ▶ ", Style::default().fg(FG).add_modifier(Modifier::BOLD)),
                    Span::styled(format!("{} ", item.login), Style::default().fg(BG).bg(FG).add_modifier(Modifier::BOLD)),
                    Span::styled(format!(" ({})", type_tag), Style::default().fg(DIM)),
                ])
            } else {
                Line::from(vec![
                    Span::raw("   "),
                    Span::styled(format!("{} ", item.login), Style::default().fg(FG)),
                    Span::styled(format!(" ({})", type_tag), Style::default().fg(DIM)),
                ])
            };
            lines.push(line);
        }
    } else if !app.add_user_input.is_empty() {
        lines.push(Line::from(Span::styled("Press Enter to explore this username/org directly", Style::default().fg(DIM))));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled("(Enter to confirm, Esc to cancel)", Style::default().fg(DIM))));

    let p = Paragraph::new(lines).block(block);
    f.render_widget(p, popup_area);
}

fn render_branch_modal(f: &mut Frame, app: &App, area: Rect) {
    let popup_area = centered_rect(50, 45, area);
    f.render_widget(Clear, popup_area);

    let title = if let Some((_, ref r)) = app.branch_modal_repo {
        format!(" Switch Branch: {} ", r)
    } else {
        " Switch Branch ".to_string()
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BORDER_TYPE)
        .border_style(Style::default().fg(ACCENT))
        .style(Style::default().bg(PANEL))
        .title(Span::styled(title, Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)));

    let mut lines = Vec::new();
    let repo_key = app.branch_modal_repo.as_ref().map(|(o, r)| format!("{}/{}", o, r)).unwrap_or_default();
    let current_br = app.branch_modal_repo.as_ref().and_then(|(o, r)| app.get_current_branch(o, r));

    if let Some(branches) = app.branches_cache.get(&repo_key) {
        lines.push(Line::from(Span::styled("Available branches (Up/Down to navigate, Enter to select):", Style::default().fg(DIM))));
        lines.push(Line::from(""));
        for (i, br) in branches.iter().enumerate() {
            let is_sel = i == app.branch_modal_selected;
            let is_cur = current_br.as_ref().map(|b| b == br).unwrap_or(false);
            let cur_marker = if is_cur { " (current)" } else { "" };

            let line = if is_sel {
                Line::from(vec![
                    Span::styled(" ▶ ", Style::default().fg(FG).add_modifier(Modifier::BOLD)),
                    Span::styled(format!("{} ", br), Style::default().fg(BG).bg(FG).add_modifier(Modifier::BOLD)),
                    Span::styled(cur_marker, Style::default().fg(DIM)),
                ])
            } else {
                Line::from(vec![
                    Span::raw("   "),
                    Span::styled(format!("{} ", br), Style::default().fg(FG)),
                    Span::styled(cur_marker, Style::default().fg(DIM)),
                ])
            };
            lines.push(line);
        }
    } else {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled("  ⏳ Loading branches...", Style::default().fg(DIM).add_modifier(Modifier::ITALIC))));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled("(Press Enter to select, Esc to close)", Style::default().fg(DIM))));

    let p = Paragraph::new(lines).block(block);
    f.render_widget(p, popup_area);
}

fn render_help_modal(f: &mut Frame, area: Rect) {
    let popup_area = centered_rect(62, 74, area);
    f.render_widget(Clear, popup_area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BORDER_TYPE)
        .border_style(Style::default().fg(ACCENT))
        .style(Style::default().bg(PANEL))
        .title(Span::styled(" Keybindings Guidance 🦆 ", Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)));

    let keys = vec![
        ("hjkl / Arrows", "Navigate lists & step into / back"),
        ("J / K", "Scroll preview pane down / up (5 lines)"),
        ("#", "Toggle line numbers on code files (ON / OFF)"),
        ("m", "Bookmark / save current repo (or remove bookmark)"),
        ("r", "Remove saved custom user/org or saved repo from list"),
        ("b", "Switch branch for current repo (remote branches)"),
        ("s", "Cycle repo sort (Updated/Created/A-Z/Status/Archived/Fork/Lang)"),
        ("c", "Git clone selected repo into launch directory"),
        ("C (Shift+c)", "Copy clone URL (.git) to system clipboard"),
        ("d", "Download selected file into current launch directory"),
        ("D (Shift+d)", "Copy Web URL of selected repo, account, or file"),
        ("o", "Open repo / file / account in browser (xdg-open)"),
        ("u", "Explore and save any user, org, or 'owner/repo'"),
        ("R (Shift+r)", "Refresh current level and re-fetch from GitHub"),
        ("/", "Filter / search current list"),
        ("?", "Toggle this keybindings guide"),
        ("q / Esc", "Quit or clear filter / close modal"),
    ];

    let mut lines = Vec::new();
    lines.push(Line::from(Span::styled("Navigation & Commands:", Style::default().fg(FG).add_modifier(Modifier::BOLD))));
    lines.push(Line::from(""));

    for (k, desc) in keys {
        lines.push(Line::from(vec![
            Span::styled(format!("  {:<16}", k), Style::default().fg(FG).add_modifier(Modifier::BOLD)),
            Span::styled(desc, Style::default().fg(DIM)),
        ]));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled("Press Esc or ? to close", Style::default().fg(DIM).add_modifier(Modifier::ITALIC))));

    let p = Paragraph::new(lines).block(block);
    f.render_widget(p, popup_area);
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}
