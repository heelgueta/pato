use crate::github::{ContentItem, GitHubClient, OrgItem, RepoItem, SearchUserItem, UserInfo};
use ratatui::widgets::ListState;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::mpsc;

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
    pub const ALL: [RepoSort; 7] = [
        RepoSort::PushedDesc,
        RepoSort::CreatedDesc,
        RepoSort::NameAsc,
        RepoSort::Status,
        RepoSort::Archived,
        RepoSort::Fork,
        RepoSort::Language,
    ];

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NavigationLevel {
    Accounts,
    Repos { owner: String, is_user: bool },
    Directory { owner: String, repo: String, path: String },
}

#[derive(Debug, Clone)]
pub enum AccountEntry {
    User(UserInfo),
    Org(OrgItem),
    Custom(String),
    AddNew,
}

impl AccountEntry {
    pub fn name(&self) -> &str {
        match self {
            AccountEntry::User(u) => &u.login,
            AccountEntry::Org(o) => &o.login,
            AccountEntry::Custom(s) => s.as_str(),
            AccountEntry::AddNew => "+ Other user/org...",
        }
    }

    pub fn is_user(&self) -> bool {
        matches!(self, AccountEntry::User(_) | AccountEntry::Custom(_))
    }
}

pub enum AppEvent {
    InitialDataLoaded {
        current_user: Option<UserInfo>,
        orgs: Vec<OrgItem>,
    },
    ReposLoaded {
        owner: String,
        repos: Vec<RepoItem>,
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
        results: Vec<SearchUserItem>,
    },
    Error(String),
}

#[derive(Debug, Clone)]
pub struct HalfBlockImage {
    pub width: u16,
    pub height: u16,
    // (top_color, bottom_color)
    pub pixels: Vec<(ratatui::style::Color, ratatui::style::Color)>,
}

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
        head_preview: Option<String>,
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

    // Loading tracking
    pub loading_keys: HashMap<String, bool>,

    // Sorting
    pub repo_sort: RepoSort,

    // Filtering
    pub is_filtering: bool,
    pub filter_query: String,

    // Adding/searching user dialog
    pub is_adding_user: bool,
    pub add_user_input: String,
    pub search_results: Vec<SearchUserItem>,
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
            loading_keys: HashMap::new(),
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

    pub fn config_path() -> PathBuf {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
        let dir = PathBuf::from(home).join(".config").join("pato");
        let _ = fs::create_dir_all(&dir);
        dir.join("saved_accounts.json")
    }

    pub fn load_saved_accounts(&mut self) {
        let path = Self::config_path();
        if let Ok(content) = fs::read_to_string(&path) {
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
    }

    pub fn persist_saved_accounts(&self) {
        let path = Self::config_path();
        let custom_names: Vec<String> = self
            .accounts
            .iter()
            .filter_map(|a| match a {
                AccountEntry::Custom(s) => Some(s.clone()),
                _ => None,
            })
            .collect();
        if let Ok(json) = serde_json::to_string_pretty(&custom_names) {
            let _ = fs::write(path, json);
        }
    }

    pub fn remove_selected_account(&mut self) {
        if matches!(self.level, NavigationLevel::Accounts) {
            let indices = self.filtered_account_indices();
            if let Some(&idx) = indices.get(self.selected_index()) {
                if let Some(acc) = self.accounts.get(idx) {
                    if let AccountEntry::Custom(name) = acc {
                        let n = name.clone();
                        self.accounts.remove(idx);
                        self.persist_saved_accounts();
                        self.set_status(format!("Removed saved user/org: {}", n));
                        if self.selected_index() >= self.accounts.len().max(1) {
                            self.list_state.select(Some(self.accounts.len().saturating_sub(1)));
                        }
                    } else if matches!(acc, AccountEntry::AddNew) {
                        // ignore
                    } else {
                        self.set_status("Cannot remove authenticated owner or default org");
                    }
                }
            }
        }
    }

    pub fn set_status(&mut self, msg: impl Into<String>) {
        self.status_message = Some((msg.into(), std::time::Instant::now()));
    }

    pub fn get_status(&self) -> Option<&str> {
        if let Some((ref msg, time)) = self.status_message {
            if time.elapsed() < std::time::Duration::from_secs(4) {
                return Some(msg.as_str());
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
            self.repos_cache.get(owner).and_then(|list| {
                list.iter().find(|r| r.name == repo).and_then(|r| r.default_branch.clone())
            })
        })
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
        if !self.branches_cache.contains_key(&repo_key) {
            let client = Arc::clone(&self.client);
            let tx = self.tx.clone();
            tokio::spawn(async move {
                if let Ok(branches) = client.get_branches(&owner, &repo).await {
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
                if matches!(acc, AccountEntry::AddNew) {
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
        self.list_state.select(Some(new_sel as usize));
        self.preview_scroll = 0;
        self.trigger_preview_fetch();
    }

    pub fn step_into(&mut self) {
        match &self.level.clone() {
            NavigationLevel::Accounts => {
                let indices = self.filtered_account_indices();
                if let Some(&idx) = indices.get(self.selected_index()) {
                    if let Some(acc) = self.accounts.get(idx) {
                        if matches!(acc, AccountEntry::AddNew) {
                            self.is_adding_user = true;
                            self.add_user_input.clear();
                            self.search_results.clear();
                            return;
                        }

                        let owner = acc.name().to_string();
                        let is_user = acc.is_user();
                        self.level = NavigationLevel::Repos {
                            owner: owner.clone(),
                            is_user,
                        };
                        self.list_state.select(Some(0));
                        self.filter_query.clear();
                        self.preview_scroll = 0;
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
                    self.list_state.select(Some(0));
                    self.filter_query.clear();
                    self.preview_scroll = 0;
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
                                self.list_state.select(Some(0));
                                self.filter_query.clear();
                                self.preview_scroll = 0;
                                self.fetch_dir_if_needed(owner, repo, &new_path);
                            }
                        }
                    }
                }
            }
        }
    }

    pub fn step_back(&mut self) {
        match &self.level.clone() {
            NavigationLevel::Accounts => {}
            NavigationLevel::Repos { .. } => {
                self.level = NavigationLevel::Accounts;
                self.list_state.select(Some(0));
                self.filter_query.clear();
                self.preview_scroll = 0;
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
                    self.list_state.select(Some(0));
                    self.filter_query.clear();
                    self.preview_scroll = 0;
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
                    self.list_state.select(Some(0));
                    self.filter_query.clear();
                    self.preview_scroll = 0;
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
                    let _ = tx.send(AppEvent::Error(format!("Failed to load repos: {}", err)));
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
                    let _ = tx.send(AppEvent::Error(format!("Failed to load directory: {}", err)));
                }
            }
        });
    }

    pub fn trigger_preview_fetch(&self) {
        match &self.level {
            NavigationLevel::Accounts => {
                let indices = self.filtered_account_indices();
                if let Some(&idx) = indices.get(self.selected_index()) {
                    if let Some(acc) = self.accounts.get(idx) {
                        if matches!(acc, AccountEntry::AddNew) {
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
                                    let _ = tx.send(AppEvent::PreviewLoaded {
                                        key,
                                        content: PreviewContent::User(u),
                                    });
                                }
                            } else {
                                let _ = tx.send(AppEvent::PreviewLoaded {
                                    key,
                                    content: PreviewContent::Org(OrgItem {
                                        login: name,
                                        description: None,
                                    }),
                                });
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
                        let _ = tx.send(AppEvent::PreviewLoaded {
                            key,
                            content: PreviewContent::Repo {
                                repo: repo_clone,
                                readme,
                            },
                        });
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
                                        let _ = tx.send(AppEvent::PreviewLoaded {
                                            key,
                                            content: PreviewContent::Directory(sub_items),
                                        });
                                    }
                                } else {
                                    let ext = item_clone.name.rsplit('.').next().unwrap_or("").to_lowercase();
                                    let is_img = matches!(ext.as_str(), "png" | "jpg" | "jpeg" | "webp" | "gif" | "bmp");

                                    // Oversize check: 100KB cap
                                    if item_clone.size > 100 * 1024 && !is_img {
                                        let _ = tx.send(AppEvent::PreviewLoaded {
                                            key,
                                            content: PreviewContent::FileOversized {
                                                name: item_clone.name,
                                                size: item_clone.size,
                                                head_preview: None,
                                                raw_url: item_clone.download_url,
                                            },
                                        });
                                        return;
                                    }

                                    if let Some(raw_bytes) = client.get_file_content_raw(&o, &r, &item_clone.path, br.as_deref()).await {
                                        if is_img {
                                            if let Ok(img) = image::load_from_memory(&raw_bytes) {
                                                let (w, h) = (img.width(), img.height());
                                                let half = render_image_halfblocks(&img, 60, 24);
                                                let _ = tx.send(AppEvent::PreviewLoaded {
                                                    key,
                                                    content: PreviewContent::Image {
                                                        name: item_clone.name,
                                                        width: w,
                                                        height: h,
                                                        half_block: Some(half),
                                                    },
                                                });
                                                return;
                                            }
                                        }

                                        let is_binary = raw_bytes.contains(&0);
                                        if is_binary {
                                            let _ = tx.send(AppEvent::PreviewLoaded {
                                                key,
                                                content: PreviewContent::File {
                                                    name: item_clone.name,
                                                    content: "Binary file preview is omitted.".into(),
                                                    is_binary: true,
                                                },
                                            });
                                        } else {
                                            let content = String::from_utf8_lossy(&raw_bytes).to_string();
                                            let _ = tx.send(AppEvent::PreviewLoaded {
                                                key,
                                                content: PreviewContent::File {
                                                    name: item_clone.name,
                                                    content,
                                                    is_binary: false,
                                                },
                                            });
                                        }
                                    } else {
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

                            self.set_status(format!("Downloading {}...", file_name));

                            tokio::spawn(async move {
                                if let Some(bytes) = client.get_file_content_raw(&o, &r, &item_p, br.as_deref()).await {
                                    if std::fs::write(&target_path, bytes).is_ok() {
                                        let _ = tx.send(AppEvent::Error(format!("Downloaded -> {}", target_str)));
                                    } else {
                                        let _ = tx.send(AppEvent::Error(format!("Failed to write {}", target_str)));
                                    }
                                } else {
                                    let _ = tx.send(AppEvent::Error("Failed to fetch file content".into()));
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
                self.set_status("Select a repository to clone");
                return;
            }
        };

        let target_dir = self.start_dir.join(&repo_name);
        let target_str = target_dir.display().to_string();
        let clone_url = format!("https://github.com/{}/{}.git", owner, repo_name);

        self.set_status(format!("Cloning {} into {} ...", repo_name, target_str));

        let tx = self.tx.clone();
        tokio::spawn(async move {
            let output = tokio::process::Command::new("git")
                .args(["clone", &clone_url, &target_str])
                .output()
                .await;

            match output {
                Ok(out) if out.status.success() => {
                    let _ = tx.send(AppEvent::Error(format!("Cloned into {}", target_str)));
                }
                Ok(out) => {
                    let err_msg = String::from_utf8_lossy(&out.stderr);
                    let _ = tx.send(AppEvent::Error(format!("Git error: {}", err_msg.trim())));
                }
                Err(e) => {
                    let _ = tx.send(AppEvent::Error(format!("Clone failed: {}", e)));
                }
            }
        });
    }

    pub fn copy_clone_url(&mut self) {
        let url = match &self.level {
            NavigationLevel::Repos { owner, .. } => {
                let sorted_repos = self.get_sorted_and_filtered_repos(owner);
                sorted_repos.get(self.selected_index()).map(|(_, r)| r.clone_url.clone())
            }
            NavigationLevel::Directory { owner, repo, .. } => {
                Some(format!("https://github.com/{}/{}.git", owner, repo))
            }
            _ => None,
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
                    self.accounts.get(idx).map(|acc| format!("https://github.com/{}", acc.name()))
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
                    self.accounts.get(idx).map(|acc| format!("https://github.com/{}", acc.name()))
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

    pub fn add_custom_user(&mut self, username: String) {
        let trimmed = username.trim().to_string();
        if !trimmed.is_empty() && !self.accounts.iter().any(|a| a.name().eq_ignore_ascii_case(&trimmed)) {
            let insert_pos = if self.accounts.is_empty() { 0 } else { self.accounts.len() - 1 };
            self.accounts.insert(insert_pos, AccountEntry::Custom(trimmed.clone()));
            self.persist_saved_accounts();
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
