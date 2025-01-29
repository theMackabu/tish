use std::{collections::HashMap, env, path::Path};

#[derive(Clone, Hash, PartialEq, Eq)]
pub enum TokenType {
    ValidCommand,
    InvalidCommand,
    Argument,
    Option,
    Variable,
    String,
    Number,
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
        styles.insert(TokenType::ValidCommand, "\x1b[1;32m".to_string());
        styles.insert(TokenType::InvalidCommand, "\x1b[1;31m".to_string());
        styles.insert(TokenType::Argument, "\x1b[0m".to_string());
        styles.insert(TokenType::Option, "\x1b[1;36m".to_string());
        styles.insert(TokenType::Variable, "\x1b[1;35m".to_string());
        styles.insert(TokenType::String, "\x1b[0;33m".to_string());
        styles.insert(TokenType::Number, "\x1b[0;34m".to_string());
        styles.insert(TokenType::Operator, "\x1b[1;37m".to_string());
        styles.insert(TokenType::Comment, "\x1b[0;90m".to_string());
        styles.insert(TokenType::Unknown, "\x1b[0m".to_string());

        Self { styles }
    }

    pub fn command_exists(&self, command: &str) -> bool {
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

        matches!(command, "cd" | "exit" | "help" | "?" | "source" | "echo" | "tish")
    }

    pub fn highlight_with_cache(&self, input: &str, command_cache: &HashMap<String, bool>) -> String {
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

    fn tokenize(&self, input: &str, command_cache: &HashMap<String, bool>) -> Vec<Token> {
        let mut tokens = Vec::new();
        let mut chars = input.chars().peekable();
        let mut current_pos = 0;
        let mut is_first_word = true;

        while let Some(c) = chars.next() {
            match c {
                '#' => {
                    let start = current_pos;
                    let mut content = String::from(c);
                    while let Some(next_c) = chars.next() {
                        content.push(next_c);
                        current_pos += 1;
                        if next_c == '\n' {
                            break;
                        }
                    }
                    tokens.push(Token {
                        token_type: TokenType::Comment,
                        start,
                        end: current_pos + 1,
                        content,
                    });
                }
                '$' => {
                    let start = current_pos;
                    let mut content = String::from(c);
                    while let Some(next_c) = chars.peek() {
                        if !next_c.is_alphanumeric() && *next_c != '_' {
                            break;
                        }
                        content.push(*next_c);
                        chars.next();
                        current_pos += 1;
                    }
                    tokens.push(Token {
                        token_type: TokenType::Variable,
                        start,
                        end: current_pos + 1,
                        content,
                    });
                }
                '"' | '\'' => {
                    let start = current_pos;
                    let quote = c;
                    let mut content = String::from(c);
                    let mut escaped = false;
                    while let Some(next_c) = chars.next() {
                        content.push(next_c);
                        current_pos += 1;
                        if !escaped && next_c == quote {
                            break;
                        }
                        escaped = !escaped && next_c == '\\';
                    }
                    tokens.push(Token {
                        token_type: TokenType::String,
                        start,
                        end: current_pos + 1,
                        content,
                    });
                    is_first_word = false;
                }
                '-' => {
                    let start = current_pos;
                    let mut content = String::from(c);
                    while let Some(next_c) = chars.peek() {
                        if next_c.is_whitespace() {
                            break;
                        }
                        content.push(*next_c);
                        chars.next();
                        current_pos += 1;
                    }
                    tokens.push(Token {
                        token_type: TokenType::Option,
                        start,
                        end: current_pos + 1,
                        content,
                    });
                    is_first_word = false;
                }
                c if c.is_ascii_digit() => {
                    let start = current_pos;
                    let mut content = String::from(c);
                    while let Some(next_c) = chars.peek() {
                        if !next_c.is_ascii_digit() && *next_c != '.' {
                            break;
                        }
                        content.push(*next_c);
                        chars.next();
                        current_pos += 1;
                    }
                    tokens.push(Token {
                        token_type: TokenType::Number,
                        start,
                        end: current_pos + 1,
                        content,
                    });
                    is_first_word = false;
                }
                c if c.is_alphabetic() || c == '_' || c == '.' || c == '/' => {
                    let start = current_pos;
                    let mut content = String::from(c);
                    while let Some(next_c) = chars.peek() {
                        if next_c.is_whitespace() {
                            break;
                        }
                        content.push(*next_c);
                        chars.next();
                        current_pos += 1;
                    }

                    let token_type = if is_first_word {
                        if let Some(&exists) = command_cache.get(&content) {
                            if exists {
                                TokenType::ValidCommand
                            } else {
                                TokenType::InvalidCommand
                            }
                        } else {
                            TokenType::ValidCommand
                        }
                    } else {
                        TokenType::Argument
                    };

                    tokens.push(Token {
                        token_type,
                        start,
                        end: current_pos + 1,
                        content,
                    });
                    is_first_word = false;
                }
                '|' | '>' | '<' | '&' | ';' | '=' => {
                    tokens.push(Token {
                        token_type: TokenType::Operator,
                        start: current_pos,
                        end: current_pos + 1,
                        content: c.to_string(),
                    });
                    is_first_word = true;
                }
                c if c.is_whitespace() => {
                    if !chars.peek().map_or(true, |c| c.is_whitespace()) {
                        is_first_word = false;
                    }
                }
                _ => {}
            }
            current_pos += 1;
        }

        tokens
    }
}
