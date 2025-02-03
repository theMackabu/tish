// TODO prefer built in commands over binaries

use crate::shell::highlight;
use anyhow::{anyhow, Result};
use parking_lot::RwLock;
use pat::Tap;
use tokio::sync::mpsc;

use std::{
    collections::{HashMap, HashSet},
    env,
    fs::{self, DirEntry},
    path::PathBuf,
    sync::Arc,
};

use rustyline::{
    completion::Completer,
    error::ReadlineError,
    highlight::{CmdKind, Highlighter, MatchingBracketHighlighter},
    hint::Hinter,
    history::{FileHistory, History, SearchDirection},
    validate::{MatchingBracketValidator, Validator},
    ColorMode, CompletionType, Config, Context, Editor, Helper,
};

type Readline<T> = Editor<T, FileHistory>;
type Receiver = Result<String, ReadlineError>;

pub struct AsyncLineReader {
    buffer: String,
    continuation: bool,
    request_tx: mpsc::Sender<String>,
    response_rx: mpsc::Receiver<Receiver>,
}

struct TishHelper {
    highlighter: highlight::Highlighter,
    bracket_highlighter: MatchingBracketHighlighter,
    validator: MatchingBracketValidator,
    command_cache: Arc<RwLock<HashMap<String, bool>>>,
    current_line: Arc<RwLock<String>>,
}

impl TishHelper {
    fn new() -> Self {
        Self {
            highlighter: highlight::Highlighter::new(),
            bracket_highlighter: MatchingBracketHighlighter::new(),
            validator: MatchingBracketValidator::new(),
            command_cache: Arc::new(RwLock::new(HashMap::new())),
            current_line: Arc::new(RwLock::new(String::new())),
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
                if result.entry.starts_with(word) {
                    matches.insert(result.entry.to_string());
                }

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

        let commands = ["cd", "exit", "help", "?", "source", "echo", "tish"];
        let (_, word) = input.rsplit_once(char::is_whitespace).map_or(("", input), |(p, w)| (p, w));

        if word.is_empty() || commands.iter().any(|cmd| cmd.starts_with(word)) {
            for cmd in commands {
                if cmd.starts_with(word) {
                    completions.push(cmd.to_string());
                }
            }
        }

        if word.starts_with("~/") {
            if let Some(home) = dirs::home_dir() {
                let replace_path = |path: &str| {
                    let home_str = home.to_string_lossy();
                    path.replace("~/", &format!("{}/", home_str))
                };

                let parent = match word {
                    "~/" => home.clone(),
                    path if path.ends_with('/') => PathBuf::from(replace_path(path)),
                    path => PathBuf::from(replace_path(path)).parent().unwrap_or(&home).to_path_buf(),
                };

                if let Ok(entries) = fs::read_dir(&parent) {
                    let search_name = match word {
                        w if w == "~/" || w.ends_with('/') => String::new(),
                        _ => PathBuf::from(word).file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default(),
                    };

                    let matches = entries
                        .filter_map(Result::ok)
                        .filter(|entry| {
                            entry.file_name().to_str().map_or(false, |name| {
                                let is_hidden = name.starts_with(".");
                                let is_home_path = word.starts_with("~/.");
                                let matches_search = name.starts_with(&search_name);

                                matches_search && (!is_hidden || is_home_path)
                            })
                        })
                        .collect::<Vec<DirEntry>>()
                        .tap(|matches| matches.sort_by(|a, b| a.file_name().cmp(&b.file_name())));

                    for entry in matches {
                        let path = entry.path();

                        if let Ok(stripped) = path.strip_prefix(&home) {
                            let completion = format!("~/{}", stripped.to_string_lossy());
                            if entry.file_type().map(|ft| ft.is_dir()).unwrap_or(false) {
                                if !completion.ends_with('/') {
                                    completions.push(format!("{}/", completion));
                                } else {
                                    completions.push(completion);
                                }
                            }
                        }
                    }
                }
            }
        } else if word.contains('/') || !word.starts_with('~') {
            let (dir_path, file_prefix) = word.rsplit_once('/').map_or((".", word), |(d, f)| (d, f));

            if let Ok(entries) = fs::read_dir(dir_path) {
                let matches: Vec<_> = entries
                    .filter_map(Result::ok)
                    .filter(|entry| {
                        entry.file_name().to_str().map_or(false, |name| {
                            let is_hidden = name.starts_with(".");
                            let show_hidden = file_prefix.starts_with(".");
                            name.starts_with(file_prefix) && (!is_hidden || show_hidden)
                        })
                    })
                    .collect::<Vec<DirEntry>>()
                    .tap(|matches| matches.sort_by_cached_key(|entry| entry.file_name().to_string_lossy().into_owned()));

                for entry in matches {
                    let completion = if dir_path == "." {
                        entry.file_name().to_string_lossy().into_owned()
                    } else {
                        format!("{}/{}", dir_path, entry.file_name().to_string_lossy())
                    };

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

        let history_matches = self.get_history_matches(word, ctx.history());
        completions.extend(history_matches);

        completions.sort();
        completions.dedup();

        return completions;
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
        if pos < line.len() || line.trim().is_empty() {
            return None;
        }

        *self.current_line.write() = line.to_string();

        let completions = self.get_completions(line, ctx);
        if let Some(hint) = completions.iter().find(|s| s.starts_with(line)) {
            return Some(hint.strip_prefix(line).unwrap_or(hint).to_string());
        }

        let word = line.rsplit_once(char::is_whitespace).map_or(line, |(_, w)| w);
        if word.is_empty() {
            return None;
        }

        completions.first().map(|s| {
            if let Some(common) = line.rsplit_once(char::is_whitespace) {
                s.strip_prefix(common.1).unwrap_or(s).to_string()
            } else {
                s.strip_prefix(line).unwrap_or(s).to_string()
            }
        })
    }
}

impl Validator for TishHelper {
    fn validate(&self, ctx: &mut rustyline::validate::ValidationContext) -> rustyline::Result<rustyline::validate::ValidationResult> { self.validator.validate(ctx) }
}

impl Highlighter for TishHelper {
    fn highlight_candidate<'c>(&self, candidate: &'c str, _: CompletionType) -> std::borrow::Cow<'c, str> { std::borrow::Cow::Borrowed(candidate) }

    fn highlight<'l>(&self, line: &'l str, _: usize) -> std::borrow::Cow<'l, str> {
        self.update_command_status(line);
        let cache = self.command_cache.read();
        self.highlighter.highlight_with_cache(line, &cache).into()
    }

    fn highlight_hint<'h>(&self, hint: &'h str) -> std::borrow::Cow<'h, str> {
        let current_line = self.current_line.read();
        if current_line.ends_with(' ') {
            std::borrow::Cow::Owned(format!("\x1b[90m {hint}\x1b[0m"))
        } else {
            std::borrow::Cow::Owned(format!("\x1b[90m{hint}\x1b[0m"))
        }
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
        let (request_tx, mut request_rx) = mpsc::channel::<String>(32);
        let (response_tx, response_rx) = mpsc::channel::<Receiver>(32);

        let config = Config::builder()
            .auto_add_history(true)
            .max_history_size(500)?
            .history_ignore_dups(true)?
            .color_mode(ColorMode::Enabled)
            .completion_type(CompletionType::Fuzzy)
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
                let result = editor.readline(&prompt);
                if let Err(e) = editor.save_history(&history_file) {
                    eprintln!("Failed to save history: {}", e);
                }
                if let Err(e) = response_tx.blocking_send(result) {
                    eprintln!("Failed to send readline result: {}", e);
                    break;
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

    pub async fn async_readline(&mut self, prompt: &str) -> Result<String, ReadlineError> {
        loop {
            let current_prompt = if self.continuation { "> " } else { prompt };

            self.request_tx.send(current_prompt.to_owned()).await.map_err(|_| ReadlineError::Interrupted)?;

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
