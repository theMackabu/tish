#![allow(non_upper_case_globals)]

use anyhow::Result;
use chrono::{DateTime, Local, TimeZone};

use std::{
    fs,
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
    process::ExitCode,
    time::{SystemTime, UNIX_EPOCH},
};

use crate::{
    cmd::file,
    os::{size::dimensions, user::get_user_by_uid},
    prelude::*,
};

struct Entry {
    name: String,
    size: String,
    modified: String,
    username: String,
    file_type: String,
    color: String,
    icon: &'static str,
    permissions: String,
}

struct ColumnWidths {
    name: usize,
    size: usize,
    file_type: usize,
    permissions: usize,
}

pub fn run(args: &Vec<String>) -> Result<ExitCode> {
    let mut table = false;
    let mut numbers = false;
    let mut show_all = false;
    let mut metadata = false;
    let mut path = PathBuf::from(".");

    argument! {
        args: args.into_iter(),
        options: {
            l => table = true,
            n => numbers = true,
            m => metadata = true,
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

    let entries = read_directory(&path, show_all)?;

    if table {
        print_table_entries(&entries, metadata, numbers)?;
    } else {
        print_standard_entries(&entries)?;
    }

    Ok(ExitCode::SUCCESS)
}

fn print_usage() {
    println!("usage: ls [-alnm] [path]");
}

fn read_directory(path: &Path, show_all: bool) -> std::io::Result<Vec<Entry>> {
    let mut entries: Vec<_> = fs::read_dir(path)?.filter_map(Result::ok).filter(|entry| show_all || !is_hidden(entry)).collect();

    entries.sort_by(|a, b| a.file_name().cmp(&b.file_name()));

    let mut result = Vec::new();
    for entry in entries {
        let metadata = entry.metadata()?;
        if let Ok(formatted_entry) = format_entry(&entry, &metadata) {
            result.push(formatted_entry);
        }
    }

    result.sort_by(|a, b| if a.file_type != b.file_type { a.file_type.cmp(&b.file_type) } else { a.name.cmp(&b.name) });

    Ok(result)
}

fn format_entry(entry: &fs::DirEntry, metadata: &fs::Metadata) -> std::io::Result<Entry> {
    let mode = metadata.mode();
    let name = entry.file_name().to_string_lossy().into_owned();
    let file_info = file::FileInfo::new(&metadata, &name);

    Ok(Entry {
        name: file_info.display_name,
        size: format_size(metadata.len()),
        modified: format_time(metadata.modified()?),
        username: get_username(metadata.uid()),
        file_type: file_info.file_type.to_string(),
        icon: file_info.icon.get_glyph(),
        color: file_info.icon.get_color(),
        permissions: format_permissions(mode),
    })
}

fn get_username(uid: u32) -> String { get_user_by_uid(uid).map(|user| user.name().to_string_lossy().into_owned()).unwrap_or_else(|| uid.to_string()) }

fn calculate_column_widths(entries: &[Entry]) -> ColumnWidths {
    let mut widths = ColumnWidths {
        name: 4,
        size: 4,
        file_type: 4,
        permissions: 11,
    };

    for entry in entries {
        widths.name = widths.name.max(entry.name.len());
        widths.size = widths.size.max(entry.size.len());
        widths.file_type = widths.file_type.max(entry.file_type.len());
    }

    widths
}

fn print_standard_entries(entries: &[Entry]) -> std::io::Result<()> {
    let terminal_width = match dimensions() {
        Some((w, _)) => w,
        None => 80,
    };

    let mut current_line_width = 0;
    let min_spacing = 2;

    for (i, entry) in entries.iter().enumerate() {
        let display_length = entry.name.len() + 2;

        if current_line_width + display_length >= terminal_width {
            println!();
            current_line_width = 0;
        }

        print!("{}{} \x1b[0m{}", entry.color, entry.icon, entry.name);

        if i < entries.len() - 1 {
            print!("  ");
            current_line_width += display_length + min_spacing;
        }
    }

    if !entries.is_empty() {
        println!();
    }

    Ok(())
}

fn print_table_entries(entries: &[Entry], show_metadata: bool, show_numbers: bool) -> std::io::Result<()> {
    const grey: &'static str = "\x1b[38;5;240m";
    const yellow: &'static str = "\x1b[33m";
    const light_pink: &'static str = "\x1b[38;5;217m";
    const cyan: &'static str = "\x1b[36m";
    const light_cyan: &'static str = "\x1b[96m";
    const light_grey: &'static str = "\x1b[37m";
    const light_green: &'static str = "\x1b[92m";
    const light_magenta: &'static str = "\x1b[95m";
    const reset: &'static str = "\x1b[0m";

    let widths = calculate_column_widths(entries);
    let num_width = if show_numbers { entries.len().to_string().len().max(1) } else { 0 };

    let mut header = format!("{}╭", grey);
    if show_numbers {
        header.push_str(&format!("{}┬", "─".repeat(num_width + 2)));
    }
    header.push_str(&format!(
        "{}┬{}┬{}┬{}",
        "─".repeat(widths.name + 4),
        "─".repeat(widths.size + 2),
        "─".repeat(widths.file_type + 2),
        "─".repeat(widths.permissions + 2)
    ));

    if show_metadata {
        header.push_str(&format!("┬{}┬{}", "─".repeat(12), "─".repeat(16)));
    }
    header.push_str(&format!("╮{}", reset));
    println!("{}", header);

    let mut titles = format!("{}│", grey);
    if show_numbers {
        titles.push_str(&format!("{} {:<width_num$} {}│", cyan, "#", grey, width_num = num_width));
    }
    titles.push_str(&format!(
        "{} {:<width_name$} {}│{} {:<width_size$} {}│{} {:<width_type$} {}│{} {:<width_perm$}",
        yellow,
        "name",
        grey,
        yellow,
        "size",
        grey,
        yellow,
        "type",
        grey,
        yellow,
        "permissions",
        width_name = widths.name + 2,
        width_size = widths.size,
        width_type = widths.file_type,
        width_perm = widths.permissions + if show_metadata { 0 } else { 1 }
    ));

    if show_metadata {
        titles.push_str(&format!(" {}│{} {:<10} {}│{} {:<14} ", grey, yellow, "user", grey, yellow, "modified"));
    }
    titles.push_str(&format!("{}│{}", grey, reset));
    println!("{}", titles);

    let mut separator = format!("{}├", grey);
    if show_numbers {
        separator.push_str(&format!("{}┼", "─".repeat(num_width + 2)));
    }
    separator.push_str(&format!(
        "{}┼{}┼{}┼{}",
        "─".repeat(widths.name + 4),
        "─".repeat(widths.size + 2),
        "─".repeat(widths.file_type + 2),
        "─".repeat(widths.permissions + 2)
    ));

    if show_metadata {
        separator.push_str(&format!("┼{}┼{}", "─".repeat(12), "─".repeat(16)));
    }
    separator.push_str(&format!("┤{}", reset));
    println!("{}", separator);

    for (idx, entry) in entries.iter().enumerate() {
        let mut line = format!("{}│", grey);
        if show_numbers {
            line.push_str(&format!("{} {:<width_num$} {}│", light_cyan, idx, grey, width_num = num_width));
        }
        line.push_str(&format!(
            "{} {}{}{} {:<width_name$} {}│{} {:>width_size$} {}│{} {:<width_type$} {}│{} \
             {:<width_perm$}",
            reset,
            entry.color,
            entry.icon,
            "\x1b[0m",
            entry.name,
            grey,
            cyan,
            entry.size,
            grey,
            light_grey,
            entry.file_type,
            grey,
            light_green,
            entry.permissions,
            width_name = widths.name,
            width_size = widths.size,
            width_type = widths.file_type,
            width_perm = widths.permissions + if show_metadata { 0 } else { 1 }
        ));

        if show_metadata {
            line.push_str(&format!(" {}│{} {:<10} {}│{} {:<14} ", grey, light_pink, entry.username, grey, light_magenta, entry.modified));
        }
        line.push_str(&format!("{}│{}", grey, reset));
        println!("{}", line);
    }

    let mut footer = format!("{}╰", grey);
    if show_numbers {
        footer.push_str(&format!("{}┴", "─".repeat(num_width + 2)));
    }
    footer.push_str(&format!(
        "{}┴{}┴{}┴{}",
        "─".repeat(widths.name + 4),
        "─".repeat(widths.size + 2),
        "─".repeat(widths.file_type + 2),
        "─".repeat(widths.permissions + 2)
    ));

    if show_metadata {
        footer.push_str(&format!("┴{}┴{}", "─".repeat(12), "─".repeat(16)));
    }
    footer.push_str(&format!("╯{}", reset));
    println!("{}", footer);

    Ok(())
}

fn format_size(size: u64) -> String {
    if size >= 1024 * 1024 * 1024 {
        format!("{:>5.1} GB", size as f64 / (1024.0 * 1024.0 * 1024.0))
    } else if size >= 1024 * 1024 {
        format!("{:>5.1} MB", size as f64 / (1024.0 * 1024.0))
    } else if size >= 1024 {
        format!("{:>5.1} KB", size as f64 / 1024.0)
    } else {
        format!("{:>6} B", size)
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

fn format_permissions(mode: u32) -> String {
    let mut result = String::with_capacity(10);

    result.push(if mode & 0o040000 != 0 {
        'd'
    } else if mode & 0o120000 != 0 {
        'l'
    } else {
        '-'
    });

    result.push(if mode & 0o400 != 0 { 'r' } else { '-' });
    result.push(if mode & 0o200 != 0 { 'w' } else { '-' });
    result.push(if mode & 0o100 != 0 { 'x' } else { '-' });

    result.push(if mode & 0o040 != 0 { 'r' } else { '-' });
    result.push(if mode & 0o020 != 0 { 'w' } else { '-' });
    result.push(if mode & 0o010 != 0 { 'x' } else { '-' });

    result.push(if mode & 0o004 != 0 { 'r' } else { '-' });
    result.push(if mode & 0o002 != 0 { 'w' } else { '-' });
    result.push(if mode & 0o001 != 0 { 'x' } else { '-' });

    result
}
