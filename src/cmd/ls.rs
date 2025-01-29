use crate::{os::user::get_user_by_uid, prelude::*};
use anyhow::Result;
use chrono::{DateTime, Local, TimeZone};

use std::{
    fs,
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
    process::ExitCode,
    time::{SystemTime, UNIX_EPOCH},
};

struct Entry {
    size: String,
    nlink: String,
    modified: String,
    username: String,
    styled_name: String,
}

const RESET: &str = "\x1b[0m";
const BOLD: &str = "\x1b[1m";
const ITALIC: &str = "\x1b[3m";
const YELLOW: &str = "\x1b[33m";
const BLUE: &str = "\x1b[34m";
const CYAN: &str = "\x1b[36m";
const GREEN: &str = "\x1b[32m";
const RED: &str = "\x1b[31m";
const MAGENTA: &str = "\x1b[35m";
const BRIGHT_YELLOW: &str = "\x1b[93m";
const LIGHT_PINK: &str = "\x1b[38;5;218m";
const LIGHT_CYAN: &str = "\x1b[38;5;159m";
const SILVER: &str = "\x1b[38;5;250m";

pub fn run(args: &Vec<String>) -> Result<ExitCode> {
    let mut show_all = false;
    let mut path = PathBuf::from(".");

    argument! {
        args: args.into_iter(),
        options: {
            a => show_all = true,
            h => {
                print_usage();
                return Ok(ExitCode::SUCCESS);
            }
        },
        command: |arg| {
            path = PathBuf::from(arg)
        },
        on_invalid: |c| {
            eprintln!("Unknown option: -{c}");
            print_usage();

        }
    }

    let (dirs, files, symlinks) = read_directory(&path, show_all)?;
    print_entries("DIRECTORIES", &dirs, true);
    print_entries("FILES", &files, false);
    print_entries("SYMLINKS", &symlinks, false);

    Ok(ExitCode::SUCCESS)
}

fn style_text(text: &str, style: &[&str]) -> String { format!("{}{}{}", style.join(""), text, RESET) }

fn print_usage() {
    println!("usage: ls [-ah] [file ...]");
}

fn print_entries(category: &str, entries: &[Entry], is_dir: bool) {
    if !entries.is_empty() {
        println!("\n{}", style_text(category, &[BOLD, YELLOW]));

        let max_size_width = entries.iter().map(|e| e.size.len()).max().unwrap_or(0).max(4);
        let max_nlink_width = entries.iter().map(|e| e.nlink.len()).max().unwrap_or(0).max(5);
        let max_modified_width = entries.iter().map(|e| e.modified.len()).max().unwrap_or(0).max(8);
        let max_username_width = entries.iter().map(|e| e.username.len()).max().unwrap_or(0).max(8);

        if is_dir {
            println!(
                "  {:width_username$}  {:>width_nlink$}  {:width_modified$}  {}",
                style_text("Owner", &[ITALIC, BRIGHT_YELLOW]),
                style_text("Links", &[ITALIC, BRIGHT_YELLOW]),
                style_text("Modified", &[ITALIC, BRIGHT_YELLOW]),
                style_text("Name", &[ITALIC, BRIGHT_YELLOW]),
                width_nlink = max_nlink_width,
                width_modified = max_modified_width,
                width_username = max_username_width
            );

            for entry in entries {
                println!(
                    "  {:width_username$}  {:>width_nlink$}  {:width_modified$}  {}",
                    style_text(&entry.username, &[LIGHT_PINK]),
                    style_text(&entry.nlink, &[LIGHT_CYAN]),
                    style_text(&entry.modified, &[SILVER]),
                    entry.styled_name,
                    width_username = max_username_width,
                    width_nlink = max_nlink_width,
                    width_modified = max_modified_width,
                );
            }
        } else {
            println!(
                "  {:width_username$}  {:>width_size$}  {:width_modified$}  {}",
                style_text("Owner", &[ITALIC, BRIGHT_YELLOW]),
                style_text("Size", &[ITALIC, BRIGHT_YELLOW]),
                style_text("Modified", &[ITALIC, BRIGHT_YELLOW]),
                style_text("Name", &[ITALIC, BRIGHT_YELLOW]),
                width_size = max_size_width,
                width_modified = max_modified_width,
                width_username = max_username_width
            );

            for entry in entries {
                println!(
                    "  {:width_username$}  {:>width_size$}  {:width_modified$}  {}",
                    style_text(&entry.username, &[LIGHT_PINK]),
                    style_text(&entry.size, &[LIGHT_CYAN]),
                    style_text(&entry.modified, &[SILVER]),
                    entry.styled_name,
                    width_username = max_username_width,
                    width_size = max_size_width,
                    width_modified = max_modified_width,
                );
            }
        }
    }
}

fn read_directory(path: &Path, show_all: bool) -> std::io::Result<(Vec<Entry>, Vec<Entry>, Vec<Entry>)> {
    let mut entries: Vec<_> = fs::read_dir(path)?.filter_map(Result::ok).filter(|entry| show_all || !is_hidden(entry)).collect();

    let mut dirs = Vec::new();
    let mut files = Vec::new();
    let mut symlinks = Vec::new();

    entries.sort_by(|a, b| a.file_name().cmp(&b.file_name()));

    for entry in entries {
        let metadata = entry.metadata()?;
        let formatted_entry = format_entry(&entry, &metadata)?;

        if metadata.is_dir() {
            dirs.push(formatted_entry);
        } else if metadata.is_symlink() {
            symlinks.push(formatted_entry);
        } else if metadata.is_file() {
            files.push(formatted_entry);
        }
    }

    Ok((dirs, files, symlinks))
}

fn format_entry(entry: &fs::DirEntry, metadata: &fs::Metadata) -> std::io::Result<Entry> {
    let nlink = metadata.nlink().to_string();
    let file_name = entry.file_name().into_string().unwrap_or_default();
    let size = format_size(metadata.len());
    let modified = format_time(metadata.modified()?);
    let styled_name = style_name(&entry, &metadata, &file_name);
    let username = get_username(metadata.uid());

    Ok(Entry {
        size,
        nlink,
        modified,
        username,
        styled_name,
    })
}

fn style_name(entry: &fs::DirEntry, metadata: &fs::Metadata, name: &str) -> String {
    let mode = metadata.mode();
    if metadata.is_dir() {
        if mode & 0o002 != 0 {
            style_text(name, &[BOLD, YELLOW])
        } else {
            style_text(name, &[BOLD, BLUE])
        }
    } else if metadata.is_symlink() {
        style_text(name, &[BOLD, CYAN])
    } else if mode & 0o111 != 0 {
        style_text(name, &[BOLD, GREEN])
    } else if is_archive(&entry.path()) {
        style_text(name, &[BOLD, RED])
    } else if is_media(&entry.path()) {
        style_text(name, &[BOLD, MAGENTA])
    } else {
        name.to_string()
    }
}

fn get_username(uid: u32) -> String { get_user_by_uid(uid).map(|user| user.name().to_string_lossy().into_owned()).unwrap_or_else(|| uid.to_string()) }

fn format_size(size: u64) -> String {
    if size >= 1024 * 1024 * 1024 {
        format!("{:>5.1}gb", size as f64 / (1024.0 * 1024.0 * 1024.0))
    } else if size >= 1024 * 1024 {
        format!("{:>5.1}mb", size as f64 / (1024.0 * 1024.0))
    } else if size >= 1024 {
        format!("{:>5.1}kb", size as f64 / 1024.0)
    } else {
        format!("{:>6}b", size)
    }
}

fn format_time(time: SystemTime) -> String {
    let duration = time.duration_since(UNIX_EPOCH).unwrap();
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();

    let secs = duration.as_secs();
    let datetime: DateTime<Local> = Local.timestamp_opt(secs as i64, 0).unwrap();

    if secs > now - 15_552_000 { datetime.format("%b %e %I:%M%p") } else { datetime.format("%b %e %Y") }
        .to_string()
        .replace("AM", "am")
        .replace("PM", "pm")
}

fn is_hidden(entry: &fs::DirEntry) -> bool { entry.file_name().as_encoded_bytes().first().map(|&b| b == b'.').unwrap_or(false) }

fn is_archive(path: &Path) -> bool {
    let extensions = [".zip", ".tar", ".gz", ".bz2", ".xz", ".7z", ".rar"];
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| extensions.contains(&ext.to_lowercase().as_str()))
        .unwrap_or(false)
}

fn is_media(path: &Path) -> bool {
    let extensions = [".jpg", ".jpeg", ".png", ".gif", ".bmp", ".tiff", ".mp3", ".wav", ".flac", ".ogg", ".mp4", ".avi", ".mkv", ".mov"];
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| extensions.contains(&ext.to_lowercase().as_str()))
        .unwrap_or(false)
}
