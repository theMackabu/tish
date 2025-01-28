use std::path::PathBuf;

pub struct EnvManager {
    input: String,
    pos: usize,
}

impl EnvManager {
    pub fn new(input: &str) -> Self {
        Self { input: input.to_string(), pos: 0 }
    }

    pub fn expand(&mut self) -> String {
        let mut result = String::new();
        let mut word_start = true;

        while let Some(c) = self.next_char() {
            match c {
                '$' => {
                    result.push_str(&self.expand_variable());
                    word_start = false;
                }
                '~' if word_start => {
                    result.push_str(&self.expand_home());
                    word_start = false;
                }
                c if c.is_whitespace() => {
                    result.push(c);
                    word_start = true;
                }
                c => {
                    result.push(c);
                    word_start = false;
                }
            }
        }

        result
    }

    pub fn expand_variable(&mut self) -> String {
        let var_name = if self.peek_char() == Some('{') {
            self.next_char();
            self.take_until('}')
        } else {
            self.take_while(|c| c.is_alphanumeric() || c == '_')
        };

        if var_name.is_empty() {
            return "$".to_string();
        }

        std::env::var(&var_name).unwrap_or_default()
    }

    pub fn contract_home(&mut self) -> String {
        let path = PathBuf::from(self.take_while(|c| !c.is_whitespace()));

        if let Ok(home) = std::env::var("HOME") {
            let home_path = PathBuf::from(&home);

            if let Ok(stripped) = path.strip_prefix(home_path) {
                if stripped.as_os_str().is_empty() {
                    return "~".to_string();
                }
                return format!("~/{}", stripped.display());
            }
        }

        path.display().to_string()
    }

    pub fn expand_home(&mut self) -> String {
        let path = self.take_while(|c| !c.is_whitespace());

        if path.is_empty() || path.starts_with('/') {
            if let Ok(home) = std::env::var("HOME") {
                return format!("{home}{path}");
            }
        } else {
            let (user, rest) = path.split_once('/').unwrap_or((&path, ""));
            #[cfg(unix)]
            {
                if let Ok(username) = std::ffi::CString::new(user) {
                    let passwd = unsafe { libc::getpwnam(username.as_ptr()) };
                    if !passwd.is_null() {
                        let home = unsafe { std::ffi::CStr::from_ptr((*passwd).pw_dir) }.to_string_lossy();
                        return format!("{home}/{rest}");
                    }
                }
            }
        }

        format!("~{path}")
    }

    fn next_char(&mut self) -> Option<char> {
        if self.pos < self.input.len() {
            let c = self.input[self.pos..].chars().next().unwrap();
            self.pos += c.len_utf8();
            Some(c)
        } else {
            None
        }
    }

    fn peek_char(&self) -> Option<char> {
        if self.pos < self.input.len() {
            self.input[self.pos..].chars().next()
        } else {
            None
        }
    }

    fn take_while<F>(&mut self, predicate: F) -> String
    where
        F: Fn(char) -> bool,
    {
        let mut result = String::new();
        while let Some(c) = self.peek_char() {
            if !predicate(c) {
                break;
            }
            result.push(c);
            self.next_char();
        }
        result
    }

    fn take_until(&mut self, end: char) -> String {
        self.take_while(|c| c != end)
    }
}
