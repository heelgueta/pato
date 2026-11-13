use serde::{Deserialize, Serialize};
use std::process::Command;
use std::sync::atomic::{AtomicI64, AtomicU32, Ordering};
use std::sync::Arc;

/// RateLimitState tracks GitHub API limits from response headers (x-ratelimit-*)
#[derive(Debug, Default)]
pub struct RateLimitState {
    pub limit: AtomicU32,
    pub remaining: AtomicU32,
    pub reset_epoch: AtomicI64,
}

impl RateLimitState {
    pub fn update_from_headers(&self, headers: &reqwest::header::HeaderMap) {
        if let Some(val) = headers.get("x-ratelimit-limit").and_then(|v| v.to_str().ok()) {
            if let Ok(num) = val.parse::<u32>() {
                self.limit.store(num, Ordering::Relaxed);
            }
        }
        if let Some(val) = headers.get("x-ratelimit-remaining").and_then(|v| v.to_str().ok()) {
            if let Ok(num) = val.parse::<u32>() {
                self.remaining.store(num, Ordering::Relaxed);
            }
        }
        if let Some(val) = headers.get("x-ratelimit-reset").and_then(|v| v.to_str().ok()) {
            if let Ok(epoch) = val.parse::<i64>() {
                self.reset_epoch.store(epoch, Ordering::Relaxed);
            }
        }
    }
}

/// Asynchronous GitHub client with auto token retrieval via `gh auth token` or `GITHUB_TOKEN`
#[derive(Debug, Clone)]
pub struct GitHubClient {
    client: reqwest::Client,
    pub token: Option<String>,
    pub rate_limit: Arc<RateLimitState>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UserInfo {
    pub login: String,
    pub name: Option<String>,
    pub bio: Option<String>,
    pub public_repos: Option<u64>,
    pub followers: Option<u64>,
    pub following: Option<u64>,
    pub html_url: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct OrgItem {
    pub login: String,
    pub name: Option<String>,
    pub description: Option<String>,
    pub location: Option<String>,
    pub blog: Option<String>,
    pub public_repos: Option<u64>,
    pub followers: Option<u64>,
    pub created_at: Option<String>,
    pub html_url: Option<String>,
    #[serde(default)]
    pub role: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoItem {
    pub name: String,
    pub full_name: String,
    pub description: Option<String>,
    pub private: bool,
    pub fork: bool,
    #[serde(default)]
    pub archived: bool,
    pub html_url: String,
    pub clone_url: String,
    pub ssh_url: String,
    pub stargazers_count: u64,
    pub forks_count: u64,
    pub language: Option<String>,
    pub default_branch: Option<String>,
    pub pushed_at: Option<String>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BranchItem {
    pub name: String,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
pub struct ContentItem {
    pub name: String,
    pub path: String,
    pub sha: String,
    pub size: u64,
    pub html_url: Option<String>,
    pub download_url: Option<String>,
    #[serde(rename = "type")]
    pub item_type: String, // "file", "dir", "symlink", "submodule"
    pub content: Option<String>,
    pub encoding: Option<String>,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
pub struct SearchUserItem {
    pub login: String,
    #[serde(rename = "type")]
    pub user_type: Option<String>, // "User" or "Organization"
    pub html_url: String,
}

#[derive(Debug, Deserialize)]
struct SearchUsersResponse {
    pub items: Vec<SearchUserItem>,
}

impl GitHubClient {
    pub fn new() -> Self {
        // Zero-config token retrieval from environment or official `gh` CLI
        let token = std::env::var("GITHUB_TOKEN").ok().or_else(|| {
            let output = Command::new("gh").args(["auth", "token"]).output().ok()?;
            if output.status.success() {
                let tok = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if !tok.is_empty() {
                    return Some(tok);
                }
            }
            None
        });

        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            reqwest::header::USER_AGENT,
            reqwest::header::HeaderValue::from_static("pato-tui/0.1.0"),
        );
        headers.insert(
            reqwest::header::ACCEPT,
            reqwest::header::HeaderValue::from_static("application/vnd.github.v3+json"),
        );

        if let Some(ref tok) = token {
            if let Ok(val) = reqwest::header::HeaderValue::from_str(&format!("Bearer {}", tok)) {
                headers.insert(reqwest::header::AUTHORIZATION, val);
            }
        }

        let client = reqwest::Client::builder()
            .default_headers(headers)
            .build()
            .unwrap_or_default();

        let rate_limit = Arc::new(RateLimitState {
            limit: AtomicU32::new(if token.is_some() { 5000 } else { 60 }),
            remaining: AtomicU32::new(if token.is_some() { 5000 } else { 60 }),
            reset_epoch: AtomicI64::new(0),
        });

        Self { client, token, rate_limit }
    }

    pub fn is_authenticated(&self) -> bool {
        self.token.is_some()
    }

    pub async fn get_current_user(&self) -> Result<UserInfo, String> {
        let res = self.client.get("https://api.github.com/user").send().await.map_err(|e| e.to_string())?;
        self.rate_limit.update_from_headers(res.headers());
        let res = res.error_for_status().map_err(|e| e.to_string())?;
        res.json::<UserInfo>().await.map_err(|e| e.to_string())
    }

    pub async fn get_user_profile(&self, username: &str) -> Result<UserInfo, String> {
        let url = format!("https://api.github.com/users/{}", username);
        let res = self.client.get(&url).send().await.map_err(|e| e.to_string())?;
        self.rate_limit.update_from_headers(res.headers());
        let res = res.error_for_status().map_err(|e| e.to_string())?;
        res.json::<UserInfo>().await.map_err(|e| e.to_string())
    }

    pub async fn get_org_profile(&self, org: &str) -> Result<OrgItem, String> {
        let url = format!("https://api.github.com/orgs/{}", org);
        let res = self.client.get(&url).send().await.map_err(|e| e.to_string())?;
        self.rate_limit.update_from_headers(res.headers());
        let res = res.error_for_status().map_err(|e| e.to_string())?;
        let mut org_item = res.json::<OrgItem>().await.map_err(|e| e.to_string())?;

        // If authenticated, check membership role
        if self.token.is_some() {
            let member_url = format!("https://api.github.com/user/memberships/orgs/{}", org);
            if let Ok(mres) = self.client.get(&member_url).send().await {
                if let Ok(val) = mres.json::<serde_json::Value>().await {
                    if let Some(r) = val.get("role").and_then(|v| v.as_str()) {
                        org_item.role = Some(r.to_string());
                    }
                }
            }
        }

        Ok(org_item)
    }

    pub async fn get_user_orgs(&self) -> Result<Vec<OrgItem>, String> {
        let res = self
            .client
            .get("https://api.github.com/user/orgs?per_page=100")
            .send()
            .await
            .map_err(|e| e.to_string())?;
        self.rate_limit.update_from_headers(res.headers());
        let res = res.error_for_status().map_err(|e| e.to_string())?;
        res.json::<Vec<OrgItem>>().await.map_err(|e| e.to_string())
    }

    pub async fn search_users(&self, query: &str) -> Result<Vec<SearchUserItem>, String> {
        let url = format!(
            "https://api.github.com/search/users?q={}&per_page=10",
            urlencoding(query)
        );
        let res = self.client.get(&url).send().await.map_err(|e| e.to_string())?;
        self.rate_limit.update_from_headers(res.headers());
        let res = res.error_for_status().map_err(|e| e.to_string())?;
        let resp = res.json::<SearchUsersResponse>().await.map_err(|e| e.to_string())?;
        Ok(resp.items)
    }

    /// Fetches a specific repository by "owner/name"
    pub async fn get_single_repo(&self, owner: &str, repo: &str) -> Result<RepoItem, String> {
        let url = format!("https://api.github.com/repos/{}/{}", owner, repo);
        let res = self.client.get(&url).send().await.map_err(|e| e.to_string())?;
        self.rate_limit.update_from_headers(res.headers());
        let res = res.error_for_status().map_err(|e| e.to_string())?;
        res.json::<RepoItem>().await.map_err(|e| e.to_string())
    }

    // Auto-paginating up to max_items (1,000) using RFC 5988 Link headers
    async fn paginate_repos(&self, initial_url: String, max_items: usize) -> Result<Vec<RepoItem>, String> {
        let mut all_repos = Vec::new();
        let mut next_url = Some(initial_url);

        while let Some(url) = next_url {
            let res = self.client.get(&url).send().await.map_err(|e| e.to_string())?;
            self.rate_limit.update_from_headers(res.headers());
            let link_header = res.headers().get("link").and_then(|h| h.to_str().ok()).map(|s| s.to_string());

            let res = res.error_for_status().map_err(|e| {
                if e.status() == Some(reqwest::StatusCode::NOT_FOUND) {
                    "Repositories not found or access denied.".to_string()
                } else if e.status() == Some(reqwest::StatusCode::FORBIDDEN) {
                    "Rate limit exceeded or SSO forbidden.".to_string()
                } else {
                    e.to_string()
                }
            })?;

            let page_repos = res.json::<Vec<RepoItem>>().await.map_err(|e| e.to_string())?;
            if page_repos.is_empty() {
                break;
            }
            all_repos.extend(page_repos);

            if all_repos.len() >= max_items {
                all_repos.truncate(max_items);
                break;
            }

            next_url = link_header.as_deref().and_then(parse_next_link);
        }

        Ok(all_repos)
    }

    pub async fn get_repos_for_user(
        &self,
        username: &str,
        is_authenticated_user: bool,
    ) -> Result<Vec<RepoItem>, String> {
        let url = if is_authenticated_user {
            "https://api.github.com/user/repos?per_page=100&sort=pushed&affiliation=owner,collaborator".to_string()
        } else {
            format!(
                "https://api.github.com/users/{}/repos?per_page=100&sort=pushed",
                username
            )
        };
        self.paginate_repos(url, 1000).await
    }

    pub async fn get_repos_for_org(&self, org: &str) -> Result<Vec<RepoItem>, String> {
        let url = format!(
            "https://api.github.com/orgs/{}/repos?per_page=100&sort=pushed",
            org
        );
        match self.paginate_repos(url, 1000).await {
            Ok(repos) => Ok(repos),
            Err(_) => self.get_repos_for_user(org, false).await,
        }
    }

    pub async fn get_branches(&self, owner: &str, repo: &str) -> Result<Vec<String>, String> {
        let url = format!(
            "https://api.github.com/repos/{}/{}/branches?per_page=100",
            owner, repo
        );
        let res = self.client.get(&url).send().await.map_err(|e| e.to_string())?;
        self.rate_limit.update_from_headers(res.headers());
        let res = res.error_for_status().map_err(|e| e.to_string())?;
        let branches = res.json::<Vec<BranchItem>>().await.map_err(|e| e.to_string())?;
        Ok(branches.into_iter().map(|b| b.name).collect())
    }

    pub async fn get_repo_contents(
        &self,
        owner: &str,
        repo: &str,
        path: &str,
        branch: Option<&str>,
    ) -> Result<Vec<ContentItem>, String> {
        let clean_path = path.trim_start_matches('/').trim_end_matches('/');
        let ref_param = branch.map(|b| format!("?ref={}", urlencoding(b))).unwrap_or_default();
        let url = if clean_path.is_empty() {
            format!("https://api.github.com/repos/{}/{}/contents{}", owner, repo, ref_param)
        } else {
            format!(
                "https://api.github.com/repos/{}/{}/contents/{}{}",
                owner, repo, clean_path, ref_param
            )
        };

        let res = self.client.get(&url).send().await.map_err(|e| e.to_string())?;
        self.rate_limit.update_from_headers(res.headers());

        let res = res.error_for_status().map_err(|e| {
            if e.status() == Some(reqwest::StatusCode::NOT_FOUND) {
                "Empty repository or folder not found.".to_string()
            } else if e.status() == Some(reqwest::StatusCode::FORBIDDEN) {
                "Rate limit exceeded or permission denied.".to_string()
            } else {
                e.to_string()
            }
        })?;
        
        let value = res.json::<serde_json::Value>().await.map_err(|e| e.to_string())?;
        if let Some(arr) = value.as_array() {
            let mut items: Vec<ContentItem> = serde_json::from_value(serde_json::Value::Array(arr.clone()))
                .unwrap_or_default();
            items.sort_by(|a, b| {
                let a_is_dir = a.item_type == "dir";
                let b_is_dir = b.item_type == "dir";
                match (a_is_dir, b_is_dir) {
                    (true, false) => std::cmp::Ordering::Less,
                    (false, true) => std::cmp::Ordering::Greater,
                    _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
                }
            });
            Ok(items)
        } else if let Ok(single) = serde_json::from_value::<ContentItem>(value) {
            Ok(vec![single])
        } else {
            Ok(vec![])
        }
    }

    pub async fn get_readme(&self, owner: &str, repo: &str, branch: Option<&str>) -> Option<String> {
        let ref_param = branch.map(|b| format!("?ref={}", urlencoding(b))).unwrap_or_default();
        let url = format!("https://api.github.com/repos/{}/{}/readme{}", owner, repo, ref_param);
        let res = self.client.get(&url).send().await.ok()?;
        self.rate_limit.update_from_headers(res.headers());
        if !res.status().is_success() {
            return None;
        }
        let item = res.json::<ContentItem>().await.ok()?;
        if let (Some(content), Some(encoding)) = (item.content, item.encoding) {
            if encoding == "base64" {
                let clean = content.replace('\n', "").replace('\r', "");
                use base64::Engine;
                let decoded = base64::engine::general_purpose::STANDARD.decode(clean).ok()?;
                return String::from_utf8(decoded).ok();
            }
        }
        None
    }

    pub async fn get_file_content_raw(&self, owner: &str, repo: &str, path: &str, branch: Option<&str>) -> Option<Vec<u8>> {
        let clean_path = path.trim_start_matches('/');
        let ref_param = branch.map(|b| format!("?ref={}", urlencoding(b))).unwrap_or_default();
        let url = format!(
            "https://api.github.com/repos/{}/{}/contents/{}{}",
            owner, repo, clean_path, ref_param
        );
        let res = self.client.get(&url).send().await.ok()?;
        self.rate_limit.update_from_headers(res.headers());
        if !res.status().is_success() {
            return None;
        }
        let item = res.json::<ContentItem>().await.ok()?;
        if let (Some(content), Some(encoding)) = (item.content, item.encoding) {
            if encoding == "base64" {
                let clean = content.replace('\n', "").replace('\r', "");
                use base64::Engine;
                return base64::engine::general_purpose::STANDARD.decode(clean).ok();
            }
        }
        if let Some(dl) = item.download_url {
            let res = self.client.get(&dl).send().await.ok()?;
            self.rate_limit.update_from_headers(res.headers());
            return res.bytes().await.ok().map(|b| b.to_vec());
        }
        None
    }
}

fn parse_next_link(link: &str) -> Option<String> {
    for part in link.split(',') {
        let segs: Vec<&str> = part.split(';').map(|s| s.trim()).collect();
        if segs.len() >= 2 && segs[1] == "rel=\"next\"" {
            let url_part = segs[0];
            if url_part.starts_with('<') && url_part.ends_with('>') {
                return Some(url_part[1..url_part.len() - 1].to_string());
            }
        }
    }
    None
}

fn urlencoding(s: &str) -> String {
    let mut encoded = String::new();
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.' || b == b'~' {
            encoded.push(b as char);
        } else {
            encoded.push_str(&format!("%{:02X}", b));
        }
    }
    encoded
}
