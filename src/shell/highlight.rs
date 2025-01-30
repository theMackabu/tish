// TODO: make lua functions work in the green/red highlighter

use std::{
    collections::HashMap,
    env,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
};

#[derive(Clone, Hash, PartialEq, Eq)]
pub enum TokenType {
    ValidCommand,
    InvalidCommand,
    Argument,
    Option,
    Variable,
    String,
    Number,
    Directory,
    ImplicitDirectory,
    Operator,
    Comment,
    Unknown,
}

#[derive(Clone)]
pub struct Token {
    pub token_type: TokenType,
    pub start: usize,
    pub end: usize,
    pub content: String,
}

pub struct Highlighter {
    styles: HashMap<TokenType, String>,
}

impl Highlighter {
    pub fn new() -> Self {
        let mut styles = HashMap::new();
        styles.insert(TokenType::ValidCommand, "\x1b[32m".to_string());
        styles.insert(TokenType::InvalidCommand, "\x1b[31m".to_string());
        styles.insert(TokenType::Argument, "\x1b[0m".to_string());
        styles.insert(TokenType::Option, "\x1b[36m".to_string());
        styles.insert(TokenType::Variable, "\x1b[35m".to_string());
        styles.insert(TokenType::Directory, "\x1b[4;35m".to_string());
        styles.insert(TokenType::ImplicitDirectory, "\x1b[4;35m".to_string());
        styles.insert(TokenType::String, "\x1b[33m".to_string());
        styles.insert(TokenType::Number, "\x1b[34m".to_string());
        styles.insert(TokenType::Operator, "\x1b[37m".to_string());
        styles.insert(TokenType::Comment, "\x1b[90m".to_string());
        styles.insert(TokenType::Unknown, "\x1b[0m".to_string());

        Self { styles }
    }

    pub fn command_exists(&self, command: &str) -> bool {
        if matches!(command, "cd" | "exit" | "help" | "?" | "source" | "echo" | "tish") {
            return true;
        }

        if Path::new(command).is_absolute() {
            return Path::new(command).exists();
        }

        if let Ok(paths) = env::var("PATH") {
            for path in env::split_paths(&paths) {
                let cmd_path = path.join(command);
                if cmd_path.exists() {
                    return true;
                }
            }
        }

        false
    }

    pub fn highlight_with_cache(&self, input: &str, command_cache: &HashMap<String, bool>) -> String {
        let input = input.trim_end();
        if input.is_empty() {
            return String::new();
        }

        let tokens = self.tokenize(input, command_cache);
        let mut result = String::new();
        let mut last_end = 0;

        for token in tokens {
            if token.start > last_end {
                result.push_str(&input[last_end..token.start]);
            }

            if let Some(style) = self.styles.get(&token.token_type) {
                result.push_str(style);
                result.push_str(&token.content);
                result.push_str("\x1b[0m");
            } else {
                result.push_str(&token.content);
            }

            last_end = token.end;
        }

        if last_end < input.len() {
            result.push_str(&input[last_end..]);
        }

        result
    }

    fn expand_path(&self, path: &Path) -> Option<PathBuf> {
        let path_str = path.to_string_lossy();
        if path_str.starts_with("~/") {
            if let Some(home_dir) = dirs::home_dir() {
                let relative_path = path_str.strip_prefix("~/").unwrap_or(&path_str);
                return Some(home_dir.join(relative_path));
            }
        } else if path.is_relative() {
            if let Ok(current_dir) = std::env::current_dir() {
                return Some(current_dir.join(path));
            }
        }
        Some(path.to_path_buf())
    }

    fn is_directory(&self, path: &Path) -> bool {
        if let Some(expanded_path) = self.expand_path(path) {
            expanded_path.is_dir()
        } else {
            false
        }
    }

    fn can_be_implicit_cd(&self, token: &str, is_first_word: bool) -> bool {
        if !is_first_word {
            return false;
        }

        let path = Path::new(token);
        if let Some(expanded_path) = self.expand_path(path) {
            return expanded_path.is_dir();
        }
        false
    }

    fn tokenize(&self, input: &str, command_cache: &HashMap<String, bool>) -> Vec<Token> {
        let mut tokens = Vec::new();
        let mut is_first_word = true;
        let mut in_whitespace = true;
        let mut chars = input.char_indices().peekable();

        while let Some((start_pos, c)) = chars.next() {
            match c {
                '#' => {
                    let start = start_pos;
                    let mut content = String::from(c);
                    let mut end = start;
                    while let Some((pos, next_c)) = chars.next() {
                        content.push(next_c);
                        end = pos;
                        if next_c == '\n' {
                            break;
                        }
                    }
                    tokens.push(Token {
                        token_type: TokenType::Comment,
                        start,
                        end: end + 1,
                        content,
                    });
                    is_first_word = false;
                    in_whitespace = true;
                }
                '$' => {
                    let start = start_pos;
                    let mut content = String::from(c);
                    let mut end = start;
                    while let Some(&(pos, next_c)) = chars.peek() {
                        if !next_c.is_alphanumeric() && next_c != '_' {
                            break;
                        }
                        content.push(next_c);
                        end = pos;
                        chars.next();
                    }
                    tokens.push(Token {
                        token_type: TokenType::Variable,
                        start,
                        end: end + 1,
                        content,
                    });
                }
                '"' | '\'' => {
                    let quote = c;
                    let start = start_pos;
                    let mut content = String::from(c);
                    let mut escaped = false;
                    let mut end = start;
                    while let Some((pos, next_c)) = chars.next() {
                        content.push(next_c);
                        end = pos;
                        if !escaped && next_c == quote {
                            break;
                        }
                        escaped = next_c == '\\' && !escaped;
                    }
                    tokens.push(Token {
                        token_type: TokenType::String,
                        start,
                        end: end + 1,
                        content,
                    });
                    is_first_word = false;
                }
                '-' if !in_whitespace => {
                    let start = start_pos;
                    let mut content = String::from(c);
                    let mut end = start;
                    while let Some(&(pos, next_c)) = chars.peek() {
                        if next_c.is_whitespace() {
                            break;
                        }
                        content.push(next_c);
                        end = pos;
                        chars.next();
                    }
                    tokens.push(Token {
                        token_type: TokenType::Argument,
                        start,
                        end: end + 1,
                        content,
                    });
                }
                '-' => {
                    let start = start_pos;
                    let mut content = String::from(c);
                    let mut end = start;
                    while let Some(&(pos, next_c)) = chars.peek() {
                        if next_c.is_whitespace() {
                            break;
                        }
                        content.push(next_c);
                        end = pos;
                        chars.next();
                    }
                    tokens.push(Token {
                        token_type: TokenType::Option,
                        start,
                        end: end + 1,
                        content,
                    });
                    is_first_word = false;
                    in_whitespace = false;
                }
                c if c.is_whitespace() => {
                    in_whitespace = true;
                    continue;
                }
                c if c.is_ascii_digit() => {
                    let start = start_pos;
                    let mut content = String::from(c);
                    let mut end = start;
                    while let Some(&(pos, next_c)) = chars.peek() {
                        if !next_c.is_ascii_digit() && next_c != '.' {
                            break;
                        }
                        content.push(next_c);
                        end = pos;
                        chars.next();
                    }
                    tokens.push(Token {
                        token_type: TokenType::Number,
                        start,
                        end: end + 1,
                        content,
                    });
                    is_first_word = false;
                }
                c if c.is_alphabetic() || c == '_' || c == '.' || c == '/' || c == '~' => {
                    let start = start_pos;
                    let mut content = String::from(c);
                    let mut end = start;

                    while let Some(&(pos, next_c)) = chars.peek() {
                        if next_c.is_whitespace() || next_c == '\\' {
                            break;
                        }
                        content.push(next_c);
                        end = pos;
                        chars.next();
                    }

                    let token_type = if is_first_word {
                        if self.can_be_implicit_cd(&content, true) {
                            TokenType::ImplicitDirectory
                        } else if content.starts_with("./") || content.starts_with("../") {
                            let path = Path::new(&content);
                            if path.exists() && path.metadata().map(|m| m.permissions().mode() & 0o111 != 0).unwrap_or(false) {
                                TokenType::ValidCommand
                            } else {
                                TokenType::InvalidCommand
                            }
                        } else {
                            command_cache
                                .get(&content)
                                .map_or(TokenType::InvalidCommand, |&exists| if exists { TokenType::ValidCommand } else { TokenType::InvalidCommand })
                        }
                    } else {
                        let path = Path::new(&content);
                        if self.is_directory(path) {
                            TokenType::Directory
                        } else {
                            TokenType::Argument
                        }
                    };

                    tokens.push(Token {
                        token_type,
                        start,
                        end: end + 1,
                        content,
                    });
                    is_first_word = false;
                }
                '|' | '>' | '<' | '&' | ';' | '=' | '\\' => {
                    let start = start_pos;
                    let mut content = c.to_string();
                    let mut end = start;

                    if let Some(&(pos, next_c)) = chars.peek() {
                        match (c, next_c) {
                            // FIXME
                            // if ls && ls (green)
                            // if anything_else && ls (its red)
                            ('&', '&') | ('|', '|') | ('>', '>') | ('<', '<') => {
                                content.push(next_c);
                                end = pos;
                                chars.next();
                            }
                            _ => {}
                        }
                    }

                    tokens.push(Token {
                        token_type: TokenType::Operator,
                        start,
                        end: end + 1,
                        content,
                    });

                    is_first_word = true;
                    in_whitespace = true;
                }
                _ => {}
            }
        }

        tokens
    }
}
