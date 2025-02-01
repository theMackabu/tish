use git2::{Repository, StatusOptions};

#[derive(Debug)]
pub struct GitStatusInfo {
    pub changed: bool,
    pub unmerged: String,
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
            unmerged: String::new(),
            deleted: String::new(),
            added: String::new(),
            modified: String::new(),
            untracked: String::new(),
            status_string: String::new(),
        }
    }
}

#[derive(Debug)]
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

fn check_upstream_status(repo: &Repository, git_info: &mut GitInfo) -> i32 {
    let (ahead, behind) = match repo.head().ok().and_then(|head| head.resolve().ok()).and_then(|branch| {
        let branch_name = branch.name()?;
        let upstream = repo.branch_upstream_name(branch_name).ok()?;
        let upstream_str = upstream.as_str()?;
        let upstream_ref = repo.find_reference(upstream_str).ok()?;

        match (branch.target(), upstream_ref.target()) {
            (Some(local), Some(upstream)) => repo.graph_ahead_behind(local, upstream).ok(),
            _ => None,
        }
    }) {
        Some((a, b)) => (a, b),
        None => return 1,
    };

    let ahead_str = if ahead > 0 { ahead.to_string() } else { String::new() };
    let behind_str = if behind > 0 { behind.to_string() } else { String::new() };

    let branch_status = if ahead > 0 && behind > 0 {
        "↕".to_string()
    } else if ahead > 0 {
        format!("↑{}", ahead)
    } else if behind > 0 {
        format!("↓{}", behind)
    } else {
        String::new()
    };

    git_info.ahead = ahead_str;
    git_info.behind = behind_str;
    git_info.branch_status = branch_status;

    return 0;
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

    git_info.stash_count = if stash_count > 0 { stash_count.to_string() } else { String::new() };

    return git_info;
}
