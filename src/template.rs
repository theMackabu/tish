use std::collections::HashMap;
use std::process::Command;

pub struct Template<'c> {
    start: String,
    end: String,

    color_start: String,
    color_end: String,

    shell_start: String,
    shell_end: String,

    template: String,
    context: HashMap<&'c str, String>,
}

const ANSI_RESET: &str = "\x1b[0m";

const ANSI_COLORS: &[(&str, &str)] = &[
    ("reset", ANSI_RESET),
    ("black", "\x1b[30m"),
    ("red", "\x1b[31m"),
    ("green", "\x1b[32m"),
    ("yellow", "\x1b[33m"),
    ("blue", "\x1b[34m"),
    ("magenta", "\x1b[35m"),
    ("cyan", "\x1b[36m"),
    ("white", "\x1b[37m"),
    ("bright_black", "\x1b[90m"),
    ("bright_red", "\x1b[91m"),
    ("bright_green", "\x1b[92m"),
    ("bright_yellow", "\x1b[93m"),
    ("bright_blue", "\x1b[94m"),
    ("bright_magenta", "\x1b[95m"),
    ("bright_cyan", "\x1b[96m"),
    ("bright_white", "\x1b[97m"),
    ("on_black", "\x1b[40m"),
    ("on_red", "\x1b[41m"),
    ("on_green", "\x1b[42m"),
    ("on_yellow", "\x1b[43m"),
    ("on_blue", "\x1b[44m"),
    ("on_magenta", "\x1b[45m"),
    ("on_cyan", "\x1b[46m"),
    ("on_white", "\x1b[47m"),
    ("on_bright_black", "\x1b[100m"),
    ("on_bright_red", "\x1b[101m"),
    ("on_bright_green", "\x1b[102m"),
    ("on_bright_yellow", "\x1b[103m"),
    ("on_bright_blue", "\x1b[104m"),
    ("on_bright_magenta", "\x1b[105m"),
    ("on_bright_cyan", "\x1b[106m"),
    ("on_bright_white", "\x1b[107m"),
];

impl<'c> Template<'c> {
    pub fn new(template: &str) -> Self {
        Self {
            start: "{t.".to_string(),
            end: "}".to_string(),

            color_start: "{c.".to_string(),
            color_end: "}".to_string(),

            shell_start: "{s.".to_string(),
            shell_end: "}".to_string(),

            context: HashMap::new(),
            template: template.to_string(),
        }
    }

    pub fn insert(&mut self, key: &'c str, value: String) { self.context.insert(key, value); }

    pub fn render(&self) -> String {
        let mut result = self.template.clone();

        for (key, value) in &self.context {
            let placeholder = format!("{}{}{}", self.start, key, self.end);
            result = result.replace(&placeholder, value);
        }

        let mut shell_result = String::new();
        let mut current_pos = 0;

        while let Some(start_idx) = result[current_pos..].find(&self.shell_start) {
            let start_idx = current_pos + start_idx;
            if let Some(end_idx) = result[start_idx..].find(&self.shell_end) {
                let end_idx = start_idx + end_idx;

                shell_result.push_str(&result[current_pos..start_idx]);

                let cmd = &result[start_idx + self.shell_start.len()..end_idx];
                let cmd_output = self.execute_command(cmd);
                shell_result.push_str(&cmd_output);

                current_pos = end_idx + self.shell_end.len();
            } else {
                break;
            }
        }

        shell_result.push_str(&result[current_pos..]);
        result = shell_result;

        let mut final_result = String::new();
        current_pos = 0;
        let mut has_colors = false;

        while let Some(start_idx) = result[current_pos..].find(&self.color_start) {
            let start_idx = current_pos + start_idx;
            if let Some(end_idx) = result[start_idx..].find(&self.color_end) {
                let end_idx = start_idx + end_idx;

                final_result.push_str(&result[current_pos..start_idx]);

                let color_name = &result[start_idx + self.color_start.len()..end_idx];
                let color_code = self.get_color_code(color_name);
                if !color_code.is_empty() {
                    has_colors = true;
                }
                final_result.push_str(&color_code);

                current_pos = end_idx + self.color_end.len();
            } else {
                break;
            }
        }

        final_result.push_str(&result[current_pos..]);

        if has_colors && !final_result.ends_with(ANSI_RESET) {
            final_result.push_str(ANSI_RESET);
        }

        final_result
    }

    fn get_color_code(&self, color: &str) -> String {
        if let Some(hex_code) = Self::parse_hex_color(color) {
            return hex_code;
        }

        for (name, code) in ANSI_COLORS {
            if *name == color {
                return code.to_string();
            }
        }

        String::new()
    }

    fn parse_hex_color(hex: &str) -> Option<String> {
        let (is_background, hex) = if hex.starts_with("on_#") {
            (true, hex.trim_start_matches("on_#"))
        } else if hex.starts_with('#') {
            (false, hex.trim_start_matches('#'))
        } else {
            return None;
        };

        if hex.len() != 6 {
            return None;
        }

        let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
        let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
        let b = u8::from_str_radix(&hex[4..6], 16).ok()?;

        if is_background {
            Some(format!("\x1b[48;2;{};{};{}m", r, g, b))
        } else {
            Some(format!("\x1b[38;2;{};{};{}m", r, g, b))
        }
    }

    fn execute_command(&self, cmd: &str) -> String {
        let cmd = cmd.trim_matches('\'');

        let mut parts = cmd.split_whitespace();
        let program = match parts.next() {
            Some(p) => p,
            None => return String::new(),
        };

        match Command::new(program).args(parts).output() {
            Ok(output) if output.status.success() => String::from_utf8_lossy(&output.stdout).trim().to_string(),
            _ => String::new(),
        }
    }
}
