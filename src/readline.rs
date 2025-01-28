use crate::shell::highlight;
use anyhow::Result;
use tokio::sync::mpsc;

use rustyline::{
    completion::Completer,
    error::ReadlineError,
    highlight::{CmdKind, Highlighter, MatchingBracketHighlighter},
    hint::{Hinter, HistoryHinter},
    history::FileHistory,
    validate::{MatchingBracketValidator, Validator},
    Config, Context, Editor, Helper,
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
    hinter: HistoryHinter,
    highlighter: highlight::Highlighter,
    bracket_highlighter: MatchingBracketHighlighter,
    validator: MatchingBracketValidator,
}

impl TishHelper {
    fn new() -> Self {
        Self {
            hinter: HistoryHinter::new(),
            highlighter: highlight::Highlighter::new(),
            bracket_highlighter: MatchingBracketHighlighter::new(),
            validator: MatchingBracketValidator::new(),
        }
    }
}

impl Helper for TishHelper {}

impl Completer for TishHelper {
    type Candidate = String;

    fn complete(&self, _line: &str, _pos: usize, _ctx: &Context<'_>) -> rustyline::Result<(usize, Vec<Self::Candidate>)> {
        Ok((0, vec![]))
    }
}

impl Hinter for TishHelper {
    type Hint = String;

    fn hint(&self, line: &str, pos: usize, ctx: &Context<'_>) -> Option<String> {
        self.hinter.hint(line, pos, ctx)
    }
}

impl Validator for TishHelper {
    fn validate(&self, ctx: &mut rustyline::validate::ValidationContext) -> rustyline::Result<rustyline::validate::ValidationResult> {
        self.validator.validate(ctx)
    }
}

impl Highlighter for TishHelper {
    fn highlight<'l>(&self, line: &'l str, _: usize) -> std::borrow::Cow<'l, str> {
        self.highlighter.highlight(line).into()
    }

    fn highlight_char(&self, line: &str, pos: usize, kind: CmdKind) -> bool {
        self.bracket_highlighter.highlight_char(line, pos, kind)
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

        let helper = TishHelper::new();
        let config = Config::builder().color_mode(rustyline::ColorMode::Enabled).build();

        let mut editor: Readline<TishHelper> = Readline::with_config(config)?;
        editor.set_helper(Some(helper));

        std::thread::spawn(move || {
            while let Some(prompt) = request_rx.blocking_recv() {
                match prompt {
                    Operation::Readline(prompt) => {
                        let result = editor.readline(&prompt);
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
