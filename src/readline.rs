use crate::shell::highlight;
use anyhow::{anyhow, Result};
use parking_lot::RwLock;
use tokio::sync::mpsc;

use std::{
    collections::{HashMap, HashSet},
    env, fs,
    path::Path,
    sync::Arc,
};

use rustyline::{
    completion::Completer,
    error::ReadlineError,
    highlight::{CmdKind, Highlighter, MatchingBracketHighlighter},
    hint::Hinter,
    history::{FileHistory, History, SearchDirection, SearchResult},
    validate::{MatchingBracketValidator, Validator},
    CompletionType, Config, Context, Editor, Helper,
};

type Readline<T> = Editor<T, FileHistory>;
type Receiver = Result<String, ReadlineError>;

enum Operation {
    Readline(String),
    AddHistory(String),
}

pub struct AsyncLineReader {
    buffer: String,
    continuation: bool,
    request_tx: mpsc::Sender<Operation>,
    response_rx: mpsc::Receiver<Receiver>,
}

struct TishHelper {
    highlighter: highlight::Highlighter,
    bracket_highlighter: MatchingBracketHighlighter,
    validator: MatchingBracketValidator,
    command_cache: Arc<RwLock<HashMap<String, bool>>>,
}

impl TishHelper {
    fn new() -> Self {
        Self {
            highlighter: highlight::Highlighter::new(),
            bracket_highlighter: MatchingBracketHighlighter::new(),
            validator: MatchingBracketValidator::new(),
            command_cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    fn update_command_status(&self, line: &str) {
        let mut cache = self.command_cache.write();
        let words: Vec<&str> = line.split_whitespace().collect();
        cache.clear();

        if let Some(first_word) = words.first() {
            if !cache.contains_key(*first_word) {
                cache.insert(first_word.to_string(), self.highlighter.command_exists(first_word));
            }
        }
    }

    fn get_history_matches(&self, word: &str, history: &dyn History) -> Vec<String> {
        let mut matches = HashSet::new();

        for index in (0..history.len()).rev() {
            if let Ok(Some(result)) = history.get(index, SearchDirection::Forward) {
                let words: Vec<&str> = result.entry.split_whitespace().collect();
                if let Some(first_word) = words.first() {
                    if first_word.starts_with(word) {
                        matches.insert(first_word.to_string());
                    }
                }
            }
        }

        let mut result: Vec<String> = matches.into_iter().collect();
        result.sort();
        result
    }

    fn get_completions(&self, input: &str, ctx: &Context<'_>) -> Vec<String> {
        let mut completions = Vec::new();
        let (_, word) = input.rsplit_once(char::is_whitespace).map_or(("", input), |(p, w)| (p, w));

        let history_matches = self.get_history_matches(word, ctx.history());
        completions.extend(history_matches);

        if !completions.is_empty() {
            return completions;
        }

        if word.is_empty() || ["cd", "exit", "help", "?", "source", "echo", "tish"].iter().any(|cmd| cmd.starts_with(word)) {
            for cmd in ["cd", "exit", "help", "?", "source", "echo", "tish"] {
                if cmd.starts_with(word) {
                    completions.push(cmd.to_string());
                }
            }
        }

        if word.starts_with("~/") {
            if let Some(home) = dirs::home_dir() {
                let home_str = home.to_string_lossy();
                let search_path = word.replace("~/", &format!("{}/", home_str));
                let dir_path = Path::new(&search_path);
                let parent = dir_path.parent().unwrap_or(dir_path);

                if let Ok(entries) = fs::read_dir(parent) {
                    let search_name = dir_path.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();

                    let mut matches: Vec<_> = entries.filter_map(Result::ok).filter(|entry| entry.file_name().to_string_lossy().starts_with(&search_name)).collect();

                    matches.sort_by(|a, b| a.file_name().cmp(&b.file_name()));

                    for entry in matches {
                        let path = entry.path();
                        if let Ok(stripped) = path.strip_prefix(&home) {
                            let completion = format!("~/{}", stripped.to_string_lossy());
                            if entry.file_type().map(|ft| ft.is_dir()).unwrap_or(false) {
                                completions.push(format!("{}/", completion));
                            } else {
                                completions.push(completion);
                            }
                        }
                    }
                }
            }
        } else if word.contains('/') || word.starts_with('.') {
            let (dir_path, file_prefix) = word.rsplit_once('/').map_or((".", word), |(d, f)| (d, f));

            if let Ok(entries) = fs::read_dir(dir_path) {
                let mut matches: Vec<_> = entries.filter_map(Result::ok).filter(|entry| entry.file_name().to_string_lossy().starts_with(file_prefix)).collect();

                matches.sort_by_cached_key(|entry| entry.file_name().to_string_lossy().into_owned());

                for entry in matches {
                    let path = entry.path();
                    let completion = path.to_string_lossy().into_owned();

                    if entry.file_type().map(|ft| ft.is_dir()).unwrap_or(false) {
                        completions.push(format!("{}/", completion));
                    } else {
                        completions.push(completion);
                    }
                }
            }
        } else {
            if let Ok(paths) = env::var("PATH") {
                for path in env::split_paths(&paths) {
                    if let Ok(entries) = fs::read_dir(path) {
                        for entry in entries.filter_map(Result::ok) {
                            let name = entry.file_name().to_string_lossy().to_string();
                            if name.starts_with(word) {
                                completions.push(name);
                            }
                        }
                    }
                }
            }

            if let Ok(entries) = fs::read_dir(".") {
                let mut matches: Vec<_> = entries.filter_map(Result::ok).filter(|entry| entry.file_name().to_string_lossy().starts_with(word)).collect();

                matches.sort_by(|a, b| a.file_name().cmp(&b.file_name()));

                for entry in matches {
                    let name = entry.file_name().to_string_lossy().into_owned();
                    if entry.file_type().map(|ft| ft.is_dir()).unwrap_or(false) {
                        completions.push(format!("{}/", name));
                    } else {
                        completions.push(name);
                    }
                }
            }
        }

        completions.sort();
        completions.dedup();
        completions
    }
}

impl Helper for TishHelper {}

impl Completer for TishHelper {
    type Candidate = String;

    fn complete(&self, line: &str, pos: usize, ctx: &Context<'_>) -> rustyline::Result<(usize, Vec<Self::Candidate>)> {
        self.update_command_status(line);
        let (start, _) = line[..pos].rsplit_once(char::is_whitespace).map_or((0, line), |(_, w)| (pos - w.len(), w));
        let completions = self.get_completions(line, ctx);
        Ok((start, completions))
    }
}

impl Hinter for TishHelper {
    type Hint = String;

    fn hint(&self, line: &str, pos: usize, ctx: &Context<'_>) -> Option<String> {
        if pos < line.len() {
            return None;
        }

        let word = line.rsplit_once(char::is_whitespace).map_or(line, |(_, w)| w);
        if word.is_empty() {
            return None;
        }

        let completions = self.get_completions(line, ctx);
        completions.first().map(|s| {
            let hint = if let Some(common) = line.rsplit_once(char::is_whitespace) {
                s.strip_prefix(common.1).unwrap_or(s).to_string()
            } else {
                s.strip_prefix(line).unwrap_or(s).to_string()
            };
            return hint;
        })
    }
}

impl Validator for TishHelper {
    fn validate(&self, ctx: &mut rustyline::validate::ValidationContext) -> rustyline::Result<rustyline::validate::ValidationResult> { self.validator.validate(ctx) }
}

impl Highlighter for TishHelper {
    fn highlight_hint<'h>(&self, hint: &'h str) -> std::borrow::Cow<'h, str> { std::borrow::Cow::Owned(format!("\x1b[90m{hint}\x1b[0m")) }

    fn highlight_candidate<'c>(&self, candidate: &'c str, _: CompletionType) -> std::borrow::Cow<'c, str> { std::borrow::Cow::Borrowed(candidate) }

    fn highlight<'l>(&self, line: &'l str, _: usize) -> std::borrow::Cow<'l, str> {
        self.update_command_status(line);
        let cache = self.command_cache.read();
        self.highlighter.highlight_with_cache(line, &cache).into()
    }

    fn highlight_char(&self, line: &str, pos: usize, kind: CmdKind) -> bool {
        if kind == CmdKind::Other || kind == CmdKind::MoveCursor {
            self.update_command_status(line);
            true
        } else {
            self.bracket_highlighter.highlight_char(line, pos, kind)
        }
    }

    fn highlight_prompt<'b, 's: 'b, 'p: 'b>(&'s self, prompt: &'p str, default: bool) -> std::borrow::Cow<'b, str> {
        if default {
            std::borrow::Cow::Borrowed(prompt)
        } else {
            std::borrow::Cow::Owned(format!("\x1b[1;32m{}\x1b[0m", prompt))
        }
    }
}

impl AsyncLineReader {
    pub fn new() -> Result<Self> {
        let (request_tx, mut request_rx) = mpsc::channel::<Operation>(32);
        let (response_tx, response_rx) = mpsc::channel::<Receiver>(32);

        let config = Config::builder()
            .history_ignore_dups(true)?
            .color_mode(rustyline::ColorMode::Enabled)
            .completion_type(rustyline::CompletionType::Fuzzy)
            .edit_mode(rustyline::EditMode::Emacs)
            .check_cursor_position(true)
            .build();

        let mut editor: Readline<TishHelper> = Readline::with_config(config)?;

        editor.set_helper(Some(TishHelper::new()));
        editor.bind_sequence(rustyline::KeyEvent::new('\r', rustyline::Modifiers::NONE), rustyline::Cmd::AcceptLine);

        let history_file = {
            let mut file = dirs::home_dir().ok_or_else(|| anyhow!("Could not determine home directory"))?;
            file.push(".tish_history");
            file
        };

        if history_file.exists() {
            if let Err(e) = editor.load_history(&history_file) {
                eprintln!("Failed to load history: {}", e);
            }
        }

        std::thread::spawn(move || {
            while let Some(prompt) = request_rx.blocking_recv() {
                match prompt {
                    Operation::Readline(prompt) => {
                        let result = editor.readline(&prompt);
                        if let Err(e) = editor.save_history(&history_file) {
                            eprintln!("Failed to save history: {}", e);
                        }
                        if let Err(e) = response_tx.blocking_send(result) {
                            eprintln!("Failed to send readline result: {}", e);
                            break;
                        }
                    }
                    Operation::AddHistory(line) => {
                        if let Err(e) = editor.add_history_entry(line) {
                            eprintln!("Failed to read history: {}", e);
                            break;
                        };
                        if let Err(e) = editor.save_history(&history_file) {
                            eprintln!("Failed to save history: {}", e);
                        }
                    }
                }
            }
        });

        Ok(Self {
            request_tx,
            response_rx,
            continuation: false,
            buffer: String::new(),
        })
    }

    pub fn clear_buffer(&mut self) {
        self.buffer.clear();
        self.continuation = false;
    }

    pub async fn add_history_entry(&mut self, line: &str) -> Result<std::process::ExitCode> {
        self.request_tx.send(Operation::AddHistory(line.to_owned())).await.map_err(|_| ReadlineError::Interrupted)?;
        Ok(std::process::ExitCode::SUCCESS)
    }

    pub async fn async_readline(&mut self, prompt: &str) -> Result<String, ReadlineError> {
        loop {
            let current_prompt = if self.continuation { "> " } else { prompt };

            self.request_tx.send(Operation::Readline(current_prompt.to_owned())).await.map_err(|_| ReadlineError::Interrupted)?;

            match self.response_rx.recv().await.unwrap_or(Err(ReadlineError::Interrupted)) {
                Ok(line) => {
                    if line.ends_with('\\') {
                        self.buffer.push_str(&line[..line.len() - 1]);
                        self.buffer.push('\n');
                        self.continuation = true;
                        continue;
                    } else {
                        if self.continuation {
                            self.buffer.push_str(&line);
                            let result = self.buffer.clone();
                            self.buffer.clear();
                            self.continuation = false;
                            return Ok(result);
                        } else {
                            return Ok(line);
                        }
                    }
                }
                Err(e) => {
                    self.clear_buffer();
                    return Err(e);
                }
            }
        }
    }
}
