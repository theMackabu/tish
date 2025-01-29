#[derive(Clone, Debug)]
pub struct Tokenizer {
    current: Option<String>,
    has_redirection: bool,
}

impl Tokenizer {
    pub fn new(line: &str) -> Self {
        let has_redirection = line.contains(" > ") || line.contains(" < ");

        Tokenizer {
            current: Some(line.to_string()),
            has_redirection,
        }
    }

    pub fn args_before_redirection(&mut self) -> Vec<String> {
        if !self.has_redirection() {
            return self.get_args();
        }

        let mut args = vec![];
        while self.current.is_some() {
            if self.peek().eq(">") || self.peek().eq("<") || self.peek().eq(">>") {
                break;
            } else {
                args.push(self.next().unwrap());
            }
        }
        args
    }

    pub fn get_args(&mut self) -> Vec<String> {
        let mut args = vec![];
        while let Some(a) = self.next() {
            if a.eq("&&") {
                break;
            }
            args.push(a);
        }
        args
    }

    pub fn peek(&self) -> String {
        let mut res = String::new();
        if let Some(cur) = self.current.as_deref() {
            let mut open = 0u8;
            for c in cur.chars().into_iter() {
                if c.eq(&'"') || c.eq(&'\'') {
                    open = open ^ 1;
                } else if c.eq(&' ') && open == 0 {
                    break;
                } else {
                    res.push(c);
                }
            }
        }
        res
    }

    pub fn has_redirection(&self) -> bool {
        self.has_redirection
    }

    pub fn is_empty(&self) -> bool {
        self.current.is_none()
    }
}

impl Iterator for Tokenizer {
    type Item = String;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(current) = self.current.take() {
            let mut stop = usize::MAX;
            let mut nxt = String::new();
            let mut remainder = String::new();
            let mut open = 0u8;

            for (i, c) in current.chars().enumerate() {
                if c == '"' || c == '\'' {
                    open ^= 1;
                } else if c == ' ' && open == 0 {
                    stop = i + 1;
                    break;
                } else {
                    nxt.push(c);
                }
            }

            if stop < current.len() {
                remainder = current[stop..].to_string();
            }

            if !remainder.is_empty() {
                self.current = Some(remainder);
            }

            if !nxt.is_empty() {
                return Some(nxt);
            }
        }
        None
    }
}
