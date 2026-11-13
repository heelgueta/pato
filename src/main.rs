mod app;
mod github;
mod ui;

use app::{AccountEntry, App, AppEvent};
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use ui::SyntaxHighlighter;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let (tx, mut rx) = mpsc::unbounded_channel::<AppEvent>();
    let mut app = App::new(tx.clone());
    let highlighter = SyntaxHighlighter::new();

    // Load auto-remembered accounts & bookmarks from ~/.config/pato/
    app.load_saved_data();

    // Initial load: current user & organizations (if authenticated)
    if app.client.is_authenticated() {
        let client = Arc::clone(&app.client);
        let event_tx = tx.clone();
        tokio::spawn(async move {
            let user_res = client.get_current_user().await.ok();
            let orgs_res = client.get_user_orgs().await.unwrap_or_default();
            let _ = event_tx.send(AppEvent::InitialDataLoaded {
                current_user: user_res,
                orgs: orgs_res,
            });
        });
    } else {
        app.set_status("Unauthenticated mode (run `gh auth login` for full private access)");
    }

    let mut preview_debounce_timer: Option<tokio::time::Instant> = None;
    let mut user_search_timer: Option<tokio::time::Instant> = None;

    loop {
        terminal.draw(|f| ui::render_ui(f, &mut app, &highlighter))?;

        if app.should_quit {
            break;
        }

        // Handle async backend events
        while let Ok(event) = rx.try_recv() {
            match event {
                AppEvent::InitialDataLoaded { current_user, orgs } => {
                    let mut new_accounts = Vec::new();
                    if let Some(ref u) = current_user {
                        new_accounts.push(AccountEntry::User(u.clone()));
                    }
                    for org in orgs {
                        new_accounts.push(AccountEntry::Org(org));
                    }
                    // Keep existing custom saved entries
                    for a in &app.accounts {
                        if let AccountEntry::Custom(s) = a {
                            if !new_accounts.iter().any(|na| na.name().eq_ignore_ascii_case(s)) {
                                new_accounts.push(AccountEntry::Custom(s.clone()));
                            }
                        }
                    }
                    new_accounts.push(AccountEntry::AddNew);
                    app.accounts = new_accounts;
                    app.rebuild_accounts_list_with_bookmarks();
                    app.current_user = current_user;
                    app.restore_cursor_for_current_level();
                    app.trigger_preview_fetch();
                }
                AppEvent::ReposLoaded { owner, repos } => {
                    app.loading_keys.remove(&format!("repos:{}", owner));
                    app.pane_errors.remove(&format!("repos:{}", owner));
                    app.repos_cache.insert(owner.clone(), repos);
                    app.restore_cursor_for_current_level();
                    app.trigger_preview_fetch();
                }
                AppEvent::SingleRepoLoaded { repo } => {
                    let owner = repo.full_name.split('/').next().unwrap_or("").to_string();
                    let entry = app.repos_cache.entry(owner).or_default();
                    if !entry.iter().any(|r| r.name == repo.name) {
                        entry.push(repo);
                    }
                }
                AppEvent::BranchesLoaded { owner, repo, branches } => {
                    let key = format!("{}/{}", owner, repo);
                    if let Some((ref o, ref r)) = app.branch_modal_repo {
                        if o == &owner && r == &repo {
                            if let Some(cur) = app.get_current_branch(&owner, &repo) {
                                if let Some(pos) = branches.iter().position(|b| b == &cur) {
                                    app.branch_modal_selected = pos;
                                }
                            }
                        }
                    }
                    app.branches_cache.insert(key, branches);
                }
                AppEvent::DirectoryLoaded {
                    owner,
                    repo,
                    path,
                    branch,
                    items,
                } => {
                    let b_str = branch.as_deref().unwrap_or("");
                    let key = format!("{}/{}:{}:{}", owner, repo, path, b_str);
                    app.loading_keys.remove(&format!("dir:{}", key));
                    app.pane_errors.remove(&format!("dir:{}", key));
                    app.dir_cache.insert(key, items);
                    app.restore_cursor_for_current_level();
                    app.trigger_preview_fetch();
                }
                AppEvent::PreviewLoaded { key, content } => {
                    app.preview_cache.insert(key, content);
                }
                AppEvent::SearchResultsLoaded { query, results } => {
                    if app.add_user_input.trim() == query.trim() {
                        app.search_results = results;
                        app.search_selected = 0;
                    }
                }
                AppEvent::ActiveJobFinished(msg) => {
                    app.active_job = None;
                    app.set_status(msg);
                }
                AppEvent::Error(err) => {
                    // Check if error is prefix-tagged with key for in-pane display
                    if let Some(pos) = err.find(':') {
                        let prefix = &err[..pos];
                        if prefix == "repos" || prefix == "dir" {
                            let rest = &err[pos + 1..];
                            if let Some(second_pos) = rest.find(':') {
                                let key = format!("{}:{}", prefix, &rest[..second_pos]);
                                let msg = &rest[second_pos + 1..];
                                app.loading_keys.remove(&key);
                                app.pane_errors.insert(key, msg.to_string());
                                continue;
                            }
                        }
                    }
                    app.set_status(err);
                }
            }
        }

        // Check preview debouncing
        if let Some(timer) = preview_debounce_timer {
            if tokio::time::Instant::now() >= timer {
                app.trigger_preview_fetch();
                preview_debounce_timer = None;
            }
        }

        // Check user & repo search debouncing
        if let Some(timer) = user_search_timer {
            if tokio::time::Instant::now() >= timer {
                let query = app.add_user_input.trim().to_string();
                if query.len() >= 2 {
                    let client = Arc::clone(&app.client);
                    let event_tx = tx.clone();
                    tokio::spawn(async move {
                        let mut suggestions = Vec::new();

                        if query.contains('/') {
                            // Query has slash, e.g. "owner/repo" -> search repositories
                            if let Ok(repos) = client.search_repos(&query).await {
                                for r in repos {
                                    suggestions.push(app::SearchSuggestion::Repo {
                                        full_name: r.full_name,
                                        stars: r.stargazers_count,
                                        language: r.language,
                                    });
                                }
                            }
                        } else {
                            // Query without slash -> search both users and repositories concurrently
                            let (users_res, repos_res) = tokio::join!(
                                client.search_users(&query),
                                client.search_repos(&query)
                            );

                            if let Ok(users) = users_res {
                                for u in users.into_iter().take(4) {
                                    suggestions.push(app::SearchSuggestion::User {
                                        login: u.login,
                                        user_type: u.user_type.unwrap_or_else(|| "User".into()),
                                    });
                                }
                            }

                            if let Ok(repos) = repos_res {
                                for r in repos.into_iter().take(4) {
                                    suggestions.push(app::SearchSuggestion::Repo {
                                        full_name: r.full_name,
                                        stars: r.stargazers_count,
                                        language: r.language,
                                    });
                                }
                            }
                        }

                        let _ = event_tx.send(AppEvent::SearchResultsLoaded { query, results: suggestions });
                    });
                }
                user_search_timer = None;
            }
        }

        // Poll keyboard events with a small timeout (50ms) for responsive async UI
        if event::poll(Duration::from_millis(50))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    // Help modal open
                    if app.show_help {
                        if matches!(key.code, KeyCode::Esc | KeyCode::Char('?') | KeyCode::Char('q')) {
                            app.show_help = false;
                        }
                        continue;
                    }

                    // Branch switcher modal open
                    if app.is_switching_branch {
                        let repo_key = app.branch_modal_repo.as_ref().map(|(o, r)| format!("{}/{}", o, r)).unwrap_or_default();
                        let count = app.branches_cache.get(&repo_key).map(|b| b.len()).unwrap_or(0);
                        match key.code {
                            KeyCode::Esc => {
                                app.is_switching_branch = false;
                                app.branch_modal_repo = None;
                            }
                            KeyCode::Down | KeyCode::Char('j') => {
                                if count > 0 {
                                    app.branch_modal_selected = (app.branch_modal_selected + 1) % count;
                                }
                            }
                            KeyCode::Up | KeyCode::Char('k') => {
                                if count > 0 {
                                    if app.branch_modal_selected == 0 {
                                        app.branch_modal_selected = count - 1;
                                    } else {
                                        app.branch_modal_selected -= 1;
                                    }
                                }
                            }
                            KeyCode::Enter => {
                                if let (Some((o, r)), Some(branches)) = (&app.branch_modal_repo, app.branches_cache.get(&repo_key)) {
                                    if let Some(selected_branch) = branches.get(app.branch_modal_selected) {
                                        let b = selected_branch.clone();
                                        let owner = o.clone();
                                        let repo = r.clone();
                                        app.set_current_branch(&owner, &repo, b);
                                        if let app::NavigationLevel::Directory { path, .. } = &app.level.clone() {
                                            app.fetch_dir_if_needed(&owner, &repo, path);
                                        }
                                    }
                                }
                                app.is_switching_branch = false;
                                app.branch_modal_repo = None;
                            }
                            _ => {}
                        }
                        continue;
                    }

                    // Modal dialog for adding / searching custom user/org
                    if app.is_adding_user {
                        match key.code {
                            KeyCode::Esc => {
                                app.is_adding_user = false;
                                app.add_user_input.clear();
                                app.search_results.clear();
                            }
                            KeyCode::Down | KeyCode::Tab => {
                                if !app.search_results.is_empty() {
                                    app.search_selected = (app.search_selected + 1) % app.search_results.len().min(6);
                                }
                            }
                            KeyCode::Up | KeyCode::BackTab => {
                                if !app.search_results.is_empty() {
                                    if app.search_selected == 0 {
                                        app.search_selected = app.search_results.len().min(6) - 1;
                                    } else {
                                        app.search_selected -= 1;
                                    }
                                }
                            }
                            KeyCode::Enter => {
                                let name = if !app.search_results.is_empty() && app.search_selected < app.search_results.len() {
                                    app.search_results[app.search_selected].value().to_string()
                                } else {
                                    app.add_user_input.clone()
                                };
                                app.is_adding_user = false;
                                app.add_user_input.clear();
                                app.search_results.clear();
                                if !name.trim().is_empty() {
                                    app.add_custom_user_or_repo(name);
                                }
                            }
                            KeyCode::Backspace => {
                                app.add_user_input.pop();
                                user_search_timer = Some(tokio::time::Instant::now() + Duration::from_millis(250));
                            }
                            KeyCode::Char(c) => {
                                app.add_user_input.push(c);
                                user_search_timer = Some(tokio::time::Instant::now() + Duration::from_millis(250));
                            }
                            _ => {}
                        }
                        continue;
                    }

                    // In filtering mode
                    if app.is_filtering {
                        match key.code {
                            KeyCode::Esc => {
                                app.is_filtering = false;
                                app.filter_query.clear();
                                app.list_state.select(Some(0));
                                app.trigger_preview_fetch();
                            }
                            KeyCode::Enter => {
                                app.is_filtering = false;
                            }
                            KeyCode::Backspace => {
                                app.filter_query.pop();
                                app.list_state.select(Some(0));
                                preview_debounce_timer = Some(tokio::time::Instant::now() + Duration::from_millis(150));
                            }
                            KeyCode::Char(c) => {
                                app.filter_query.push(c);
                                app.list_state.select(Some(0));
                                preview_debounce_timer = Some(tokio::time::Instant::now() + Duration::from_millis(150));
                            }
                            _ => {}
                        }
                        continue;
                    }

                    // Normal Navigation Mode
                    match key.code {
                        KeyCode::Char('q') => {
                            app.should_quit = true;
                        }
                        KeyCode::Char('?') => {
                            app.show_help = true;
                        }
                        KeyCode::Esc => {
                            if !app.filter_query.is_empty() {
                                app.filter_query.clear();
                                app.list_state.select(Some(0));
                                app.trigger_preview_fetch();
                            } else {
                                app.step_back();
                            }
                        }
                        KeyCode::Char('j') | KeyCode::Down => {
                            app.move_selection(1);
                            preview_debounce_timer = Some(tokio::time::Instant::now() + Duration::from_millis(150));
                        }
                        KeyCode::Char('k') | KeyCode::Up => {
                            app.move_selection(-1);
                            preview_debounce_timer = Some(tokio::time::Instant::now() + Duration::from_millis(150));
                        }
                        KeyCode::Char('l') | KeyCode::Right | KeyCode::Enter => {
                            app.step_into();
                        }
                        KeyCode::Char('h') | KeyCode::Left | KeyCode::Backspace => {
                            app.step_back();
                        }
                        KeyCode::Char('/') => {
                            app.is_filtering = true;
                        }
                        KeyCode::Char('b') => {
                            app.open_branch_switcher();
                        }
                        KeyCode::Char('m') => {
                            app.toggle_bookmark_current_repo();
                        }
                        KeyCode::Char('#') => {
                            app.show_line_numbers = !app.show_line_numbers;
                            let state = if app.show_line_numbers { "ON" } else { "OFF" };
                            app.set_status(format!("Line numbers: {}", state));
                        }
                        KeyCode::Char('r') => {
                            app.remove_selected_account();
                        }
                        KeyCode::Char('R') => {
                            app.refresh_current_level();
                        }
                        KeyCode::Char('s') => {
                            app.cycle_sort();
                        }
                        KeyCode::Char('d') => {
                            app.download_selected_file();
                        }
                        KeyCode::Char('D') => {
                            app.copy_current_url();
                        }
                        KeyCode::Char('c') => {
                            app.clone_selected_repo();
                        }
                        KeyCode::Char('C') => {
                            app.copy_clone_url();
                        }
                        KeyCode::Char('u') => {
                            app.is_adding_user = true;
                            app.add_user_input.clear();
                            app.search_results.clear();
                        }
                        KeyCode::Char('o') => {
                            app.open_in_browser();
                        }
                        KeyCode::Char('J') => {
                            app.preview_scroll = app.preview_scroll.saturating_add(5);
                        }
                        KeyCode::Char('K') => {
                            app.preview_scroll = app.preview_scroll.saturating_sub(5);
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    Ok(())
}
