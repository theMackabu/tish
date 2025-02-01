use regex::Regex;
use std::{cell::RefCell, collections::HashMap, env, process::Command};

enum TemplateToken {
    Space(usize),
    Text(String),
    Variable(String),
    Command(String),
    EnvironmentVariable(String),

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
    EnvVariable(String),
    Literal(String),
    Boolean(Box<ConditionType>),
    Or(Vec<ConditionType>),
    And(Vec<ConditionType>),
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
        for (name, value) in self.updates.to_owned() {
            context.set(name, value);
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
                        TemplateToken::EnvironmentVariable(env_name) => env::var(env_name).unwrap_or_default(),
                        TemplateToken::Variable(var_name) => context.get(var_name).unwrap_or_default(),
                        TemplateToken::StringOperation { source, operations } => {
                            let mut result = match &**source {
                                TemplateToken::Command(cmd) => self.execute_command(cmd),
                                TemplateToken::Variable(var) => context.get(var).unwrap_or_default(),
                                TemplateToken::Text(text) => text.clone(),
                                TemplateToken::EnvironmentVariable(env_name) => env::var(env_name).unwrap_or_default(),
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
                TemplateToken::EnvironmentVariable(name) => {
                    result.push_str(&env::var(name).unwrap_or_default());
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
                                if group_idx < captures.len() {
                                    captures.get(group_idx).map(|m| m.as_str().to_string()).unwrap_or_default()
                                } else {
                                    String::new()
                                }
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
        let mut color_expr = String::new();
        let mut content = Vec::new();
        let mut in_name = true;
        let mut nested = String::new();
        let mut brace_depth = 0;

        chars.next();
        if let Some('.') = chars.next() {}

        while let Some(c) = chars.next() {
            match c {
                '{' if in_name => {
                    brace_depth += 1;
                    color_expr.push(c);
                }
                '}' if in_name => {
                    brace_depth -= 1;
                    color_expr.push(c);
                    if brace_depth == 0 {
                        in_name = false;
                    }
                }
                '>' if in_name && brace_depth == 0 => in_name = false,
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
                _ if in_name => color_expr.push(c),
                _ => nested.push(c),
            }
        }

        let color = if color_expr.starts_with('{') {
            let tokens = self.parse_tokens(&color_expr);
            if let Some(TemplateToken::Conditional { if_body, else_body, .. }) = tokens.first() {
                let mut context = ScopedContext::new();
                let result = self.render_tokens_with_context(if_body, &mut context, &mut PendingUpdates::new());
                if result.is_empty() && else_body.is_some() {
                    self.render_tokens_with_context(else_body.as_ref().unwrap(), &mut context, &mut PendingUpdates::new())
                } else {
                    result
                }
            } else {
                color_expr
            }
        } else {
            color_expr
        };

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

        let trimmed = content.trim();
        if trimmed.starts_with('$') {
            return TemplateToken::EnvironmentVariable(trimmed[1..].to_string());
        }

        if content.starts_with("var ") {
            self.parse_variable_declaration(&content)
        } else if content.contains('|') {
            self.parse_chained_operations(&content)
        } else if content.starts_with("if ") {
            self.parse_conditional(&content[3..])
        } else if content.starts_with("' '") {
            let count = if content.len() > 3 { content[3..].trim().parse().unwrap_or(1) } else { 1 };
            TemplateToken::Space(count)
        } else if content.starts_with("cmd('") {
            TemplateToken::Command(content[4..].trim_matches('\'').trim_matches(')').to_string())
        } else if content.starts_with("match(") || content.starts_with("split(") || content.starts_with("replace(") {
            self.parse_single_operation(&content)
        } else {
            TemplateToken::Variable(content.trim().to_string())
        }
    }

    fn parse_variable_declaration(&self, content: &str) -> TemplateToken {
        let content = content[4..].trim();
        let parts: Vec<&str> = content.split('=').map(|s| s.trim()).collect();

        if parts.len() != 2 {
            return TemplateToken::Text(format!("{{var {}}}", content));
        }

        let name = parts[0].to_string();
        let value = parts[1];

        let value_token = if value.contains('|') {
            if let TemplateToken::StringOperation { source, operations } = self.parse_chained_operations(value) {
                TemplateToken::StringOperation { source, operations }
            } else {
                TemplateToken::Text(value.to_string())
            }
        } else if value.starts_with("cmd('") {
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
            Box::new(TemplateToken::Variable(parts[0].to_string()))
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

        let mut parts = Vec::new();
        let mut current = String::new();
        let mut in_quotes = false;

        for c in args.chars() {
            match c {
                '\'' | '"' => {
                    in_quotes = !in_quotes;
                    current.push(c);
                }
                ',' if !in_quotes => {
                    if !current.is_empty() {
                        parts.push(current.trim().to_string());
                        current = String::new();
                    }
                }
                _ => current.push(c),
            }
        }
        if !current.is_empty() {
            parts.push(current.trim().to_string());
        }

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

    fn evaluate_boolean_condition(&self, condition: &ConditionType, context: &ScopedContext) -> bool {
        self.resolve_condition_value(condition, context).map(|val| Self::is_truthy(&val)).unwrap_or(false)
    }

    fn resolve_condition_value(&self, condition: &ConditionType, context: &ScopedContext) -> Option<String> {
        match condition {
            ConditionType::Command(cmd) => Some(self.execute_command(cmd)),
            ConditionType::Variable(name) => Some(context.get(name).unwrap_or_default()),
            ConditionType::EnvVariable(name) => Some(env::var(name).unwrap_or_default()),
            ConditionType::Literal(val) => Some(val.to_string()),
            ConditionType::Boolean(inner) => self.resolve_condition_value(inner, context).map(|val| Self::is_truthy(&val).to_string()),
            ConditionType::Or(conditions) => Some(
                conditions
                    .iter()
                    .any(|cond| self.resolve_condition_value(cond, context).map(|val| Self::is_truthy(&val)).unwrap_or(false))
                    .to_string(),
            ),
            ConditionType::And(conditions) => Some(
                conditions
                    .iter()
                    .all(|cond| self.resolve_condition_value(cond, context).map(|val| Self::is_truthy(&val)).unwrap_or(false))
                    .to_string(),
            ),
        }
    }

    fn evaluate_condition(&self, condition: &ConditionType, operator: &str, comparison: &str, context: &ScopedContext) -> bool {
        match condition {
            ConditionType::Or(conditions) => conditions.iter().any(|cond| self.evaluate_boolean_condition(cond, context)),
            ConditionType::And(conditions) => conditions.iter().all(|cond| self.evaluate_boolean_condition(cond, context)),
            ConditionType::Boolean(inner_condition) => self.evaluate_boolean_condition(inner_condition, context),
            _ => {
                let value = self.resolve_condition_value(condition, context).unwrap_or_default();

                if operator == "is_truthy" {
                    return Self::is_truthy(&value);
                }

                let comparison_value = if comparison.starts_with('$') {
                    env::var(&comparison[1..]).unwrap_or_default()
                } else {
                    let unquoted = Self::strip_quotes(comparison);
                    context.get(unquoted).unwrap_or(unquoted.to_string())
                };

                match operator {
                    "is_empty" => value.is_empty(),
                    "not_empty" => !value.is_empty(),

                    "equals" | "==" => value == comparison_value,
                    "not_equals" | "!=" => value != comparison_value,
                    "equals_ignore_case" | "ieq" => value.to_lowercase() == comparison_value.to_lowercase(),

                    "contains" | "includes" => value.contains(&comparison_value),
                    "not_contains" | "excludes" => !value.contains(&comparison_value),

                    "length_equals" => value.len() == comparison_value.parse().unwrap_or(0),
                    "length_greater" => value.len() > comparison_value.parse().unwrap_or(0),
                    "length_less" => value.len() < comparison_value.parse().unwrap_or(0),

                    "in" => comparison_value.split(',').map(str::trim).any(|x| x == value),
                    "not_in" => !comparison_value.split(',').map(str::trim).any(|x| x == value),

                    "is_number" => value.parse::<f64>().is_ok(),
                    "is_integer" => value.parse::<i64>().is_ok(),

                    "starts_with" => value.starts_with(&comparison_value),
                    "ends_with" => value.ends_with(&comparison_value),

                    "greater" | ">" => self.compare_values(&value, &comparison_value, |a, b| a > b),
                    "greater_equals" | ">=" => self.compare_values(&value, &comparison_value, |a, b| a >= b),
                    "less" | "<" => self.compare_values(&value, &comparison_value, |a, b| a < b),
                    "less_equals" | "<=" => self.compare_values(&value, &comparison_value, |a, b| a <= b),

                    "matches" => {
                        if let Ok(re) = Regex::new(&comparison_value) {
                            re.is_match(&value)
                        } else {
                            false
                        }
                    }
                    _ => false,
                }
            }
        }
    }

    fn compare_values<F>(&self, value: &str, comparison: &str, compare_fn: F) -> bool
    where
        F: Fn(&str, &str) -> bool + Copy,
    {
        let looks_like_version = |s: &str| s.split('.').all(|part| part.parse::<u32>().is_ok());

        if let (Ok(v), Ok(c)) = (value.parse::<i64>(), comparison.parse::<i64>()) {
            return compare_fn(&v.to_string(), &c.to_string());
        }

        if let (Ok(v), Ok(c)) = (value.parse::<f64>(), comparison.parse::<f64>()) {
            if v.is_nan() || c.is_nan() {
                return false;
            }
            return compare_fn(&v.to_string(), &c.to_string());
        }

        if looks_like_version(value) && looks_like_version(comparison) {
            return self.compare_versions(value, comparison, compare_fn);
        }

        compare_fn(value, comparison)
    }

    fn compare_versions<F>(&self, v1: &str, v2: &str, compare_fn: F) -> bool
    where
        F: Fn(&str, &str) -> bool,
    {
        let v1_parts: Vec<u32> = v1.split('.').filter_map(|x| x.parse().ok()).collect();
        let v2_parts: Vec<u32> = v2.split('.').filter_map(|x| x.parse().ok()).collect();

        let max_len = v1_parts.len().max(v2_parts.len());
        let v1_normalized: String = v1_parts.iter().chain(std::iter::repeat(&0)).take(max_len).map(|n| format!("{:010}", n)).collect();
        let v2_normalized: String = v2_parts.iter().chain(std::iter::repeat(&0)).take(max_len).map(|n| format!("{:010}", n)).collect();

        compare_fn(&v1_normalized, &v2_normalized)
    }

    fn parse_conditional_bodies(&self, content: &str) -> (Vec<TemplateToken>, Option<Vec<TemplateToken>>) {
        let if_body_str = self.extract_block(content);
        let if_body = self.parse_tokens(&if_body_str);

        let else_body = if let Some(else_idx) = content[if_body_str.len()..].find("else") {
            let else_content = &content[if_body_str.len() + else_idx + 4..];
            if else_content.trim().starts_with('{') {
                let else_str = self.extract_block(else_content);
                Some(self.parse_tokens(&else_str))
            } else {
                None
            }
        } else {
            None
        };

        (if_body, else_body)
    }

    fn parse_conditional(&self, content: &str) -> TemplateToken {
        let condition_str = content.split('{').next().unwrap_or("").trim();
        let remaining = &content[condition_str.len()..];

        let (condition, operator, comparison) = if !condition_str.contains(' ') {
            (
                ConditionType::Boolean(Box::new(self.parse_condition_expression(condition_str))),
                String::from("is_truthy"),
                String::new(),
            )
        } else {
            let parts = self.split_conditional_parts(condition_str);
            if parts.len() < 3 {
                return TemplateToken::Text(format!("{{if {} }}", condition_str));
            }
            (
                self.parse_condition_expression(parts[0].trim()),
                parts[1].trim().to_string(),
                parts[2].trim_matches('(').trim_matches(')').trim().to_string(),
            )
        };

        let (if_body, else_body) = self.parse_conditional_bodies(remaining);

        TemplateToken::Conditional {
            condition,
            operator,
            comparison,
            if_body,
            else_body,
        }
    }

    fn split_conditional_parts(&self, condition_str: &str) -> Vec<String> {
        let mut parts = Vec::new();
        let mut current = String::new();
        let mut paren_depth = 0;
        let mut in_quotes = false;
        let mut chars = condition_str.chars().peekable();

        while let Some(c) = chars.next() {
            match c {
                '\'' => {
                    in_quotes = !in_quotes;
                    current.push(c);
                }
                '(' if !in_quotes => {
                    paren_depth += 1;
                    if paren_depth == 1 {
                        continue;
                    }
                    current.push(c);
                }
                ')' if !in_quotes => {
                    paren_depth -= 1;
                    if paren_depth == 0 {
                        parts.push(current.trim().to_string());
                        current = String::new();
                        continue;
                    }
                    current.push(c);
                }
                ' ' if paren_depth == 0 && !in_quotes => {
                    if !current.is_empty() {
                        parts.push(current.trim().to_string());
                        current = String::new();
                    }
                }
                _ => current.push(c),
            }
        }

        if !current.is_empty() {
            parts.push(current.trim().to_string());
        }

        parts.into_iter().filter(|s| !s.is_empty()).map(|s| s.trim().to_string()).collect()
    }

    fn parse_condition_expression(&self, expr: &str) -> ConditionType {
        let parts = self.split_top_level_operator(expr, "||");

        if parts.len() > 1 {
            ConditionType::Or(parts.iter().map(|part| self.parse_and_expression(part)).collect())
        } else {
            self.parse_and_expression(expr)
        }
    }

    fn parse_and_expression(&self, expr: &str) -> ConditionType {
        let parts = self.split_top_level_operator(expr, "&&");

        if parts.len() > 1 {
            ConditionType::And(parts.iter().map(|part| self.parse_single_condition(part)).collect())
        } else {
            self.parse_single_condition(expr)
        }
    }

    fn parse_single_condition(&self, expr: &str) -> ConditionType {
        let clean_expr = expr.trim_matches('(').trim_matches(')').trim();

        if expr.starts_with("cmd('") && expr.ends_with("')") {
            let cmd = expr[5..expr.len() - 2].to_string();
            ConditionType::Command(cmd)
        } else if clean_expr.starts_with('$') {
            ConditionType::EnvVariable(clean_expr[1..].to_string())
        } else if (clean_expr.starts_with('\'') && clean_expr.ends_with('\'')) || (clean_expr.starts_with('"') && clean_expr.ends_with('"')) {
            let literal = clean_expr[1..clean_expr.len() - 1].to_string();
            ConditionType::Literal(literal)
        } else if clean_expr.parse::<f64>().is_ok() {
            ConditionType::Literal(clean_expr.to_string())
        } else {
            ConditionType::Variable(clean_expr.to_string())
        }
    }

    fn split_top_level_operator(&self, expr: &str, operator: &str) -> Vec<String> {
        let mut parts = Vec::new();
        let mut current = String::new();
        let mut paren_depth = 0;
        let mut chars = expr.chars().peekable();
        let operator_chars: Vec<char> = operator.chars().collect();

        while let Some(c) = chars.next() {
            match c {
                '(' => {
                    paren_depth += 1;
                    current.push(c);
                }
                ')' => {
                    paren_depth -= 1;
                    current.push(c);
                }
                c if paren_depth == 0 && c == operator_chars[0] => {
                    let mut is_operator = true;
                    let mut operator_buffer = vec![c];
                    for expected_char in &operator_chars[1..] {
                        if let Some(&next_char) = chars.peek() {
                            if next_char == *expected_char {
                                operator_buffer.push(next_char);
                                chars.next();
                            } else {
                                is_operator = false;
                                break;
                            }
                        } else {
                            is_operator = false;
                            break;
                        }
                    }

                    if is_operator {
                        if !current.is_empty() {
                            parts.push(current.trim().to_string());
                            current = String::new();
                        }
                    } else {
                        current.push(c);
                        current.extend(operator_buffer.iter().skip(1));
                    }
                }
                _ => current.push(c),
            }
        }

        if !current.is_empty() {
            parts.push(current.trim().to_string());
        }

        parts
    }

    fn strip_quotes(s: &str) -> &str {
        if (s.starts_with('\'') && s.ends_with('\'')) || (s.starts_with('"') && s.ends_with('"')) {
            &s[1..s.len() - 1]
        } else {
            s
        }
    }

    fn is_truthy(value: &str) -> bool {
        match value.to_lowercase().as_str() {
            "true" | "yes" | "1" | "on" => true,
            "false" | "no" | "0" | "off" | "" => false,
            other => {
                if let Ok(num) = other.parse::<f64>() {
                    num != 0.0
                } else {
                    !other.is_empty()
                }
            }
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
