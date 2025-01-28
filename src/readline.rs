use anyhow::Result;
use rustyline::{error::ReadlineError, history::FileHistory, Editor};
use tokio::sync::mpsc;

type Readline = Editor<(), FileHistory>;
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

impl AsyncLineReader {
    pub fn new() -> Result<Self> {
        let (request_tx, mut request_rx) = mpsc::channel::<Operation>(32);
        let (response_tx, response_rx) = mpsc::channel::<Receiver>(32);

        let mut editor = Readline::new()?;

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
