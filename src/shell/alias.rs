use std::collections::HashSet;

pub fn resolve_command(line: String) -> String {
    if line.trim().is_empty() {
        return String::new();
    }

    let mut words: Vec<&str> = line.split_whitespace().collect();
    if words.is_empty() {
        return String::new();
    }

    let first_word = words[0].to_string();
    words.remove(0);

    let resolved = resolve_alias_recursively(first_word, Vec::new());

    if !words.is_empty() {
        format!("{} {}", resolved, words.join(" "))
    } else {
        resolved
    }
}

fn resolve_alias_recursively(command: String, mut accumulated_args: Vec<String>) -> String {
    let mut seen_aliases = HashSet::new();
    let mut current_command = command;

    while !seen_aliases.contains(&current_command) {
        seen_aliases.insert(current_command.clone());

        let alias = crate::ALIASES.lock().expect("Unable to acquire alias lock");

        match alias.get(&current_command) {
            Some(resolved) => {
                let parts: Vec<&str> = resolved.split_whitespace().collect();
                if parts.is_empty() {
                    return current_command;
                }

                let new_command = parts[0].to_string();

                if parts.len() > 1 {
                    let new_args: Vec<String> = parts[1..].iter().map(|&s| s.to_string()).collect();
                    let mut combined_args = new_args;
                    combined_args.extend(accumulated_args);
                    accumulated_args = combined_args;
                }

                if new_command == current_command {
                    if accumulated_args.is_empty() {
                        return resolved.to_string();
                    } else {
                        return format!("{} {}", new_command, accumulated_args.join(" "));
                    }
                }

                current_command = new_command;
            }
            None => {
                if accumulated_args.is_empty() {
                    return current_command;
                } else {
                    return format!("{} {}", current_command, accumulated_args.join(" "));
                }
            }
        }
    }

    if accumulated_args.is_empty() {
        current_command
    } else {
        format!("{} {}", current_command, accumulated_args.join(" "))
    }
}
