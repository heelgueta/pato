use crate::github::{ContentItem, GitHubClient, OrgItem, RepoItem, UserInfo};
use ratatui::widgets::ListState;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc;

/// Repository sorting options cycled with `s`
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepoSort {
    PushedDesc,
    CreatedDesc,
    NameAsc,
    Status,   // Private first, then public
    Archived, // Archived first, then active
    Fork,     // Own repos first, then forks
    Language,
}

impl RepoSort {
    pub fn label(&self) -> &'static str {
        match self {
            RepoSort::PushedDesc => "Latest Updated",
            RepoSort::CreatedDesc => "Latest Created",
            RepoSort::NameAsc => "A-Z (Name)",
            RepoSort::Status => "Status (Private/Public)",
            RepoSort::Archived => "Archived/Active",
            RepoSort::Fork => "Source/Fork",
            RepoSort::Language => "Language",
        }
    }

    pub fn next(&self) -> Self {
        match self {
            RepoSort::PushedDesc => RepoSort::CreatedDesc,
            RepoSort::CreatedDesc => RepoSort::NameAsc,
            RepoSort::NameAsc => RepoSort::Status,
            RepoSort::Status => RepoSort::Archived,
            RepoSort::Archived => RepoSort::Fork,
            RepoSort::Fork => RepoSort::Language,
            RepoSort::Language => RepoSort::PushedDesc,
        }
    }
}

/// Miller-column navigation levels
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NavigationLevel {
    Accounts,
    Repos { owner: String, is_user: bool },
    Directory { owner: String, repo: String, path: String },
}

/// Account & saved repository root entries
#[derive(Debug, Clone)]
pub enum AccountEntry {
    User(UserInfo),
    Org(OrgItem),
    SavedReposSection,            // Section header: "⭐ Saved Repositories"
    SavedRepo(String, String),    // (owner, repo_name)
    Custom(String),               // Saved user or org handle
    AddNew,                       // "+ Other user/org/repo..."
}

impl AccountEntry {
    pub fn name(&self) -> &str {
        match self {
            AccountEntry::User(u) => &u.login,
            AccountEntry::Org(o) => &o.login,
            AccountEntry::SavedReposSection => "⭐ Saved Repositories",
            AccountEntry::SavedRepo(_owner, repo) => repo.as_str(),
            AccountEntry::Custom(s) => s.as_str(),
            AccountEntry::AddNew => "+ Other user/org/repo...",
        }
    }

    pub fn is_user(&self) -> bool {
        matches!(self, AccountEntry::User(_) | AccountEntry::Custom(_))
    }
}

/// Saved bookmark reference (saved to ~/.config/pato/saved_items.json)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedBookmark {
    pub owner: String,
    pub repo: String,
    pub path: Option<String>,      // None if bookmark is an entire repo
    pub is_file: bool,
}

/// Autocomplete suggestions in search dialog: can be user/org or repository
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SearchSuggestion {
    User {
        login: String,
        user_type: String,
    },
    Repo {
        full_name: String,
        stars: u64,
        language: Option<String>,
    },
}

impl SearchSuggestion {
    pub fn value(&self) -> &str {
        match self {
            SearchSuggestion::User { login, .. } => login.as_str(),
            SearchSuggestion::Repo { full_name, .. } => full_name.as_str(),
        }
    }
}

#[allow(dead_code)]
pub enum AppEvent {
    InitialDataLoaded {
        current_user: Option<UserInfo>,
        orgs: Vec<OrgItem>,
    },
    ReposLoaded {
        owner: String,
        repos: Vec<RepoItem>,
    },
    SingleRepoLoaded {
        repo: RepoItem,
    },
    BranchesLoaded {
        owner: String,
        repo: String,
        branches: Vec<String>,
    },
    DirectoryLoaded {
        owner: String,
        repo: String,
        path: String,
        branch: Option<String>,
        items: Vec<ContentItem>,
    },
    PreviewLoaded {
        key: String,
        content: PreviewContent,
    },
    SearchResultsLoaded {
        query: String,
        results: Vec<SearchSuggestion>,
    },
    ActiveJobFinished(String),
    Error(String),
}

#[derive(Debug, Clone)]
pub struct HalfBlockImage {
    pub width: u16,
    pub height: u16,
    pub pixels: Vec<(ratatui::style::Color, ratatui::style::Color)>,
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub enum PreviewContent {
    Loading,
    User(UserInfo),
    Org(OrgItem),
    Repo {
        repo: RepoItem,
        readme: Option<String>,
    },
    Directory(Vec<ContentItem>),
    File {
        name: String,
        content: String,
        is_binary: bool,
        lines_count: usize,
        size_bytes: u64,
        syntax_name: String,
    },
    Image {
        name: String,
        width: u32,
        height: u32,
        half_block: Option<HalfBlockImage>,
    },
    FileOversized {
        name: String,
        size: u64,
        raw_url: Option<String>,
    },
    Empty,
    Error(String),
}

pub struct App {
    pub client: Arc<GitHubClient>,
    pub current_user: Option<UserInfo>,
    pub accounts: Vec<AccountEntry>,
    pub level: NavigationLevel,
    pub list_state: ListState,
    pub start_dir: PathBuf,

    // Caches
    pub repos_cache: HashMap<String, Vec<RepoItem>>,
    pub dir_cache: HashMap<String, Vec<ContentItem>>,
    pub preview_cache: HashMap<String, PreviewContent>,
    pub branches_cache: HashMap<String, Vec<String>>,
    pub active_branch: HashMap<String, String>, // "owner/repo" -> branch name

    // In-pane error tracking (key -> message)
    pub pane_errors: HashMap<String, String>,

    // Cursor position memory per level key (Yazi-style)
    pub cursor_history: HashMap<String, usize>,

    // Generation counter for dropping stale background preview responses
    pub preview_generation: Arc<AtomicU64>,

    // Active background job notification (e.g. "Cloning pato...", "Downloading index.js...")
    pub active_job: Option<String>,

    // Loading tracking
    pub loading_keys: HashMap<String, bool>,

    // Saved Bookmarks / Stars (persisted in ~/.config/pato/saved_items.json)
    pub saved_bookmarks: Vec<SavedBookmark>,

    // Line number toggle for code files (default false, toggled with '#')
    pub show_line_numbers: bool,

    // Sorting
    pub repo_sort: RepoSort,

    // Filtering
    pub is_filtering: bool,
    pub filter_query: String,

    // Adding/searching user dialog
    pub is_adding_user: bool,
    pub add_user_input: String,
    pub search_results: Vec<SearchSuggestion>,
    pub search_selected: usize,

    // Branch switching dialog
    pub is_switching_branch: bool,
    pub branch_modal_selected: usize,
    pub branch_modal_repo: Option<(String, String)>, // (owner, repo)

    // Help modal
    pub show_help: bool,

    // Status & messages
    pub status_message: Option<(String, std::time::Instant)>,
    pub tx: mpsc::UnboundedSender<AppEvent>,
    pub preview_scroll: usize,
    pub should_quit: bool,
}

impl App {
    pub fn new(tx: mpsc::UnboundedSender<AppEvent>) -> Self {
        let client = Arc::new(GitHubClient::new());
        let mut list_state = ListState::default();
        list_state.select(Some(0));
        let start_dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));

        Self {
            client,
            current_user: None,
            accounts: vec![AccountEntry::AddNew],
            level: NavigationLevel::Accounts,
            list_state,
            start_dir,
            repos_cache: HashMap::new(),
            dir_cache: HashMap::new(),
            preview_cache: HashMap::new(),
            branches_cache: HashMap::new(),
            active_branch: HashMap::new(),
            pane_errors: HashMap::new(),
            cursor_history: HashMap::new(),
            preview_generation: Arc::new(AtomicU64::new(0)),
            active_job: None,
            loading_keys: HashMap::new(),
            saved_bookmarks: Vec::new(),
            show_line_numbers: false,
            repo_sort: RepoSort::PushedDesc,
            is_filtering: false,
            filter_query: String::new(),
            is_adding_user: false,
            add_user_input: String::new(),
            search_results: Vec::new(),
            search_selected: 0,
            is_switching_branch: false,
            branch_modal_selected: 0,
            branch_modal_repo: None,
            show_help: false,
            status_message: None,
            tx,
            preview_scroll: 0,
            should_quit: false,
        }
    }

    pub fn current_level_key(&self) -> String {
        match &self.level {
            NavigationLevel::Accounts => "root:accounts".to_string(),
            NavigationLevel::Repos { owner, .. } => format!("repos:{}", owner),
            NavigationLevel::Directory { owner, repo, path } => {
                let branch = self.get_current_branch(owner, repo);
                let b_str = branch.as_deref().unwrap_or("");
                format!("dir:{}/{}:{}:{}", owner, repo, path, b_str)
            }
        }
    }

    pub fn save_current_cursor(&mut self) {
        let key = self.current_level_key();
        let idx = self.selected_index();
        self.cursor_history.insert(key, idx);
    }

    pub fn restore_cursor_for_current_level(&mut self) {
        let key = self.current_level_key();
        let max_len = self.active_items_count();
        if max_len == 0 {
            self.list_state.select(None);
            return;
        }
        let saved = self.cursor_history.get(&key).copied().unwrap_or(0);
        let valid_idx = saved.min(max_len.saturating_sub(1));
        self.list_state.select(Some(valid_idx));
    }

    pub fn config_dir() -> PathBuf {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
        let dir = PathBuf::from(home).join(".config").join("pato");
        let _ = fs::create_dir_all(&dir);
        dir
    }

    pub fn load_saved_data(&mut self) {
        let dir = Self::config_dir();
        
        // 1. Load saved user & org accounts
        let acc_path = dir.join("saved_accounts.json");
        if let Ok(content) = fs::read_to_string(&acc_path) {
            if let Ok(saved) = serde_json::from_str::<Vec<String>>(&content) {
                for name in saved {
                    let trimmed = name.trim().to_string();
                    if !trimmed.is_empty()
                        && !self.accounts.iter().any(|a| a.name().eq_ignore_ascii_case(&trimmed))
                    {
                        let pos = if self.accounts.is_empty() { 0 } else { self.accounts.len() - 1 };
                        self.accounts.insert(pos, AccountEntry::Custom(trimmed));
                    }
                }
            }
        }

        // 2. Load saved repo/file bookmarks
        let bm_path = dir.join("saved_bookmarks.json");
        if let Ok(content) = fs::read_to_string(&bm_path) {
            if let Ok(saved) = serde_json::from_str::<Vec<SavedBookmark>>(&content) {
                self.saved_bookmarks = saved;
                self.rebuild_accounts_list_with_bookmarks();
            }
        }
    }

    pub fn persist_saved_data(&self) {
        let dir = Self::config_dir();

        // 1. Persist accounts
        let custom_names: Vec<String> = self
            .accounts
            .iter()
            .filter_map(|a| match a {
                AccountEntry::Custom(s) => Some(s.clone()),
                _ => None,
            })
            .collect();
        if let Ok(json) = serde_json::to_string_pretty(&custom_names) {
            let _ = fs::write(dir.join("saved_accounts.json"), json);
        }

        // 2. Persist bookmarks
        if let Ok(json) = serde_json::to_string_pretty(&self.saved_bookmarks) {
            let _ = fs::write(dir.join("saved_bookmarks.json"), json);
        }
    }

    pub fn rebuild_accounts_list_with_bookmarks(&mut self) {
        // Strip previous bookmark sections from accounts list
        self.accounts.retain(|a| !matches!(a, AccountEntry::SavedReposSection | AccountEntry::SavedRepo(_, _)));

        if !self.saved_bookmarks.is_empty() {
            let insert_pos = if self.accounts.is_empty() { 0 } else { self.accounts.len().saturating_sub(1) };
            self.accounts.insert(insert_pos, AccountEntry::SavedReposSection);

            for (i, bm) in self.saved_bookmarks.iter().enumerate() {
                self.accounts.insert(insert_pos + 1 + i, AccountEntry::SavedRepo(bm.owner.clone(), bm.repo.clone()));
            }
        }
    }

    /// Toggle bookmarking current repository (press 'm' for mark / bookmark)
    pub fn toggle_bookmark_current_repo(&mut self) {
        let (owner, repo) = match &self.level {
            NavigationLevel::Repos { owner, .. } => {
                let sorted_repos = self.get_sorted_and_filtered_repos(owner);
                if let Some((_, r)) = sorted_repos.get(self.selected_index()) {
                    (owner.clone(), r.name.clone())
                } else {
                    return;
                }
            }
            NavigationLevel::Directory { owner, repo, .. } => (owner.clone(), repo.clone()),
            _ => {
                self.set_status("Select a repository to bookmark/save");
                return;
            }
        };

        if let Some(pos) = self.saved_bookmarks.iter().position(|b| b.owner == owner && b.repo == repo && b.path.is_none()) {
            self.saved_bookmarks.remove(pos);
            self.rebuild_accounts_list_with_bookmarks();
            self.persist_saved_data();
            self.set_status(format!("Removed bookmark: {}/{}", owner, repo));
        } else {
            self.saved_bookmarks.push(SavedBookmark {
                owner: owner.clone(),
                repo: repo.clone(),
                path: None,
                is_file: false,
            });
            self.rebuild_accounts_list_with_bookmarks();
            self.persist_saved_data();
            self.set_status(format!("Saved bookmark ⭐ {}/{}", owner, repo));
        }
    }

    pub fn remove_selected_account(&mut self) {
        if matches!(self.level, NavigationLevel::Accounts) {
            let indices = self.filtered_account_indices();
            if let Some(&idx) = indices.get(self.selected_index()) {
                if let Some(acc) = self.accounts.get(idx) {
                    match acc {
                        AccountEntry::Custom(name) => {
                            let n = name.clone();
                            self.accounts.remove(idx);
                            self.persist_saved_data();
                            self.set_status(format!("Removed saved user/org: {}", n));
                            if self.selected_index() >= self.accounts.len().max(1) {
                                self.list_state.select(Some(self.accounts.len().saturating_sub(1)));
                            }
                        }
                        AccountEntry::SavedRepo(owner, repo) => {
                            let (o, r) = (owner.clone(), repo.clone());
                            if let Some(pos) = self.saved_bookmarks.iter().position(|b| b.owner == o && b.repo == r) {
                                self.saved_bookmarks.remove(pos);
                                self.rebuild_accounts_list_with_bookmarks();
                                self.persist_saved_data();
                                self.set_status(format!("Removed saved repo: {}/{}", o, r));
                            }
                        }
                        AccountEntry::SavedReposSection | AccountEntry::AddNew => {}
                        _ => {
                            self.set_status("Cannot remove authenticated owner or primary org");
                        }
                    }
                }
            }
        }
    }

    pub fn refresh_current_level(&mut self) {
        match &self.level.clone() {
            NavigationLevel::Accounts => {
                let client = Arc::clone(&self.client);
                let tx = self.tx.clone();
                tokio::spawn(async move {
                    let user_res = client.get_current_user().await.ok();
                    let orgs_res = client.get_user_orgs().await.unwrap_or_default();
                    let _ = tx.send(AppEvent::InitialDataLoaded {
                        current_user: user_res,
                        orgs: orgs_res,
                    });
                });
                self.set_status("Refreshing accounts & orgs...");
            }
            NavigationLevel::Repos { owner, is_user } => {
                self.repos_cache.remove(owner);
                self.pane_errors.remove(&format!("repos:{}", owner));
                self.fetch_repos_if_needed(owner, *is_user);
                self.set_status(format!("Refreshing {}'s repositories...", owner));
            }
            NavigationLevel::Directory { owner, repo, path } => {
                let branch = self.get_current_branch(owner, repo);
                let b_str = branch.as_deref().unwrap_or("");
                let key = format!("{}/{}:{}:{}", owner, repo, path, b_str);
                self.dir_cache.remove(&key);
                self.pane_errors.remove(&format!("dir:{}", key));
                self.fetch_dir_if_needed(owner, repo, path);
                self.set_status(format!("Refreshing {} directory...", repo));
            }
        }
        self.trigger_preview_fetch();
    }

    pub fn set_status(&mut self, msg: impl Into<String>) {
        self.status_message = Some((msg.into(), std::time::Instant::now()));
    }

    pub fn get_status(&self) -> Option<String> {
        if let Some(ref job) = self.active_job {
            return Some(format!("⏳ {}", job));
        }
        if let Some((ref msg, time)) = self.status_message {
            if time.elapsed() < std::time::Duration::from_secs(4) {
                return Some(msg.clone());
            }
        }
        None
    }

    pub fn selected_index(&self) -> usize {
        self.list_state.selected().unwrap_or(0)
    }

    pub fn cycle_sort(&mut self) {
        self.repo_sort = self.repo_sort.next();
        self.set_status(format!("Sort: {}", self.repo_sort.label()));
    }

    pub fn get_current_branch(&self, owner: &str, repo: &str) -> Option<String> {
        let key = format!("{}/{}", owner, repo);
        self.active_branch.get(&key).cloned().or_else(|| {
            self.get_repo_default_branch(owner, repo)
        })
    }

    /// Looks up repository default branch from cache or preview
    pub fn get_repo_default_branch(&self, owner: &str, repo: &str) -> Option<String> {
        if let Some(list) = self.repos_cache.get(owner) {
            if let Some(r) = list.iter().find(|r| r.name.eq_ignore_ascii_case(repo)) {
                return r.default_branch.clone();
            }
        }
        for content in self.preview_cache.values() {
            if let PreviewContent::Repo { repo: r, .. } = content {
                if r.name.eq_ignore_ascii_case(repo) {
                    return r.default_branch.clone();
                }
            }
        }
        None
    }

    /// Checks if a branch is non-main (i.e. not "main", not "master", and not repo's default branch)
    pub fn is_non_main_branch(&self, owner: &str, repo: &str, branch: &str) -> bool {
        let b = branch.trim();
        if b.is_empty() || b.eq_ignore_ascii_case("main") || b.eq_ignore_ascii_case("master") {
            return false;
        }
        if let Some(def) = self.get_repo_default_branch(owner, repo) {
            if b.eq_ignore_ascii_case(&def) {
                return false;
            }
        }
        true
    }

    /// Returns active branch ONLY if user has chosen a non-main / non-default branch
    pub fn get_non_main_branch(&self, owner: &str, repo: &str) -> Option<String> {
        let br = self.get_current_branch(owner, repo)?;
        if self.is_non_main_branch(owner, repo, &br) {
            Some(br)
        } else {
            None
        }
    }

    pub fn set_current_branch(&mut self, owner: &str, repo: &str, branch: String) {
        let key = format!("{}/{}", owner, repo);
        self.active_branch.insert(key, branch.clone());
        self.set_status(format!("Switched branch to {}", branch));
        self.trigger_preview_fetch();
    }

    pub fn open_branch_switcher(&mut self) {
        let (owner, repo) = match &self.level {
            NavigationLevel::Repos { owner, .. } => {
                let sorted_repos = self.get_sorted_and_filtered_repos(owner);
                if let Some((_, r)) = sorted_repos.get(self.selected_index()) {
                    (owner.clone(), r.name.clone())
                } else {
                    return;
                }
            }
            NavigationLevel::Directory { owner, repo, .. } => (owner.clone(), repo.clone()),
            _ => {
                self.set_status("Select a repository to switch branches");
                return;
            }
        };

        self.branch_modal_repo = Some((owner.clone(), repo.clone()));
        self.branch_modal_selected = 0;
        self.is_switching_branch = true;

        let repo_key = format!("{}/{}", owner, repo);
        if let Some(branches) = self.branches_cache.get(&repo_key) {
            if let Some(cur) = self.get_current_branch(&owner, &repo) {
                if let Some(pos) = branches.iter().position(|b| b == &cur) {
                    self.branch_modal_selected = pos;
                }
            }
        } else {
            let client = Arc::clone(&self.client);
            let tx = self.tx.clone();
            let def_br = self.get_repo_default_branch(&owner, &repo);
            tokio::spawn(async move {
                if let Ok(branches) = client.get_branches(&owner, &repo, def_br.as_deref()).await {
                    let _ = tx.send(AppEvent::BranchesLoaded {
                        owner,
                        repo,
                        branches,
                    });
                }
            });
        }
    }

    pub fn filtered_account_indices(&self) -> Vec<usize> {
        self.accounts
            .iter()
            .enumerate()
            .filter(|(_, acc)| {
                if matches!(acc, AccountEntry::AddNew | AccountEntry::SavedReposSection) {
                    return true;
                }
                if self.filter_query.is_empty() {
                    true
                } else {
                    acc.name().to_lowercase().contains(&self.filter_query.to_lowercase())
                }
            })
            .map(|(i, _)| i)
            .collect()
    }

    pub fn get_sorted_and_filtered_repos(&self, owner: &str) -> Vec<(usize, RepoItem)> {
        let empty = Vec::new();
        let repos = self.repos_cache.get(owner).unwrap_or(&empty);
        let mut list: Vec<(usize, RepoItem)> = repos
            .iter()
            .enumerate()
            .filter(|(_, r)| {
                if self.filter_query.is_empty() {
                    true
                } else {
                    let q = self.filter_query.to_lowercase();
                    r.name.to_lowercase().contains(&q)
                        || r.description
                            .as_ref()
                            .map(|d| d.to_lowercase().contains(&q))
                            .unwrap_or(false)
                        || r.language
                            .as_ref()
                            .map(|l| l.to_lowercase().contains(&q))
                            .unwrap_or(false)
                }
            })
            .map(|(i, r)| (i, r.clone()))
            .collect();

        match self.repo_sort {
            RepoSort::PushedDesc => list.sort_by(|a, b| b.1.pushed_at.cmp(&a.1.pushed_at)),
            RepoSort::CreatedDesc => list.sort_by(|a, b| b.1.created_at.cmp(&a.1.created_at)),
            RepoSort::NameAsc => list.sort_by(|a, b| a.1.name.to_lowercase().cmp(&b.1.name.to_lowercase())),
            RepoSort::Status => list.sort_by(|a, b| b.1.private.cmp(&a.1.private).then_with(|| a.1.name.cmp(&b.1.name))),
            RepoSort::Archived => list.sort_by(|a, b| b.1.archived.cmp(&a.1.archived).then_with(|| a.1.name.cmp(&b.1.name))),
            RepoSort::Fork => list.sort_by(|a, b| a.1.fork.cmp(&b.1.fork).then_with(|| a.1.name.cmp(&b.1.name))),
            RepoSort::Language => list.sort_by(|a, b| a.1.language.cmp(&b.1.language).then_with(|| a.1.name.cmp(&b.1.name))),
        }

        list
    }

    pub fn filtered_content_indices(&self, cache_key: &str) -> Vec<usize> {
        let empty = Vec::new();
        let items = self.dir_cache.get(cache_key).unwrap_or(&empty);
        items
            .iter()
            .enumerate()
            .filter(|(_, c)| {
                if self.filter_query.is_empty() {
                    true
                } else {
                    c.name.to_lowercase().contains(&self.filter_query.to_lowercase())
                }
            })
            .map(|(i, _)| i)
            .collect()
    }

    pub fn active_items_count(&self) -> usize {
        match &self.level {
            NavigationLevel::Accounts => self.filtered_account_indices().len(),
            NavigationLevel::Repos { owner, .. } => self.get_sorted_and_filtered_repos(owner).len(),
            NavigationLevel::Directory { owner, repo, path } => {
                let branch = self.get_current_branch(owner, repo);
                let b_str = branch.as_deref().unwrap_or("");
                let key = format!("{}/{}:{}:{}", owner, repo, path, b_str);
                self.filtered_content_indices(&key).len()
            }
        }
    }

    pub fn move_selection(&mut self, delta: isize) {
        let count = self.active_items_count();
        if count == 0 {
            self.list_state.select(None);
            return;
        }

        let current = self.list_state.selected().unwrap_or(0) as isize;
        let mut new_sel = current + delta;
        if new_sel < 0 {
            new_sel = 0;
        }
        if new_sel >= count as isize {
            new_sel = (count - 1) as isize;
        }

        // If landing on section header, skip past it
        if matches!(self.level, NavigationLevel::Accounts) {
            let indices = self.filtered_account_indices();
            if let Some(&idx) = indices.get(new_sel as usize) {
                if matches!(self.accounts.get(idx), Some(AccountEntry::SavedReposSection)) {
                    if delta > 0 && (new_sel + 1) < count as isize {
                        new_sel += 1;
                    } else if delta < 0 && (new_sel - 1) >= 0 {
                        new_sel -= 1;
                    }
                }
            }
        }

        self.list_state.select(Some(new_sel as usize));
        self.save_current_cursor();
        self.preview_scroll = 0;
        self.trigger_preview_fetch();
    }

    pub fn step_into(&mut self) {
        self.save_current_cursor();
        match &self.level.clone() {
            NavigationLevel::Accounts => {
                let indices = self.filtered_account_indices();
                if let Some(&idx) = indices.get(self.selected_index()) {
                    if let Some(acc) = self.accounts.get(idx) {
                        if matches!(acc, AccountEntry::SavedReposSection) {
                            return;
                        }

                        if matches!(acc, AccountEntry::AddNew) {
                            self.is_adding_user = true;
                            self.add_user_input.clear();
                            self.search_results.clear();
                            return;
                        }

                        // Direct jump into a saved repo
                        if let AccountEntry::SavedRepo(owner, repo_name) = acc {
                            let o = owner.clone();
                            let r = repo_name.clone();
                            self.level = NavigationLevel::Directory {
                                owner: o.clone(),
                                repo: r.clone(),
                                path: String::new(),
                            };
                            self.filter_query.clear();
                            self.preview_scroll = 0;
                            self.restore_cursor_for_current_level();
                            self.fetch_dir_if_needed(&o, &r, "");
                            return;
                        }

                        let owner = acc.name().to_string();
                        let is_user = acc.is_user();
                        self.level = NavigationLevel::Repos {
                            owner: owner.clone(),
                            is_user,
                        };
                        self.filter_query.clear();
                        self.preview_scroll = 0;
                        self.restore_cursor_for_current_level();
                        self.fetch_repos_if_needed(&owner, is_user);
                    }
                }
            }
            NavigationLevel::Repos { owner, .. } => {
                let sorted_repos = self.get_sorted_and_filtered_repos(owner);
                if let Some((_, repo)) = sorted_repos.get(self.selected_index()) {
                    let repo_name = repo.name.clone();
                    self.level = NavigationLevel::Directory {
                        owner: owner.clone(),
                        repo: repo_name.clone(),
                        path: String::new(),
                    };
                    self.filter_query.clear();
                    self.preview_scroll = 0;
                    self.restore_cursor_for_current_level();
                    self.fetch_dir_if_needed(owner, &repo_name, "");
                }
            }
            NavigationLevel::Directory { owner, repo, path } => {
                let branch = self.get_current_branch(owner, repo);
                let b_str = branch.as_deref().unwrap_or("");
                let key = format!("{}/{}:{}:{}", owner, repo, path, b_str);
                let indices = self.filtered_content_indices(&key);
                if let Some(&idx) = indices.get(self.selected_index()) {
                    if let Some(items) = self.dir_cache.get(&key) {
                        if let Some(item) = items.get(idx) {
                            if item.item_type == "dir" {
                                let new_path = item.path.clone();
                                self.level = NavigationLevel::Directory {
                                    owner: owner.clone(),
                                    repo: repo.clone(),
                                    path: new_path.clone(),
                                };
                                self.filter_query.clear();
                                self.preview_scroll = 0;
                                self.restore_cursor_for_current_level();
                                self.fetch_dir_if_needed(owner, repo, &new_path);
                            }
                        }
                    }
                }
            }
        }
    }

    pub fn step_back(&mut self) {
        self.save_current_cursor();
        match &self.level.clone() {
            NavigationLevel::Accounts => {}
            NavigationLevel::Repos { .. } => {
                self.level = NavigationLevel::Accounts;
                self.filter_query.clear();
                self.preview_scroll = 0;
                self.restore_cursor_for_current_level();
                self.trigger_preview_fetch();
            }
            NavigationLevel::Directory { owner, repo, path } => {
                let clean = path.trim_matches('/');
                if clean.is_empty() {
                    let is_user = self
                        .current_user
                        .as_ref()
                        .map(|u| u.login.eq_ignore_ascii_case(owner))
                        .unwrap_or(true);
                    self.level = NavigationLevel::Repos {
                        owner: owner.clone(),
                        is_user,
                    };
                    self.filter_query.clear();
                    self.preview_scroll = 0;
                    self.restore_cursor_for_current_level();
                    self.trigger_preview_fetch();
                } else {
                    let parent_path = match clean.rfind('/') {
                        Some(pos) => clean[..pos].to_string(),
                        None => String::new(),
                    };
                    self.level = NavigationLevel::Directory {
                        owner: owner.clone(),
                        repo: repo.clone(),
                        path: parent_path,
                    };
                    self.filter_query.clear();
                    self.preview_scroll = 0;
                    self.restore_cursor_for_current_level();
                    self.trigger_preview_fetch();
                }
            }
        }
    }

    pub fn fetch_repos_if_needed(&mut self, owner: &str, is_user: bool) {
        if self.repos_cache.contains_key(owner) {
            self.trigger_preview_fetch();
            return;
        }

        self.loading_keys.insert(format!("repos:{}", owner), true);
        let client = Arc::clone(&self.client);
        let owner = owner.to_string();
        let tx = self.tx.clone();

        tokio::spawn(async move {
            let res = if is_user {
                let is_auth = client
                    .get_current_user()
                    .await
                    .map(|u| u.login.eq_ignore_ascii_case(&owner))
                    .unwrap_or(false);
                client.get_repos_for_user(&owner, is_auth).await
            } else {
                client.get_repos_for_org(&owner).await
            };

            match res {
                Ok(repos) => {
                    let _ = tx.send(AppEvent::ReposLoaded { owner, repos });
                }
                Err(err) => {
                    let _ = tx.send(AppEvent::Error(format!("repos:{}:{}", owner, err)));
                }
            }
        });
    }

    pub fn fetch_dir_if_needed(&mut self, owner: &str, repo: &str, path: &str) {
        let branch = self.get_current_branch(owner, repo);
        let b_str = branch.as_deref().unwrap_or("").to_string();
        let key = format!("{}/{}:{}:{}", owner, repo, path, b_str);
        if self.dir_cache.contains_key(&key) {
            self.trigger_preview_fetch();
            return;
        }

        self.loading_keys.insert(format!("dir:{}", key), true);
        let client = Arc::clone(&self.client);
        let owner = owner.to_string();
        let repo = repo.to_string();
        let path = path.to_string();
        let branch_opt = branch.clone();
        let tx = self.tx.clone();

        tokio::spawn(async move {
            match client.get_repo_contents(&owner, &repo, &path, branch_opt.as_deref()).await {
                Ok(items) => {
                    let _ = tx.send(AppEvent::DirectoryLoaded {
                        owner,
                        repo,
                        path,
                        branch: branch_opt,
                        items,
                    });
                }
                Err(err) => {
                    let _ = tx.send(AppEvent::Error(format!("dir:{}:{}", key, err)));
                }
            }
        });
    }

    pub fn trigger_preview_fetch(&mut self) {
        let current_gen = self.preview_generation.fetch_add(1, Ordering::SeqCst) + 1;
        let gen_arc = Arc::clone(&self.preview_generation);

        match &self.level {
            NavigationLevel::Accounts => {
                let indices = self.filtered_account_indices();
                if let Some(&idx) = indices.get(self.selected_index()) {
                    if let Some(acc) = self.accounts.get(idx) {
                        if matches!(acc, AccountEntry::AddNew | AccountEntry::SavedReposSection) {
                            return;
                        }

                        // Preview for saved repo from root accounts list
                        if let AccountEntry::SavedRepo(owner, repo) = acc {
                            let key = format!("repo:{}/{}", owner, repo);
                            if self.preview_cache.contains_key(&key) {
                                return;
                            }
                            let client = Arc::clone(&self.client);
                            let tx = self.tx.clone();
                            let o = owner.clone();
                            let r = repo.clone();
                            tokio::spawn(async move {
                                if let Ok(repo_item) = client.get_single_repo(&o, &r).await {
                                    let readme = client.get_readme(&o, &r, None).await;
                                    if gen_arc.load(Ordering::SeqCst) == current_gen {
                                        let _ = tx.send(AppEvent::PreviewLoaded {
                                            key,
                                            content: PreviewContent::Repo {
                                                repo: repo_item,
                                                readme,
                                            },
                                        });
                                    }
                                }
                            });
                            return;
                        }

                        let name = acc.name().to_string();
                        let key = format!("acc:{}", name);
                        if self.preview_cache.contains_key(&key) {
                            return;
                        }

                        let client = Arc::clone(&self.client);
                        let tx = self.tx.clone();
                        let is_user = acc.is_user();

                        tokio::spawn(async move {
                            if is_user {
                                if let Ok(u) = client.get_user_profile(&name).await {
                                    if gen_arc.load(Ordering::SeqCst) == current_gen {
                                        let _ = tx.send(AppEvent::PreviewLoaded {
                                            key,
                                            content: PreviewContent::User(u),
                                        });
                                    }
                                }
                            } else {
                                if let Ok(o) = client.get_org_profile(&name).await {
                                    if gen_arc.load(Ordering::SeqCst) == current_gen {
                                        let _ = tx.send(AppEvent::PreviewLoaded {
                                            key,
                                            content: PreviewContent::Org(o),
                                        });
                                    }
                                }
                            }
                        });
                    }
                }
            }
            NavigationLevel::Repos { owner, .. } => {
                let sorted_repos = self.get_sorted_and_filtered_repos(owner);
                if let Some((_, repo)) = sorted_repos.get(self.selected_index()) {
                    let branch = self.get_current_branch(owner, &repo.name);
                    let b_str = branch.as_deref().unwrap_or("");
                    let key = format!("repo:{}/{}:{}", owner, repo.name, b_str);
                    if self.preview_cache.contains_key(&key) {
                        return;
                    }

                    let client = Arc::clone(&self.client);
                    let tx = self.tx.clone();
                    let repo_clone = repo.clone();
                    let o = owner.to_string();
                    let r = repo.name.clone();
                    let br = branch.clone();

                    tokio::spawn(async move {
                        let readme = client.get_readme(&o, &r, br.as_deref()).await;
                        if gen_arc.load(Ordering::SeqCst) == current_gen {
                            let _ = tx.send(AppEvent::PreviewLoaded {
                                key,
                                content: PreviewContent::Repo {
                                    repo: repo_clone,
                                    readme,
                                },
                            });
                        }
                    });
                }
            }
            NavigationLevel::Directory { owner, repo, path } => {
                let branch = self.get_current_branch(owner, repo);
                let b_str = branch.as_deref().unwrap_or("");
                let dir_key = format!("{}/{}:{}:{}", owner, repo, path, b_str);
                let indices = self.filtered_content_indices(&dir_key);
                if let Some(&idx) = indices.get(self.selected_index()) {
                    if let Some(items) = self.dir_cache.get(&dir_key) {
                        if let Some(item) = items.get(idx) {
                            let key = format!("file:{}/{}:{}:{}", owner, repo, item.path, b_str);
                            if self.preview_cache.contains_key(&key) {
                                return;
                            }

                            let client = Arc::clone(&self.client);
                            let tx = self.tx.clone();
                            let item_clone = item.clone();
                            let o = owner.to_string();
                            let r = repo.to_string();
                            let br = branch.clone();

                            tokio::spawn(async move {
                                if item_clone.item_type == "dir" {
                                    if let Ok(sub_items) = client.get_repo_contents(&o, &r, &item_clone.path, br.as_deref()).await {
                                        if gen_arc.load(Ordering::SeqCst) == current_gen {
                                            let _ = tx.send(AppEvent::PreviewLoaded {
                                                key,
                                                content: PreviewContent::Directory(sub_items),
                                            });
                                        }
                                    }
                                } else {
                                    let ext = item_clone.name.rsplit('.').next().unwrap_or("").to_lowercase();
                                    let is_img = matches!(ext.as_str(), "png" | "jpg" | "jpeg" | "webp" | "gif" | "bmp");

                                    if item_clone.size > 100 * 1024 && !is_img {
                                        if gen_arc.load(Ordering::SeqCst) == current_gen {
                                            let _ = tx.send(AppEvent::PreviewLoaded {
                                                key,
                                                content: PreviewContent::FileOversized {
                                                    name: item_clone.name,
                                                    size: item_clone.size,
                                                    raw_url: item_clone.download_url,
                                                },
                                            });
                                        }
                                        return;
                                    }

                                    if let Some(raw_bytes) = client.get_file_content_raw(&o, &r, &item_clone.path, br.as_deref()).await {
                                        if is_img {
                                            if let Ok(img) = image::load_from_memory(&raw_bytes) {
                                                let (w, h) = (img.width(), img.height());
                                                let half = render_image_halfblocks(&img, 60, 24);
                                                if gen_arc.load(Ordering::SeqCst) == current_gen {
                                                    let _ = tx.send(AppEvent::PreviewLoaded {
                                                        key,
                                                        content: PreviewContent::Image {
                                                            name: item_clone.name,
                                                            width: w,
                                                            height: h,
                                                            half_block: Some(half),
                                                        },
                                                    });
                                                }
                                                return;
                                            }
                                        }

                                        let is_binary = raw_bytes.contains(&0);
                                        if gen_arc.load(Ordering::SeqCst) == current_gen {
                                            if is_binary {
                                                let _ = tx.send(AppEvent::PreviewLoaded {
                                                    key,
                                                    content: PreviewContent::File {
                                                        name: item_clone.name,
                                                        content: "Binary file preview is omitted.".into(),
                                                        is_binary: true,
                                                        lines_count: 0,
                                                        size_bytes: item_clone.size,
                                                        syntax_name: "Binary".into(),
                                                    },
                                                });
                                            } else {
                                                let content = String::from_utf8_lossy(&raw_bytes).to_string();
                                                let lines_count = content.lines().count();
                                                let syntax_name = match ext.as_str() {
                                                    "rs" => "Rust",
                                                    "py" => "Python",
                                                    "js" => "JavaScript",
                                                    "ts" => "TypeScript",
                                                    "md" => "Markdown",
                                                    "json" => "JSON",
                                                    "toml" => "TOML",
                                                    "yaml" | "yml" => "YAML",
                                                    "html" => "HTML",
                                                    "css" => "CSS",
                                                    "sh" | "bash" => "Shell",
                                                    _ => "Plain Text",
                                                }.to_string();

                                                let _ = tx.send(AppEvent::PreviewLoaded {
                                                    key,
                                                    content: PreviewContent::File {
                                                        name: item_clone.name,
                                                        content,
                                                        is_binary: false,
                                                        lines_count,
                                                        size_bytes: item_clone.size,
                                                        syntax_name,
                                                    },
                                                });
                                            }
                                        }
                                    } else if gen_arc.load(Ordering::SeqCst) == current_gen {
                                        let _ = tx.send(AppEvent::PreviewLoaded {
                                            key,
                                            content: PreviewContent::Error("Unable to fetch file content".into()),
                                        });
                                    }
                                }
                            });
                        }
                    }
                }
            }
        }
    }

    pub fn download_selected_file(&mut self) {
        if let NavigationLevel::Directory { owner, repo, path } = &self.level {
            let branch = self.get_current_branch(owner, repo);
            let b_str = branch.as_deref().unwrap_or("");
            let dir_key = format!("{}/{}:{}:{}", owner, repo, path, b_str);
            let indices = self.filtered_content_indices(&dir_key);
            if let Some(&idx) = indices.get(self.selected_index()) {
                if let Some(items) = self.dir_cache.get(&dir_key) {
                    if let Some(item) = items.get(idx) {
                        if item.item_type != "dir" {
                            let client = Arc::clone(&self.client);
                            let o = owner.clone();
                            let r = repo.clone();
                            let item_p = item.path.clone();
                            let file_name = item.name.clone();
                            let target_path = self.start_dir.join(&file_name);
                            let target_str = target_path.display().to_string();
                            let br = branch.clone();
                            let tx = self.tx.clone();

                            self.active_job = Some(format!("Downloading {}...", file_name));

                            tokio::spawn(async move {
                                if let Some(bytes) = client.get_file_content_raw(&o, &r, &item_p, br.as_deref()).await {
                                    if std::fs::write(&target_path, bytes).is_ok() {
                                        let _ = tx.send(AppEvent::ActiveJobFinished(format!("Downloaded -> {}", target_str)));
                                    } else {
                                        let _ = tx.send(AppEvent::ActiveJobFinished(format!("Failed to write {}", target_str)));
                                    }
                                } else {
                                    let _ = tx.send(AppEvent::ActiveJobFinished("Failed to fetch file content".into()));
                                }
                            });
                        }
                    }
                }
            }
        }
    }

    pub fn clone_selected_repo(&mut self) {
        let (owner, repo_name) = match &self.level {
            NavigationLevel::Accounts => {
                let indices = self.filtered_account_indices();
                if let Some(&idx) = indices.get(self.selected_index()) {
                    if let Some(AccountEntry::SavedRepo(o, r)) = self.accounts.get(idx) {
                        (o.clone(), r.clone())
                    } else {
                        self.set_status("Select a repository to clone");
                        return;
                    }
                } else {
                    return;
                }
            }
            NavigationLevel::Repos { owner, .. } => {
                let sorted_repos = self.get_sorted_and_filtered_repos(owner);
                if let Some((_, r)) = sorted_repos.get(self.selected_index()) {
                    (owner.clone(), r.name.clone())
                } else {
                    return;
                }
            }
            NavigationLevel::Directory { owner, repo, .. } => (owner.clone(), repo.clone()),
        };

        let target_dir = self.start_dir.join(&repo_name);
        let target_str = target_dir.display().to_string();
        let clone_url = format!("https://github.com/{}/{}.git", owner, repo_name);

        self.active_job = Some(format!("Cloning {} into {} ...", repo_name, target_str));

        let tx = self.tx.clone();
        tokio::spawn(async move {
            let output = tokio::process::Command::new("git")
                .args(["clone", &clone_url, &target_str])
                .output()
                .await;

            match output {
                Ok(out) if out.status.success() => {
                    let _ = tx.send(AppEvent::ActiveJobFinished(format!("Cloned into {}", target_str)));
                }
                Ok(out) => {
                    let err_msg = String::from_utf8_lossy(&out.stderr);
                    let _ = tx.send(AppEvent::ActiveJobFinished(format!("Git error: {}", err_msg.trim())));
                }
                Err(e) => {
                    let _ = tx.send(AppEvent::ActiveJobFinished(format!("Clone failed: {}", e)));
                }
            }
        });
    }

    pub fn copy_clone_url(&mut self) {
        let url = match &self.level {
            NavigationLevel::Accounts => {
                let indices = self.filtered_account_indices();
                indices.get(self.selected_index()).and_then(|&idx| {
                    if let Some(AccountEntry::SavedRepo(o, r)) = self.accounts.get(idx) {
                        Some(format!("https://github.com/{}/{}.git", o, r))
                    } else {
                        None
                    }
                })
            }
            NavigationLevel::Repos { owner, .. } => {
                let sorted_repos = self.get_sorted_and_filtered_repos(owner);
                sorted_repos.get(self.selected_index()).map(|(_, r)| r.clone_url.clone())
            }
            NavigationLevel::Directory { owner, repo, .. } => {
                Some(format!("https://github.com/{}/{}.git", owner, repo))
            }
        };

        if let Some(u) = url {
            match arboard::Clipboard::new() {
                Ok(mut cb) => {
                    if cb.set_text(u.clone()).is_ok() {
                        self.set_status(format!("Copied Clone URL: {}", u));
                    } else {
                        self.set_status("Failed to set clipboard");
                    }
                }
                Err(_) => {
                    self.set_status(format!("URL: {}", u));
                }
            }
        }
    }

    pub fn copy_current_url(&mut self) {
        let url = match &self.level {
            NavigationLevel::Accounts => {
                let indices = self.filtered_account_indices();
                indices.get(self.selected_index()).and_then(|&idx| {
                    match self.accounts.get(idx) {
                        Some(AccountEntry::SavedRepo(o, r)) => Some(format!("https://github.com/{}/{}", o, r)),
                        Some(acc) => Some(format!("https://github.com/{}", acc.name())),
                        None => None,
                    }
                })
            }
            NavigationLevel::Repos { owner, .. } => {
                let sorted_repos = self.get_sorted_and_filtered_repos(owner);
                sorted_repos.get(self.selected_index()).map(|(_, r)| r.html_url.clone())
            }
            NavigationLevel::Directory { owner, repo, path } => {
                let branch = self.get_current_branch(owner, repo);
                let b_str = branch.as_deref().unwrap_or("main");
                let dir_key = format!("{}/{}:{}:{}", owner, repo, path, branch.as_deref().unwrap_or(""));
                let indices = self.filtered_content_indices(&dir_key);
                indices.get(self.selected_index()).and_then(|&idx| {
                    self.dir_cache.get(&dir_key).and_then(|items| items.get(idx)).and_then(|item| {
                        item.html_url.clone().or_else(|| {
                            Some(format!("https://github.com/{}/{}/blob/{}/{}", owner, repo, b_str, item.path))
                        })
                    })
                })
            }
        };

        if let Some(u) = url {
            match arboard::Clipboard::new() {
                Ok(mut cb) => {
                    if cb.set_text(u.clone()).is_ok() {
                        self.set_status(format!("Copied Web URL: {}", u));
                    } else {
                        self.set_status("Failed to set clipboard");
                    }
                }
                Err(_) => {
                    self.set_status(format!("URL: {}", u));
                }
            }
        }
    }

    pub fn open_in_browser(&mut self) {
        let url = match &self.level {
            NavigationLevel::Accounts => {
                let indices = self.filtered_account_indices();
                indices.get(self.selected_index()).and_then(|&idx| {
                    match self.accounts.get(idx) {
                        Some(AccountEntry::SavedRepo(o, r)) => Some(format!("https://github.com/{}/{}", o, r)),
                        Some(acc) => Some(format!("https://github.com/{}", acc.name())),
                        None => None,
                    }
                })
            }
            NavigationLevel::Repos { owner, .. } => {
                let sorted_repos = self.get_sorted_and_filtered_repos(owner);
                sorted_repos.get(self.selected_index()).map(|(_, r)| r.html_url.clone())
            }
            NavigationLevel::Directory { owner, repo, path } => {
                let branch = self.get_current_branch(owner, repo);
                let b_str = branch.as_deref().unwrap_or("main");
                let dir_key = format!("{}/{}:{}:{}", owner, repo, path, branch.as_deref().unwrap_or(""));
                let indices = self.filtered_content_indices(&dir_key);
                indices.get(self.selected_index()).and_then(|&idx| {
                    self.dir_cache.get(&dir_key).and_then(|items| items.get(idx)).and_then(|item| {
                        item.html_url.clone().or_else(|| {
                            Some(format!("https://github.com/{}/{}/tree/{}/{}", owner, repo, b_str, item.path))
                        })
                    })
                })
            }
        };

        if let Some(u) = url {
            if open::that(&u).is_ok() {
                self.set_status(format!("Opened {}", u));
            } else {
                self.set_status(format!("Failed to open {}", u));
            }
        }
    }

    pub fn add_custom_user_or_repo(&mut self, input: String) {
        let trimmed = input.trim().to_string();
        if trimmed.is_empty() {
            return;
        }

        // If user entered "owner/repo", treat as a direct repo bookmark
        if trimmed.contains('/') {
            let parts: Vec<&str> = trimmed.split('/').collect();
            if parts.len() == 2 && !parts[0].is_empty() && !parts[1].is_empty() {
                let owner = parts[0].to_string();
                let repo = parts[1].to_string();
                if !self.saved_bookmarks.iter().any(|b| b.owner.eq_ignore_ascii_case(&owner) && b.repo.eq_ignore_ascii_case(&repo)) {
                    self.saved_bookmarks.push(SavedBookmark {
                        owner: owner.clone(),
                        repo: repo.clone(),
                        path: None,
                        is_file: false,
                    });
                    self.rebuild_accounts_list_with_bookmarks();
                    self.persist_saved_data();
                    self.set_status(format!("Added saved repo ⭐ {}/{}", owner, repo));
                }
                return;
            }
        }

        // Otherwise treat as a user or organization handle
        if !self.accounts.iter().any(|a| a.name().eq_ignore_ascii_case(&trimmed)) {
            let insert_pos = if self.accounts.is_empty() { 0 } else { self.accounts.len().saturating_sub(1) };
            self.accounts.insert(insert_pos, AccountEntry::Custom(trimmed.clone()));
            self.persist_saved_data();
            self.set_status(format!("Added and remembered: {}", trimmed));
        }
    }
}

pub fn render_image_halfblocks(img: &image::DynamicImage, max_w: u16, max_h: u16) -> HalfBlockImage {
    let pixel_w = max_w as u32;
    let pixel_h = (max_h as u32) * 2;

    let resized = img.resize(pixel_w, pixel_h, image::imageops::FilterType::Triangle);
    let rgba = resized.to_rgba8();
    let (w, h) = rgba.dimensions();

    let term_h = ((h + 1) / 2) as u16;
    let term_w = w as u16;

    let mut pixels = Vec::with_capacity((term_w as usize) * (term_h as usize));

    for row in 0..term_h {
        let top_y = (row as u32) * 2;
        let bot_y = top_y + 1;

        for col in 0..term_w {
            let x = col as u32;

            let top_px = if top_y < h {
                let p = rgba.get_pixel(x, top_y);
                ratatui::style::Color::Rgb(p[0], p[1], p[2])
            } else {
                ratatui::style::Color::Reset
            };

            let bot_px = if bot_y < h {
                let p = rgba.get_pixel(x, bot_y);
                ratatui::style::Color::Rgb(p[0], p[1], p[2])
            } else {
                ratatui::style::Color::Reset
            };

            pixels.push((top_px, bot_px));
        }
    }

    HalfBlockImage {
        width: term_w,
        height: term_h,
        pixels,
    }
}
