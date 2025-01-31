use regex::Regex;
use std::cell::RefCell;
use std::collections::HashMap;
use std::process::Command;

enum TemplateToken {
    Space(usize),
    Text(String),
    Variable(String),
    Command(String),
    ColorTag {
        color: String,
        content: Vec<TemplateToken>,
    },
    FormatTag {
        format_type: FormatType,
        content: Vec<TemplateToken>,
    },
    VariableDeclaration {
        name: String,
        value: Box<TemplateToken>,
    },
    StringOperation {
        source: Box<TemplateToken>,
        operations: Vec<Operation>,
    },
    Conditional {
        condition: ConditionType,
        operator: String,
        comparison: String,
        if_body: Vec<TemplateToken>,
        else_body: Option<Vec<TemplateToken>>,
    },
}

enum ConditionType {
    Command(String),
    Variable(String),
}

enum OperationParam {
    Index(usize),
    ReplaceStr(String),
}

#[derive(Clone, Copy)]
enum FormatType {
    Bold,
    Italic,
    Underline,
}

#[derive(PartialEq)]
enum StringOperationType {
    Match,
    Split,
    Replace,
}

struct Operation {
    operation_type: StringOperationType,
    pattern: Option<String>,
    param: Option<OperationParam>,
}

struct ScopedContext<'c> {
    variables: HashMap<String, String>,
    parent: Option<&'c ScopedContext<'c>>,
}

struct PendingUpdates {
    updates: Vec<(String, String)>,
}

impl PendingUpdates {
    fn new() -> Self { Self { updates: Vec::new() } }

    fn add(&mut self, name: String, value: String) { self.updates.push((name, value)); }

    fn apply(&self, context: &mut ScopedContext) {
        for (name, value) in &self.updates {
            context.set(name.clone(), value.clone());
        }
    }
}

impl<'c> ScopedContext<'c> {
    fn new() -> Self {
        Self {
            variables: HashMap::new(),
            parent: None,
        }
    }

    fn with_parent(parent: &'c ScopedContext<'c>) -> Self {
        Self {
            variables: HashMap::new(),
            parent: Some(parent),
        }
    }

    fn get(&self, key: &str) -> Option<String> {
        if let Some(value) = self.variables.get(key) {
            Some(value.clone())
        } else if let Some(parent) = self.parent {
            parent.get(key)
        } else {
            None
        }
    }

    fn set(&mut self, key: String, value: String) { self.variables.insert(key, value); }
}

pub struct Template<'c> {
    template: String,
    global_context: RefCell<ScopedContext<'c>>,
}

const ANSI_RESET: &str = "\x1b[0m";
const ANSI_BOLD: &str = "\x1b[1m";
const ANSI_ITALIC: &str = "\x1b[3m";
const ANSI_UNDERLINE: &str = "\x1b[4m";

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
            template: template.to_string(),
            global_context: RefCell::new(ScopedContext::new()),
        }
    }

    pub fn insert(&self, key: &'c str, value: String) {
        let mut ctx = self.global_context.borrow_mut();
        ctx.set(key.to_string(), value);
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

    fn get_format_code(&self, format_type: FormatType) -> &'static str {
        match format_type {
            FormatType::Bold => ANSI_BOLD,
            FormatType::Italic => ANSI_ITALIC,
            FormatType::Underline => ANSI_UNDERLINE,
        }
    }

    fn parse_hex_color(color: &str) -> Option<String> {
        let hex = if color.starts_with('#') { &color[1..] } else { color };

        if hex.len() != 6 {
            return None;
        }

        let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
        let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
        let b = u8::from_str_radix(&hex[4..6], 16).ok()?;

        Some(format!("\x1b[38;2;{};{};{}m", r, g, b))
    }

    fn execute_command(&self, cmd: &str) -> String {
        let cmd = cmd.trim_matches('\'').trim_start_matches("cmd(").trim_end_matches(")");
        let mut parts = cmd.split_whitespace();

        let program = match parts.next() {
            Some(p) => p,
            None => return String::new(),
        };

        match Command::new(program).args(parts).output() {
            Ok(output) => String::from_utf8_lossy(&output.stdout).trim().to_string(),
            Err(_) => String::new(),
        }
    }

    pub fn render(&self) -> String {
        let normalized = self
            .template
            .replace("\\n", "\x00") // temporarily replace \n with null char
            .split('\n')
            .map(|s| s.trim())
            .collect::<Vec<_>>()
            .join("")
            .replace("\x00", "\n");

        let tokens = self.parse_tokens(&normalized);
        let global_ref = self.global_context.borrow();
        let mut context = ScopedContext::with_parent(&global_ref);
        let mut pending_updates = PendingUpdates::new();

        let result = self.render_tokens_with_context(&tokens, &mut context, &mut pending_updates);

        // drop the immutable borrow before applying updates
        drop(global_ref);

        if !pending_updates.updates.is_empty() {
            let mut global = self.global_context.borrow_mut();
            pending_updates.apply(&mut global);
        }

        result
    }

    fn render_tokens_with_context(&self, tokens: &[TemplateToken], context: &mut ScopedContext, pending_updates: &mut PendingUpdates) -> String {
        let mut result = String::new();
        let mut has_formatting = false;

        for token in tokens {
            match token {
                TemplateToken::VariableDeclaration { name, value } => {
                    let value_str = match &**value {
                        TemplateToken::Command(cmd) => self.execute_command(cmd),
                        TemplateToken::Text(text) => text.clone(),
                        TemplateToken::Variable(var_name) => context.get(var_name).unwrap_or_default(),
                        TemplateToken::StringOperation { source, operations } => {
                            let mut result = match &**source {
                                TemplateToken::Command(cmd) => self.execute_command(cmd),
                                TemplateToken::Variable(var) => context.get(var).unwrap_or_default(),
                                TemplateToken::Text(text) => text.clone(),
                                _ => String::new(),
                            };

                            for op in operations {
                                result = self.apply_operation(&result, op);
                            }
                            result
                        }
                        _ => String::new(),
                    };

                    context.set(name.to_owned(), value_str.to_owned());
                    pending_updates.add(name.to_owned(), value_str);
                }
                TemplateToken::Text(text) => result.push_str(text),
                TemplateToken::Space(count) => result.push_str(&" ".repeat(*count)),
                TemplateToken::Command(cmd) => {
                    result.push_str(&self.execute_command(cmd));
                }
                TemplateToken::Variable(name) => {
                    if let Some(value) = context.get(name) {
                        result.push_str(&value);
                    }
                }
                TemplateToken::ColorTag { color, content } => {
                    has_formatting = true;
                    result.push_str(&self.get_color_code(color));
                    result.push_str(&self.render_tokens_with_context(content, context, pending_updates));
                    result.push_str(ANSI_RESET);
                }
                TemplateToken::FormatTag { format_type, content } => {
                    has_formatting = true;
                    result.push_str(self.get_format_code(*format_type));
                    result.push_str(&self.render_tokens_with_context(content, context, pending_updates));
                    result.push_str(ANSI_RESET);
                }
                TemplateToken::StringOperation { source, operations } => {
                    let mut op_result = match &**source {
                        TemplateToken::Command(cmd) => self.execute_command(cmd),
                        TemplateToken::Variable(var) => context.get(var).unwrap_or_default(),
                        TemplateToken::Text(text) => text.clone(),
                        _ => String::new(),
                    };

                    for op in operations {
                        op_result = self.apply_operation(&op_result, op);
                    }
                    result.push_str(&op_result);
                }
                TemplateToken::Conditional {
                    condition,
                    operator,
                    comparison,
                    if_body,
                    else_body,
                } => {
                    let mut conditional_context = ScopedContext::with_parent(context);
                    if self.evaluate_condition(condition, operator, comparison, context) {
                        result.push_str(&self.render_tokens_with_context(if_body, &mut conditional_context, pending_updates));
                    } else if let Some(else_tokens) = else_body {
                        result.push_str(&self.render_tokens_with_context(else_tokens, &mut conditional_context, pending_updates));
                    }
                }
            }
        }

        if has_formatting && !result.ends_with(ANSI_RESET) {
            result.push_str(ANSI_RESET);
        }

        result
    }

    fn apply_operation(&self, input: &str, op: &Operation) -> String {
        match op.operation_type {
            StringOperationType::Replace => {
                if let (Some(pattern), Some(OperationParam::ReplaceStr(replacement))) = (&op.pattern, &op.param) {
                    input.replace(pattern, replacement)
                } else {
                    input.to_string()
                }
            }
            StringOperationType::Split => {
                if let (Some(delimiter), Some(OperationParam::Index(index))) = (&op.pattern, &op.param) {
                    let parts: Vec<&str> = input.split(delimiter).collect();
                    if *index < parts.len() {
                        parts[*index].trim().to_string()
                    } else {
                        input.to_string()
                    }
                } else {
                    input.to_string()
                }
            }
            StringOperationType::Match => {
                if let Some(pattern) = &op.pattern {
                    if let Ok(re) = Regex::new(pattern) {
                        if let Some(captures) = re.captures(input) {
                            if let Some(OperationParam::Index(group_idx)) = op.param {
                                captures.get(group_idx).map(|m| m.as_str().to_string()).unwrap_or_default()
                            } else {
                                captures.get(0).map(|m| m.as_str().to_string()).unwrap_or_default()
                            }
                        } else {
                            String::new()
                        }
                    } else {
                        input.to_string()
                    }
                } else {
                    input.to_string()
                }
            }
        }
    }

    fn parse_tokens(&self, template: &str) -> Vec<TemplateToken> {
        let mut tokens = Vec::new();
        let mut chars = template.chars().peekable();
        let mut current_text = String::new();

        while let Some(c) = chars.next() {
            match c {
                '<' => {
                    if !current_text.is_empty() {
                        tokens.push(TemplateToken::Text(current_text.clone()));
                        current_text.clear();
                    }

                    if let Some(&next_char) = chars.peek() {
                        if next_char == 'c' {
                            tokens.push(self.parse_color_tag(&mut chars));
                        } else {
                            tokens.push(self.parse_format_tag(&mut chars));
                        }
                    }
                }
                '{' => {
                    if !current_text.is_empty() {
                        tokens.push(TemplateToken::Text(current_text.clone()));
                        current_text.clear();
                    }
                    tokens.push(self.parse_special_token(&mut chars));
                }
                _ => current_text.push(c),
            }
        }

        if !current_text.is_empty() {
            tokens.push(TemplateToken::Text(current_text));
        }

        tokens
    }

    fn parse_format_tag(&self, chars: &mut std::iter::Peekable<std::str::Chars>) -> TemplateToken {
        let mut tag_name = String::new();
        let mut content = Vec::new();
        let mut in_name = true;
        let mut nested = String::new();

        while let Some(c) = chars.next() {
            match c {
                '>' if in_name => in_name = false,
                '<' if !in_name => {
                    if chars.peek() == Some(&'/') {
                        while let Some(c) = chars.next() {
                            if c == '>' {
                                break;
                            }
                        }
                        break;
                    } else {
                        nested.push(c);
                    }
                }
                _ if in_name => tag_name.push(c),
                _ => nested.push(c),
            }
        }

        let format_type = match tag_name.as_str() {
            "b" => FormatType::Bold,
            "i" => FormatType::Italic,
            "u" => FormatType::Underline,
            _ => return TemplateToken::Text(format!("<{}>", tag_name)),
        };

        if !nested.is_empty() {
            content = self.parse_tokens(&nested);
        }

        TemplateToken::FormatTag { format_type, content }
    }

    fn parse_color_tag(&self, chars: &mut std::iter::Peekable<std::str::Chars>) -> TemplateToken {
        let mut color = String::new();
        let mut content = Vec::new();
        let mut in_name = true;
        let mut nested = String::new();

        while let Some(c) = chars.next() {
            match c {
                '>' if in_name => in_name = false,
                '<' if !in_name => {
                    if chars.peek() == Some(&'/') {
                        while let Some(c) = chars.next() {
                            if c == '>' {
                                break;
                            }
                        }
                        break;
                    } else {
                        nested.push(c);
                    }
                }
                _ if in_name => {
                    if c == 'c' && chars.peek() == Some(&'.') {
                        chars.next();
                        continue;
                    }
                    color.push(c);
                }
                _ => nested.push(c),
            }
        }

        if !nested.is_empty() {
            content = self.parse_tokens(&nested);
        }

        TemplateToken::ColorTag { color, content }
    }

    fn parse_special_token(&self, chars: &mut std::iter::Peekable<std::str::Chars>) -> TemplateToken {
        let mut content = String::new();
        let mut depth = 1;

        while let Some(c) = chars.next() {
            match c {
                '{' => {
                    depth += 1;
                    content.push(c);
                }
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                    content.push(c);
                }
                _ => content.push(c),
            }
        }

        if content.contains('|') && (content.contains("split(") || content.contains("match(") || content.contains("replace(")) {
            self.parse_chained_operations(&content)
        } else if content.starts_with("var ") {
            self.parse_variable_declaration(&content[4..])
        } else if content.starts_with("' '") {
            let count = if content.len() > 3 { content[3..].trim().parse().unwrap_or(1) } else { 1 };
            TemplateToken::Space(count)
        } else if content.starts_with("if ") {
            self.parse_conditional(&content[3..])
        } else if content.starts_with("cmd('") {
            TemplateToken::Command(content[4..].trim_matches('\'').trim_matches(')').to_string())
        } else if content.starts_with("match(") || content.starts_with("split(") || content.starts_with("replace(") {
            self.parse_single_operation(&content)
        } else {
            TemplateToken::Variable(content)
        }
    }

    fn parse_variable_declaration(&self, content: &str) -> TemplateToken {
        let parts: Vec<&str> = content.splitn(2, '=').collect();
        if parts.len() != 2 {
            return TemplateToken::Text(format!("{{var {}}}", content));
        }

        let name = parts[0].trim().to_string();
        let value = parts[1].trim();

        if value.contains('|') {
            if let TemplateToken::StringOperation { source, operations } = self.parse_chained_operations(value) {
                return TemplateToken::VariableDeclaration {
                    name,
                    value: Box::new(TemplateToken::StringOperation { source, operations }),
                };
            }
        }

        let value_token = if value.starts_with("cmd('") {
            TemplateToken::Command(value[4..].trim_matches('\'').trim_matches(')').to_string())
        } else if value.starts_with('\'') && value.ends_with('\'') {
            TemplateToken::Text(value[1..value.len() - 1].to_string())
        } else {
            TemplateToken::Variable(value.to_string())
        };

        TemplateToken::VariableDeclaration { name, value: Box::new(value_token) }
    }

    fn parse_chained_operations(&self, content: &str) -> TemplateToken {
        let parts: Vec<&str> = content.split('|').map(|s| s.trim()).collect();
        if parts.is_empty() {
            return TemplateToken::Text(content.to_string());
        }

        let source = if parts[0].starts_with("cmd('") {
            Box::new(TemplateToken::Command(parts[0][5..parts[0].len() - 2].to_string()))
        } else {
            Box::new(TemplateToken::Variable(parts[0].trim().to_string()))
        };

        let mut operations = Vec::new();
        for part in parts.iter().skip(1) {
            if let Some(op) = self.parse_operation(part) {
                operations.push(op);
            }
        }

        TemplateToken::StringOperation { source, operations }
    }

    fn parse_operation(&self, op_str: &str) -> Option<Operation> {
        let (op_type, args) = if op_str.starts_with("match(") {
            (StringOperationType::Match, &op_str[6..op_str.len() - 1])
        } else if op_str.starts_with("split(") {
            (StringOperationType::Split, &op_str[6..op_str.len() - 1])
        } else if op_str.starts_with("replace(") {
            (StringOperationType::Replace, &op_str[8..op_str.len() - 1])
        } else {
            return None;
        };

        let parts: Vec<&str> = args.split(',').map(|s| s.trim()).filter(|s| !s.is_empty()).collect();
        let pattern = parts.get(0).map(|p| p.trim_matches('\'').trim_matches('"').to_string());

        let param = match op_type {
            StringOperationType::Replace => parts.get(1).map(|r| OperationParam::ReplaceStr(r.trim_matches('\'').trim_matches('"').to_string())),
            _ => parts.get(1).and_then(|g| g.trim().parse().ok()).map(OperationParam::Index),
        };

        Some(Operation {
            operation_type: op_type,
            pattern,
            param,
        })
    }

    fn parse_single_operation(&self, content: &str) -> TemplateToken {
        if let Some(op) = self.parse_operation(content) {
            TemplateToken::StringOperation {
                source: Box::new(TemplateToken::Text(String::new())),
                operations: vec![op],
            }
        } else {
            TemplateToken::Text(content.to_string())
        }
    }

    fn evaluate_condition(&self, condition: &ConditionType, operator: &str, comparison: &str, context: &ScopedContext) -> bool {
        let value = match condition {
            ConditionType::Command(cmd) => self.execute_command(cmd),
            ConditionType::Variable(var_name) => {
                if let Some(value) = context.get(var_name) {
                    value
                } else {
                    return false;
                }
            }
        };

        match operator {
            "equals" => value == comparison,
            "contains" => value.contains(comparison),
            "startswith" => value.starts_with(comparison),
            "endswith" => value.ends_with(comparison),
            "matches" => {
                if let Ok(re) = Regex::new(comparison) {
                    re.is_match(&value)
                } else {
                    false
                }
            }
            _ => false,
        }
    }

    fn parse_conditional(&self, content: &str) -> TemplateToken {
        let condition_str = content.split('{').next().unwrap_or("").trim();

        let (condition, rest) = if let Some(cmd_start) = condition_str.find("cmd('") {
            if let Some(cmd_end) = condition_str[cmd_start..].find("')") {
                let cmd = &condition_str[cmd_start + 5..cmd_start + cmd_end];
                let rest = &condition_str[cmd_start + cmd_end + 2..];
                (ConditionType::Command(cmd.to_string()), rest.trim())
            } else {
                return TemplateToken::Text(content.to_string());
            }
        } else {
            let parts: Vec<&str> = condition_str.split_whitespace().collect();
            if parts.is_empty() {
                return TemplateToken::Text(content.to_string());
            }

            let var_name = parts[0].trim_matches('(').trim_matches(')').to_string();
            let rest = condition_str.split_once(char::is_whitespace).map(|(_, r)| r.trim()).unwrap_or("");

            (ConditionType::Variable(var_name), rest)
        };

        let rest_parts: Vec<&str> = rest.split_whitespace().collect();
        if rest_parts.len() != 2 {
            return TemplateToken::Text(content.to_string());
        }

        let operator = rest_parts[0].to_string();
        let comparison = rest_parts[1].trim_matches('\'').to_string();
        let remaining = &content[condition_str.len()..];
        let if_body_str = self.extract_block(remaining);
        let if_body = self.parse_tokens(&if_body_str);

        let else_body = if let Some(else_idx) = remaining[if_body_str.len()..].find("else") {
            let else_content = &remaining[if_body_str.len() + else_idx + 4..];
            if else_content.trim().starts_with('{') {
                let else_str = self.extract_block(else_content);
                Some(self.parse_tokens(&else_str))
            } else {
                None
            }
        } else {
            None
        };

        TemplateToken::Conditional {
            condition,
            operator,
            comparison,
            if_body,
            else_body,
        }
    }

    fn extract_block(&self, content: &str) -> String {
        let mut depth = 0;
        let mut start_pos = 0;
        let mut end_pos = 0;
        let mut in_block = false;

        for (i, c) in content.chars().enumerate() {
            match c {
                '{' => {
                    if !in_block {
                        in_block = true;
                        start_pos = i + 1;
                    }
                    depth += 1;
                }
                '}' => {
                    depth -= 1;
                    if depth == 0 && in_block {
                        end_pos = i;
                        break;
                    }
                }
                _ => {}
            }
        }

        if in_block && end_pos > start_pos {
            content[start_pos..end_pos].trim().to_string()
        } else {
            String::new()
        }
    }
}
