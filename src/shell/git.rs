use git2::{Repository, StatusOptions};

pub struct GitStatusInfo {
    pub changed: bool,
    pub deleted: String,
    pub added: String,
    pub modified: String,
    pub untracked: String,
    pub status_string: String,
}

impl Default for GitStatusInfo {
    fn default() -> Self {
        Self {
            changed: false,
            deleted: String::new(),
            added: String::new(),
            modified: String::new(),
            untracked: String::new(),
            status_string: String::new(),
        }
    }
}

pub struct GitInfo {
    pub in_repo: bool,
    pub working: GitStatusInfo,
    pub staging: GitStatusInfo,
    pub ahead: String,
    pub behind: String,
    pub stash_count: String,
    pub branch_status: String,
    pub branch_name: String,
}

impl Default for GitInfo {
    fn default() -> Self {
        Self {
            in_repo: false,
            working: GitStatusInfo::default(),
            staging: GitStatusInfo::default(),
            ahead: String::new(),
            behind: String::new(),
            stash_count: String::new(),
            branch_status: String::new(),
            branch_name: String::new(),
        }
    }
}

impl GitInfo {
    pub fn status(&self) -> String {
        let mut status_parts = Vec::new();

        if !self.branch_status.is_empty() {
            status_parts.push(self.branch_status.clone());
        }

        if !self.staging.status_string.is_empty() {
            status_parts.push(self.staging.status_string.trim().to_string());
        }

        if !self.working.status_string.is_empty() {
            status_parts.push(self.working.status_string.trim().to_string());
        }

        if status_parts.is_empty() {
            String::new()
        } else {
            format!("{} ", status_parts.join(" "))
        }
    }
}

fn get_branch_name(repo: &Repository) -> String {
    if let Ok(head) = repo.head() {
        if head.is_branch() {
            if let Some(name) = head.shorthand() {
                return name.to_string();
            }
        } else if let Ok(commit) = head.peel_to_commit() {
            return format!("({:.7})", commit.id());
        }
    }
    "HEAD".to_string()
}

fn check_upstream_status(repo: &Repository, git_info: &mut GitInfo) {
    let (ahead, behind) = match (|| {
        let head = repo.head().ok()?.resolve().ok()?;
        let branch_name = head.name()?;
        let upstream = repo.branch_upstream_name(branch_name).ok()?;
        let upstream_ref = repo.find_reference(upstream.as_str()?).ok()?;

        let local = head.target()?;
        let upstream = upstream_ref.target()?;

        repo.graph_ahead_behind(local, upstream).ok()
    })() {
        Some((a, b)) => (a, b),
        None => return,
    };

    let ahead_str = if ahead > 0 { ahead.to_string() } else { String::new() };
    let behind_str = if behind > 0 { behind.to_string() } else { String::new() };

    let branch_status = match (ahead, behind) {
        (0, 0) => String::new(),
        (a, 0) => format!("↑{a}"),
        (0, b) => format!("↓{b}"),
        (_, _) => "↕".to_string(),
    };

    git_info.ahead = ahead_str;
    git_info.behind = behind_str;
    git_info.branch_status = branch_status;
}

pub fn get_info() -> GitInfo {
    let mut git_info = GitInfo::default();

    let mut repo = match Repository::discover(".") {
        Ok(repo) => repo,
        Err(_) => return git_info,
    };

    git_info.in_repo = true;
    git_info.branch_name = get_branch_name(&repo);

    let mut working_status = GitStatusInfo::default();
    let mut staging_status = GitStatusInfo::default();

    if let Ok(statuses) = repo.statuses(Some(
        StatusOptions::new()
            .include_untracked(true)
            .include_ignored(false)
            .include_unmodified(false)
            .renames_head_to_index(true)
            .renames_index_to_workdir(true),
    )) {
        let mut working_modified = 0;
        let mut working_untracked = 0;
        let mut working_deleted = 0;
        let mut staging_modified = 0;
        let mut staging_added = 0;
        let mut staging_deleted = 0;

        for entry in statuses.iter() {
            let status = entry.status();

            if status.is_wt_modified() {
                working_modified += 1;
            }
            if status.is_wt_new() {
                working_untracked += 1;
            }
            if status.is_wt_deleted() {
                working_deleted += 1;
            }
            if status.is_index_modified() {
                staging_modified += 1;
            }
            if status.is_index_new() {
                staging_added += 1;
            }
            if status.is_index_deleted() {
                staging_deleted += 1;
            }
        }

        if working_modified > 0 {
            working_status.modified = working_modified.to_string();
            working_status.changed = true;
        }
        if working_untracked > 0 {
            working_status.untracked = working_untracked.to_string();
            working_status.changed = true;
        }
        if working_deleted > 0 {
            working_status.deleted = working_deleted.to_string();
            working_status.changed = true;
        }

        if staging_modified > 0 {
            staging_status.modified = staging_modified.to_string();
            staging_status.changed = true;
        }
        if staging_added > 0 {
            staging_status.added = staging_added.to_string();
            staging_status.changed = true;
        }
        if staging_deleted > 0 {
            staging_status.deleted = staging_deleted.to_string();
            staging_status.changed = true;
        }

        let mut working_parts = Vec::new();
        if !working_status.untracked.is_empty() {
            working_parts.push(format!("?{}", working_status.untracked));
        }
        if !working_status.modified.is_empty() {
            working_parts.push(format!("~{}", working_status.modified));
        }
        if !working_status.deleted.is_empty() {
            working_parts.push(format!("-{}", working_status.deleted));
        }

        working_status.status_string = working_parts.join(" ");

        let mut staging_parts = Vec::new();
        if !staging_status.added.is_empty() {
            staging_parts.push(format!("+{}", staging_status.added));
        }
        if !staging_status.modified.is_empty() {
            staging_parts.push(format!("~{}", staging_status.modified));
        }
        if !staging_status.deleted.is_empty() {
            staging_parts.push(format!("-{}", staging_status.deleted));
        }

        staging_status.status_string = staging_parts.join(" ");
    }

    git_info.working = working_status;
    git_info.staging = staging_status;

    check_upstream_status(&repo, &mut git_info);

    let stash_count = {
        let mut count = 0;
        repo.stash_foreach(|_, _, _| {
            count += 1;
            true
        })
        .unwrap_or(());
        count
    };

    git_info.stash_count = if stash_count > 0 { stash_count.to_string() } else { String::from("0") };

    return git_info;
}
