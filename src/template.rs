use anyhow::{anyhow, Error};
use regex::Regex;
use serde::Deserialize;

use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    env,
    iter::Peekable,
    process::Command,
    str::Chars,
};

#[derive(Debug, Clone)]
enum StyleType {
    Color(String),
    Rgb(u8, u8, u8),
    Format(FormatType),
}

#[derive(Deserialize, Debug, Clone)]
enum Value {
    String(String),
    Number(f64),
    Array(Vec<Value>),
    Map(HashMap<String, Value>),
    Bool(bool),
}

impl std::fmt::Display for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Value::String(s) => write!(f, "{}", s),
            Value::Number(n) => write!(f, "{}", n),
            Value::Bool(b) => write!(f, "{}", b),
            Value::Array(arr) => {
                write!(f, "[")?;
                for (i, val) in arr.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", val)?;
                }
                write!(f, "]")
            }
            Value::Map(map) => {
                write!(f, "{{")?;
                for (i, (key, val)) in map.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "\"{}\": {}", key, val)?;
                }
                write!(f, "}}")
            }
        }
    }
}

enum StyleParserState {
    CollectingStyle,
    WaitingForContent,
    CollectingContent,
}

#[derive(Debug)]
enum TemplateToken {
    Text(String),
    Variable(String),
    Command(String),
    EnvironmentVariable(String),
    Array(Vec<TemplateToken>),

    Loop {
        iterator: Box<TemplateToken>,
        loop_var: String,
        index_var: Option<String>,
        body: Vec<TemplateToken>,
    },

    Partial {
        path: String,
    },

    Repeat {
        content: String,
        count: usize,
    },

    StyleTag {
        style: StyleType,
        content: Vec<TemplateToken>,
    },

    DynamicStyleTag {
        style_tokens: Vec<TemplateToken>,
        content: Vec<TemplateToken>,
    },

    VariableDeclaration {
        name: String,
        value: Box<TemplateToken>,
        is_constant: bool,
    },

    VariableAssignment {
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

#[derive(Debug, Clone, PartialEq)]
enum Operator {
    // equality
    Equals,
    NotEquals,
    EqualsIgnoreCase,

    // content checks
    Contains,
    NotContains,
    StartsWith,
    EndsWith,
    Matches,

    // collection operations
    In,
    NotIn,

    // length operations
    IsEmpty,
    NotEmpty,
    LengthEquals,
    LengthGreater,
    LengthLess,

    // numeric comparisons
    Greater,
    GreaterEquals,
    Less,
    LessEquals,

    // type checks
    IsNumber,
    IsInteger,
}

impl Operator {
    fn from_str(s: &str) -> Option<Self> {
        use Operator::*;
        match s {
            "equals" | "==" => Some(Equals),
            "not_equals" | "!=" => Some(NotEquals),
            "equals_ignore_case" | "ieq" => Some(EqualsIgnoreCase),
            "contains" | "includes" => Some(Contains),
            "not_contains" | "excludes" => Some(NotContains),
            "starts_with" => Some(StartsWith),
            "ends_with" => Some(EndsWith),
            "matches" => Some(Matches),
            "in" => Some(In),
            "not_in" => Some(NotIn),
            "is_empty" => Some(IsEmpty),
            "not_empty" => Some(NotEmpty),
            "length_equals" => Some(LengthEquals),
            "length_greater" => Some(LengthGreater),
            "length_less" => Some(LengthLess),
            "greater" | ">" => Some(Greater),
            "greater_equals" | ">=" => Some(GreaterEquals),
            "less" | "<" => Some(Less),
            "less_equals" | "<=" => Some(LessEquals),
            "is_number" => Some(IsNumber),
            "is_integer" => Some(IsInteger),
            _ => None,
        }
    }

    fn all_operators() -> &'static [&'static str] {
        &[
            "equals",
            "==",
            "not_equals",
            "!=",
            "equals_ignore_case",
            "ieq",
            "contains",
            "includes",
            "not_contains",
            "excludes",
            "starts_with",
            "ends_with",
            "matches",
            "in",
            "not_in",
            "is_empty",
            "not_empty",
            "length_equals",
            "length_greater",
            "length_less",
            "greater",
            ">",
            "greater_equals",
            ">=",
            "less",
            "<",
            "less_equals",
            "<=",
            "is_number",
            "is_integer",
        ]
    }
}

#[derive(Debug, Clone)]
enum ConditionType {
    Command(String),
    Variable(String),
    EnvVariable(String),
    Literal(String),
    Boolean(Box<ConditionType>, bool),
    Or(Vec<ConditionType>),
    And(Vec<ConditionType>),

    Compare { lhs: Box<ConditionType>, operator: String, rhs: Box<ConditionType> },
    StringOperation { source: Box<ConditionType>, operations: Vec<Operation> },
}

#[derive(Debug, Clone)]
enum OperationParam {
    Index(usize),
    ReplaceStr(String),
}

#[derive(Debug, Clone, Copy)]
enum FormatType {
    Bold,
    Italic,
    Underline,
}

#[derive(Debug, Clone, PartialEq)]
enum StringOperationType {
    Match,
    Split,
    Replace,
    DefaultValue,
}

#[derive(Debug, Clone)]
struct Operation {
    operation_type: StringOperationType,
    pattern: Option<String>,
    param: Option<OperationParam>,
}

#[derive(Debug)]
struct ScopedContext<'c> {
    variables: HashMap<String, String>,
    constants: HashSet<String>,
    parent: Option<&'c ScopedContext<'c>>,
}

#[derive(Debug)]
struct PendingUpdates {
    updates: Vec<(String, String, bool)>,
}

impl PendingUpdates {
    fn new() -> Self { Self { updates: Vec::new() } }

    fn add(&mut self, name: String, value: String, is_constant: bool) { self.updates.push((name, value, is_constant)); }

    fn apply(self, context: &mut ScopedContext) -> Result<(), Error> {
        for (name, value, is_constant) in self.updates {
            if is_constant || !context.variables.contains_key(&name) {
                context.declare(name, value, is_constant);
            } else {
                context.set(name, value)?;
            }
        }
        Ok(())
    }

    fn is_empty(&self) -> bool { self.updates.is_empty() }
}

impl<'c> ScopedContext<'c> {
    fn new() -> Self {
        Self {
            variables: HashMap::new(),
            constants: HashSet::new(),
            parent: None,
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

    fn set(&mut self, key: String, value: String) -> Result<(), Error> {
        if self.constants.contains(&key) {
            return Err(anyhow!("Cannot reassign constant variable '{}'", key));
        }

        if !self.variables.contains_key(&key) {
            return Err(anyhow!("Variable '{}' not declared", key));
        }

        self.variables.insert(key, value);
        Ok(())
    }

    fn declare(&mut self, key: String, value: String, is_constant: bool) {
        if is_constant {
            self.constants.insert(key.clone());
        }
        self.variables.insert(key, value);
    }
}

type State<'c> = (ScopedContext<'c>, PendingUpdates);

pub struct Template<'c> {
    template: String,
    state: RefCell<State<'c>>,
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
            state: RefCell::new((ScopedContext::new(), PendingUpdates::new())),
        }
    }

    pub fn insert(&self, key: &'c str, value: String) {
        let mut state = self.state.borrow_mut();
        state.0.declare(key.to_string(), value, true);
    }

    fn parse_static_style(&self, style_str: &str) -> StyleType {
        let style_str = style_str.trim();

        match style_str {
            "bold" | "b" => StyleType::Format(FormatType::Bold),
            "italic" | "i" => StyleType::Format(FormatType::Italic),
            "underline" | "u" => StyleType::Format(FormatType::Underline),

            // rgb(r,g,b)
            s if s.starts_with("rgb(") && s.ends_with(")") => {
                let rgb = s[4..s.len() - 1].split(',').map(|n| n.trim().parse().unwrap_or(0)).collect::<Vec<u8>>();

                if rgb.len() == 3 {
                    StyleType::Rgb(rgb[0], rgb[1], rgb[2])
                } else {
                    StyleType::Color("reset".to_string())
                }
            }

            // #RRGGBB or #RGB
            s if s.starts_with('#') && (s.len() == 7 || s.len() == 4) => match s.len() {
                7 => {
                    if let (Ok(r), Ok(g), Ok(b)) = (u8::from_str_radix(&s[1..3], 16), u8::from_str_radix(&s[3..5], 16), u8::from_str_radix(&s[5..7], 16)) {
                        StyleType::Rgb(r, g, b)
                    } else {
                        StyleType::Color("reset".to_string())
                    }
                }
                4 => {
                    if let (Ok(r), Ok(g), Ok(b)) = (
                        u8::from_str_radix(&format!("{}{}", &s[1..2], &s[1..2]), 16),
                        u8::from_str_radix(&format!("{}{}", &s[2..3], &s[2..3]), 16),
                        u8::from_str_radix(&format!("{}{}", &s[3..4], &s[3..4]), 16),
                    ) {
                        StyleType::Rgb(r, g, b)
                    } else {
                        StyleType::Color("reset".to_string())
                    }
                }
                _ => StyleType::Color("reset".to_string()),
            },

            color_name if ANSI_COLORS.iter().any(|(name, _)| *name == color_name) => StyleType::Color(color_name.to_string()),

            _ => StyleType::Color("reset".to_string()),
        }
    }

    fn execute_command(&self, cmd: &str) -> String {
        let cmd = cmd.trim_matches('\'').trim_start_matches("cmd(").trim_end_matches(")");

        let parts: Vec<String> = cmd
            .split('"')
            .enumerate()
            .map(|(i, s)| if i % 2 == 0 { s.split_whitespace().map(String::from).collect() } else { vec![s.to_string()] })
            .flatten()
            .filter(|s| !s.is_empty())
            .collect();

        if parts.is_empty() {
            return String::new();
        }

        match Command::new(&parts[0]).args(&parts[1..]).output() {
            Ok(output) => String::from_utf8_lossy(&output.stdout).trim().to_string(),
            Err(_) => String::new(),
        }
    }

    fn normalize(content: &String) -> String {
        content
            .replace("\\n", "\x00") // temporarily replace \n with null char
            .split('\n')
            .map(|s| s.trim())
            .collect::<Vec<_>>()
            .join("")
            .replace("\x00", "\n")
    }

    pub fn render(&self) -> Result<String, Error> {
        let mut state = self.state.borrow_mut();

        let normalized = Self::normalize(&self.template);
        let tokens = self.parse_tokens(&normalized, &mut state);
        let result = self.render_tokens_with_context(&tokens, &mut state);

        if !state.1.is_empty() {
            let updates = std::mem::replace(&mut state.1, PendingUpdates::new());
            updates.apply(&mut state.0)?;
        }

        Ok(result)
    }

    fn evaluate_token_value(&self, token: &TemplateToken, state: &mut State) -> String {
        match token {
            TemplateToken::Command(cmd) => self.execute_command(cmd),
            TemplateToken::Text(text) => text.clone(),
            TemplateToken::EnvironmentVariable(env_name) => env::var(env_name).unwrap_or_default(),

            TemplateToken::Variable(name) => {
                if name.contains('[') || name.contains('.') {
                    self.evaluate_complex_variable(name, state)
                } else {
                    state.0.get(name).unwrap_or_default()
                }
            }

            TemplateToken::StringOperation { source, operations } => {
                let mut result = self.evaluate_token_value(source, state);
                for op in operations {
                    result = self.apply_operation(&result, op);
                }
                result
            }

            TemplateToken::Conditional {
                condition,
                operator,
                comparison,
                if_body,
                else_body,
            } => {
                if self.evaluate_condition(condition, operator, comparison, &state.0) {
                    self.render_tokens_with_context(if_body, state)
                } else if let Some(else_tokens) = else_body {
                    self.render_tokens_with_context(else_tokens, state)
                } else {
                    String::new()
                }
            }

            _ => String::new(),
        }
    }

    fn evaluate_complex_variable(&self, expr: &str, state: &mut State) -> String {
        let mut parts = Vec::new();
        for seg in expr.split('.') {
            if seg.contains('[') {
                let mut subparts = seg.split('[');
                if let Some(first) = subparts.next() {
                    if !first.is_empty() {
                        parts.push(first.to_string());
                    }
                }
                for idx in subparts {
                    let idx = idx.trim_end_matches(']');
                    parts.push(idx.to_string());
                }
            } else {
                parts.push(seg.to_string());
            }
        }

        let base_name = parts.get(0).unwrap();
        let mut value_str = state.0.get(base_name).unwrap_or_default();

        if (value_str.starts_with('[') && value_str.ends_with(']')) || (value_str.starts_with('{') && value_str.ends_with('}')) {
            value_str = value_str.replace("'", "\"");
        }

        for accessor in parts.iter().skip(1) {
            if let Ok(index) = accessor.parse::<usize>() {
                if value_str.starts_with('[') {
                    if let Ok(json_val) = serde_json::from_str::<serde_json::Value>(&value_str) {
                        if let Some(item) = json_val.get(index) {
                            value_str = match item {
                                serde_json::Value::String(s) => s.to_string(),
                                _ => item.to_string(),
                            };
                        } else {
                            value_str = String::new();
                        }
                    }
                } else {
                    let trimmed = value_str.trim_matches(|c| c == '[' || c == ']');
                    let items: Vec<&str> = trimmed.split(',').map(|s| s.trim().trim_matches('\'').trim_matches('"')).collect();
                    value_str = if index < items.len() { items[index].to_string() } else { String::new() };
                }
            } else {
                if value_str.starts_with('{') {
                    if let Ok(json_obj) = serde_json::from_str::<serde_json::Value>(&value_str) {
                        if let Some(prop) = json_obj.get(accessor) {
                            value_str = match prop {
                                serde_json::Value::String(s) => s.to_string(),
                                _ => prop.to_string(),
                            };
                        } else {
                            value_str = String::new();
                        }
                    }
                }
            }
        }

        value_str
    }

    fn render_tokens_with_context(&self, tokens: &[TemplateToken], state: &mut State) -> String {
        let mut result = String::new();
        let mut errors = Vec::new();
        let mut has_formatting = false;

        for token in tokens {
            match token {
                TemplateToken::Array(items) => {
                    let mut array_values = Vec::new();
                    for item in items {
                        array_values.push(self.evaluate_token_value(item, state));
                    }
                    result.push_str(&array_values.join(", "));
                }

                TemplateToken::Loop { iterator, loop_var, index_var, body } => {
                    result.push_str(&self.render_loop(iterator, loop_var, index_var, body, state));
                }

                TemplateToken::VariableDeclaration { name, value, is_constant } => {
                    let value_str = match &**value {
                        TemplateToken::Variable(v) if v.starts_with('[') => v.to_string(),
                        _ => self.evaluate_token_value(value, state),
                    };
                    state.0.declare(name.clone(), value_str, *is_constant);
                }

                TemplateToken::VariableAssignment { name, value } => {
                    let value_str = self.evaluate_token_value(value, state);
                    if let Err(err) = state.0.set(name.clone(), value_str.clone()) {
                        errors.push(err.to_string());
                    } else {
                        state.1.add(name.clone(), value_str, false);
                    }
                }

                TemplateToken::DynamicStyleTag { style_tokens, content } => {
                    has_formatting = true;

                    let style_str = self.render_tokens_with_context(style_tokens, state);
                    let style = self.parse_static_style(&style_str);

                    match &style {
                        StyleType::Color(name) => result.push_str(ANSI_COLORS.iter().find(|(ansi_name, _)| *ansi_name == name).map_or("", |(_, code)| code)),

                        StyleType::Rgb(r, g, b) => result.push_str(&format!("\x1b[38;2;{};{};{}m", r, g, b)),

                        StyleType::Format(format_type) => {
                            result.push_str(match format_type {
                                FormatType::Bold => ANSI_BOLD,
                                FormatType::Italic => ANSI_ITALIC,
                                FormatType::Underline => ANSI_UNDERLINE,
                            });
                        }
                    }

                    result.push_str(&self.render_tokens_with_context(content, state));
                    result.push_str(ANSI_RESET);
                }

                TemplateToken::Partial { path } => {
                    if let Ok(content) = std::fs::read_to_string(path) {
                        let partial_template = Template::new(&content);
                        let normalized = Self::normalize(&content);

                        let mut partial_state = (
                            ScopedContext {
                                variables: state.0.variables.clone(),
                                constants: state.0.constants.clone(),
                                parent: Some(&state.0),
                            },
                            PendingUpdates::new(),
                        );

                        result.push_str(&partial_template.render_tokens_with_context(&partial_template.parse_tokens(&normalized, &mut partial_state), &mut partial_state));

                        if !partial_state.1.is_empty() {
                            let updates = std::mem::replace(&mut partial_state.1, PendingUpdates::new());
                            if let Err(e) = updates.apply(&mut state.0) {
                                errors.push(e.to_string());
                            }
                        }
                    }
                }

                TemplateToken::Text(text) => result.push_str(text),

                TemplateToken::Repeat { content, count } => {
                    let parts: Vec<&str> = content.split('\'').collect();
                    if parts.len() >= 3 {
                        let text = parts[1];
                        result.push_str(&text.repeat(*count));
                    }
                }

                TemplateToken::Command(cmd) => {
                    result.push_str(&self.execute_command(cmd));
                }

                TemplateToken::Variable(name) => match state.0.get(name) {
                    Some(value) => result.push_str(&value),
                    None => result.push_str(&self.evaluate_token_value(token, state)),
                },

                TemplateToken::EnvironmentVariable(name) => {
                    result.push_str(&env::var(name).unwrap_or_default());
                }

                TemplateToken::StyleTag { style, content } => {
                    has_formatting = true;
                    match style {
                        StyleType::Color(name) => result.push_str(ANSI_COLORS.iter().find(|(ansi_name, _)| *ansi_name == name).map_or("", |(_, code)| code)),

                        StyleType::Rgb(r, g, b) => result.push_str(&format!("\x1b[38;2;{};{};{}m", r, g, b)),

                        StyleType::Format(format_type) => {
                            result.push_str(match format_type {
                                FormatType::Bold => ANSI_BOLD,
                                FormatType::Italic => ANSI_ITALIC,
                                FormatType::Underline => ANSI_UNDERLINE,
                            });
                        }
                    }
                    result.push_str(&self.render_tokens_with_context(content, state));
                    result.push_str(ANSI_RESET);
                }
                TemplateToken::StringOperation { source, operations } => {
                    let mut op_result = self.evaluate_token_value(source, state);
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
                    if self.evaluate_condition(condition, operator, comparison, &state.0) {
                        result.push_str(&self.render_tokens_with_context(if_body, state));
                    } else if let Some(else_tokens) = else_body {
                        result.push_str(&self.render_tokens_with_context(else_tokens, state));
                    }
                }
            }
        }

        if has_formatting && !result.ends_with(ANSI_RESET) {
            result.push_str(ANSI_RESET);
        }

        if !errors.is_empty() {
            format!("{}\n{}", errors.join("\n"), result)
        } else {
            result
        }
    }

    fn render_loop(&self, iterator: &TemplateToken, loop_var: &str, index_var: &Option<String>, body: &[TemplateToken], state: &mut State) -> String {
        let mut result = String::new();

        match iterator {
            TemplateToken::Variable(var_name) => {
                if let Some(array_value) = state.0.get(var_name) {
                    let array_content = array_value.trim_matches('[').trim_matches(']');

                    if !array_content.contains('{') {
                        let items: Vec<String> = array_content.split(',').map(|s| s.trim().trim_matches('\'').trim_matches('"').to_string()).collect();

                        for (i, item) in items.iter().enumerate() {
                            let mut loop_state = (
                                ScopedContext {
                                    variables: HashMap::new(),
                                    constants: HashSet::new(),
                                    parent: Some(&state.0),
                                },
                                PendingUpdates::new(),
                            );

                            loop_state.0.declare(loop_var.to_string(), item.clone(), false);

                            if let Some(idx_var) = index_var {
                                loop_state.0.declare(idx_var.clone(), i.to_string(), false);
                            }

                            result.push_str(&self.render_tokens_with_context(body, &mut loop_state));
                        }
                    } else {
                        let mut current_object = String::new();
                        let mut depth = 0;
                        let mut objects = Vec::new();

                        for c in array_content.chars() {
                            match c {
                                '{' => {
                                    depth += 1;
                                    current_object.push(c);
                                }
                                '}' => {
                                    depth -= 1;
                                    current_object.push(c);
                                    if depth == 0 {
                                        objects.push(current_object.trim().to_string());
                                        current_object = String::new();
                                    }
                                }
                                ',' if depth == 0 => continue,
                                _ => {
                                    if depth > 0 {
                                        current_object.push(c);
                                    }
                                }
                            }
                        }

                        for (i, obj) in objects.iter().enumerate() {
                            let mut loop_state = (
                                ScopedContext {
                                    variables: HashMap::new(),
                                    constants: HashSet::new(),
                                    parent: Some(&state.0),
                                },
                                PendingUpdates::new(),
                            );

                            loop_state.0.declare(loop_var.to_string(), obj.to_string(), false);

                            if let Some(idx_var) = index_var {
                                loop_state.0.declare(idx_var.clone(), i.to_string(), false);
                            }

                            result.push_str(&self.render_tokens_with_context(body, &mut loop_state));
                        }
                    }
                }
            }
            TemplateToken::Array(items) => {
                for (i, item) in items.iter().enumerate() {
                    let item_value = self.evaluate_token_value(item, state);

                    let mut loop_state = (
                        ScopedContext {
                            variables: HashMap::new(),
                            constants: HashSet::new(),
                            parent: Some(&state.0),
                        },
                        PendingUpdates::new(),
                    );

                    loop_state.0.declare(loop_var.to_string(), item_value, false);

                    if let Some(idx_var) = index_var {
                        loop_state.0.declare(idx_var.clone(), i.to_string(), false);
                    }

                    result.push_str(&self.render_tokens_with_context(body, &mut loop_state));
                }
            }
            _ => {}
        }

        result
    }

    fn parse_array(&self, content: &str) -> TemplateToken {
        let content = content.trim_matches(|c| c == '[' || c == ']');
        let mut items = Vec::new();
        let mut current = String::new();
        let mut depth = 0;
        let mut in_quotes = false;

        for c in content.chars() {
            match c {
                '\'' | '"' if !in_quotes => {
                    in_quotes = true;
                    current.push(c);
                }
                '\'' | '"' if in_quotes => {
                    in_quotes = false;
                    current.push(c);
                }
                '[' | '{' if !in_quotes => {
                    depth += 1;
                    current.push(c);
                }
                ']' | '}' if !in_quotes => {
                    depth -= 1;
                    current.push(c);
                }
                ',' if !in_quotes && depth == 0 => {
                    if !current.is_empty() {
                        items.push(TemplateToken::Text(current.trim_matches(|c| c == '\'' || c == '"').to_string()));
                        current.clear();
                    }
                }
                _ => current.push(c),
            }
        }

        if !current.is_empty() {
            items.push(TemplateToken::Text(current.trim_matches(|c| c == '\'' || c == '"').to_string()));
        }

        TemplateToken::Array(items)
    }

    fn apply_operation(&self, input: &str, op: &Operation) -> String {
        match op.operation_type {
            StringOperationType::DefaultValue => {
                if input.is_empty() {
                    op.pattern.as_ref().map_or(String::new(), |default| default.clone())
                } else {
                    input.to_string()
                }
            }

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
                                if group_idx > 0 && group_idx <= captures.len() {
                                    return captures.get(group_idx).map(|m| m.as_str().to_string()).unwrap_or_default();
                                }
                            }
                            return captures.get(0).map(|m| m.as_str().to_string()).unwrap_or_default();
                        }
                    }
                }
                String::new()
            }
        }
    }

    fn parse_tokens(&self, template: &str, state: &mut State) -> Vec<TemplateToken> {
        let mut tokens = Vec::new();
        let mut chars = template.chars().peekable();
        let mut current_text = String::new();

        while let Some(c) = chars.next() {
            match c {
                '<' => {
                    if chars.peek().map_or(false, |&next| next == 's') && {
                        chars.next();
                        chars.peek().map_or(false, |&next| next == '.')
                    } {
                        if !current_text.is_empty() {
                            tokens.push(TemplateToken::Text(current_text.clone()));
                            current_text.clear();
                        }
                        tokens.push(self.parse_style_tag(&mut chars, state));
                    } else {
                        current_text.push('<');
                    }
                }
                '{' => {
                    if !current_text.is_empty() {
                        tokens.push(TemplateToken::Text(current_text.clone()));
                        current_text.clear();
                    }
                    tokens.push(self.parse_special_token(&mut chars, state));
                }
                _ => current_text.push(c),
            }
        }

        if !current_text.is_empty() {
            tokens.push(TemplateToken::Text(current_text));
        }

        tokens
    }

    fn parse_style_tag(&self, chars: &mut Peekable<Chars>, state: &mut State) -> TemplateToken {
        chars.next(); // Skip '.'

        let mut style_expr = String::new();
        let mut content = Vec::new();
        let mut nested = String::new();
        let mut brace_depth = 0;
        let mut parser_state = StyleParserState::CollectingStyle;

        while let Some(c) = chars.next() {
            match (c, &parser_state) {
                ('{', StyleParserState::CollectingStyle) => {
                    brace_depth += 1;
                    style_expr.push(c);
                }
                ('}', StyleParserState::CollectingStyle) => {
                    brace_depth -= 1;
                    style_expr.push(c);
                }
                ('>', StyleParserState::CollectingStyle) if brace_depth == 0 => {
                    parser_state = StyleParserState::WaitingForContent;
                }
                ('<', StyleParserState::CollectingContent) => {
                    if chars.peek() == Some(&'/') {
                        chars.next(); // skip '/'
                        chars.next(); // skip 's'
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
                (c, StyleParserState::CollectingStyle) => {
                    if !c.is_whitespace() || brace_depth > 0 {
                        style_expr.push(c);
                    }
                }
                (c, StyleParserState::WaitingForContent) => {
                    parser_state = StyleParserState::CollectingContent;
                    nested.push(c);
                }
                (c, StyleParserState::CollectingContent) => {
                    nested.push(c);
                }
            }
        }

        if !nested.is_empty() {
            content = self.parse_tokens(&nested, state);
        }

        if style_expr.starts_with('{') && style_expr.ends_with('}') {
            let style_tokens = self.parse_tokens(&style_expr, state);
            return TemplateToken::DynamicStyleTag { style_tokens, content };
        }

        let style = self.parse_static_style(&style_expr);
        TemplateToken::StyleTag { style, content }
    }

    fn parse_special_token(&self, chars: &mut std::iter::Peekable<std::str::Chars>, state: &mut State) -> TemplateToken {
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

        if let Some(colon_pos) = trimmed.find(':') {
            let left = trimmed[..colon_pos].trim();
            let right = trimmed[colon_pos + 1..].trim();

            if left.starts_with("cmd('") && left.ends_with("')") {
                return TemplateToken::VariableAssignment {
                    name: right.to_string(),
                    value: Box::new(TemplateToken::Command(left[5..left.len() - 2].to_string())),
                };
            }

            if left.starts_with('$') && right.starts_with("cmd('") && right.ends_with("')") {
                return TemplateToken::VariableAssignment {
                    name: left[1..].to_string(),
                    value: Box::new(TemplateToken::Command(right[5..right.len() - 2].to_string())),
                };
            }

            let var_name = left;
            let default_value = right;

            if !var_name.contains(|c: char| !c.is_alphanumeric() && c != '_')
                && ((default_value.starts_with('\'') && default_value.ends_with('\'')) || (default_value.starts_with('"') && default_value.ends_with('"')))
            {
                return TemplateToken::StringOperation {
                    source: Box::new(TemplateToken::Variable(var_name.to_string())),
                    operations: vec![Operation {
                        operation_type: StringOperationType::DefaultValue,
                        pattern: Some(Self::strip_quotes(default_value).to_string()),
                        param: None,
                    }],
                };
            }
        }

        if trimmed.starts_with("for ") {
            let loop_content = &trimmed[4..];
            if let Some(in_pos) = loop_content.find(" in ") {
                let var_part = &loop_content[..in_pos];
                let (loop_var, index_var) = if let Some((v, i)) = var_part.split_once(',') {
                    (v.trim().to_string(), Some(i.trim().to_string()))
                } else {
                    (var_part.trim().to_string(), None)
                };

                if let Some(brace_pos) = loop_content[in_pos..].find('{') {
                    let iterator_expr = &loop_content[in_pos + 4..in_pos + brace_pos].trim();

                    let iterator = if iterator_expr.starts_with('[') {
                        Box::new(self.parse_array(iterator_expr))
                    } else if iterator_expr.contains("..") {
                        let range: Vec<&str> = iterator_expr.split("..").collect();
                        if range.len() == 2 {
                            if let (Ok(start), Ok(end)) = (range[0].trim().parse::<i64>(), range[1].trim().parse::<i64>()) {
                                let numbers: Vec<TemplateToken> = (start..end).map(|n| TemplateToken::Text(n.to_string())).collect();
                                Box::new(TemplateToken::Array(numbers))
                            } else {
                                Box::new(TemplateToken::Text(iterator_expr.to_string()))
                            }
                        } else {
                            Box::new(self.parse_value_token(iterator_expr))
                        }
                    } else {
                        Box::new(self.parse_value_token(iterator_expr))
                    };

                    let body_content = self.extract_block(&loop_content[in_pos + brace_pos..]);
                    let body = self.parse_tokens(&body_content, state);

                    return TemplateToken::Loop { iterator, loop_var, index_var, body };
                }
            }
        }

        if trimmed.starts_with('>') {
            return TemplateToken::Partial {
                path: trimmed[1..].trim().to_string(),
            };
        }

        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            return self.parse_array(trimmed);
        }

        if trimmed.starts_with('$') {
            return TemplateToken::EnvironmentVariable(trimmed[1..].to_string());
        }

        if trimmed.starts_with("if ") {
            return self.parse_conditional(&trimmed[3..], state);
        }

        if !trimmed.starts_with("let") && !trimmed.starts_with("const") && trimmed.contains('=') {
            return self.parse_variable_assignment(&content);
        }

        if content.starts_with("let ") || trimmed.starts_with("const") {
            self.parse_variable_declaration(&content)
        } else if content.contains('|') {
            self.parse_chained_operations(&content)
        } else if content.starts_with('\'') {
            let mut after_first_quote = false;
            let mut found_second_quote = false;
            let mut count_start = 0;

            for (i, c) in content.chars().enumerate() {
                if c == '\'' {
                    if !after_first_quote {
                        after_first_quote = true;
                    } else {
                        found_second_quote = true;
                        count_start = i + 1;
                        break;
                    }
                }
            }

            if found_second_quote {
                let count = if count_start < content.len() { content[count_start..].trim().parse().unwrap_or(1) } else { 1 };
                TemplateToken::Repeat { content: content.to_string(), count }
            } else {
                TemplateToken::Text(content.to_string())
            }
        } else if content.starts_with("cmd('") {
            TemplateToken::Command(content[4..].trim_matches('\'').trim_matches(')').to_string())
        } else if content.starts_with("match(") || content.starts_with("split(") || content.starts_with("replace(") {
            self.parse_single_operation(&content)
        } else {
            TemplateToken::Variable(content.trim().to_string())
        }
    }

    fn parse_variable_assignment(&self, content: &str) -> TemplateToken {
        let parts: Vec<&str> = content.split('=').map(|s| s.trim()).collect();

        if parts.len() != 2 {
            return TemplateToken::Text(format!("{{Invalid assignment: {}}}", content));
        }

        let name = parts[0].to_string();
        let value = if parts[1].trim().starts_with("if ") {
            Box::new(self.parse_conditional(parts[1], &mut (ScopedContext::new(), PendingUpdates::new())))
        } else {
            Box::new(self.parse_value_token(parts[1]))
        };

        TemplateToken::VariableAssignment { name, value }
    }

    fn parse_variable_declaration(&self, content: &str) -> TemplateToken {
        let content = content.trim();
        let is_constant = content.starts_with("const ");
        let content = if is_constant {
            &content[6..]
        } else {
            &content[4..] // skip "let "
        };

        let mut parts = content.splitn(2, '=');
        let name = parts.next().unwrap().trim().to_string();
        let value_str = parts.next().unwrap_or("").trim();

        if name.is_empty() || value_str.is_empty() {
            return TemplateToken::Text(format!("{{Invalid declaration: {}}}", content));
        }

        let value = if value_str.starts_with("if ") {
            Box::new(self.parse_conditional(value_str, &mut (ScopedContext::new(), PendingUpdates::new())))
        } else if value_str.contains('|') {
            Box::new(self.parse_chained_operations(value_str))
        } else {
            Box::new(self.parse_value_token(value_str))
        };

        TemplateToken::VariableDeclaration { name, value, is_constant }
    }

    fn parse_value_token(&self, value: &str) -> TemplateToken {
        if value.starts_with("cmd('") {
            TemplateToken::Command(value[4..].trim_matches('\'').trim_matches(')').to_string())
        } else if value.starts_with('\'') && value.ends_with('\'') {
            TemplateToken::Text(value[1..value.len() - 1].to_string())
        } else if value.starts_with('$') {
            TemplateToken::EnvironmentVariable(value[1..].to_string())
        } else if value == "true" || value == "false" || value == "yes" || value == "no" {
            TemplateToken::Text(value.to_string())
        } else if value.parse::<i64>().is_ok() || value.parse::<f64>().is_ok() {
            TemplateToken::Text(value.to_string())
        } else {
            TemplateToken::Variable(value.to_string())
        }
    }

    fn parse_chained_operations(&self, content: &str) -> TemplateToken {
        let parts: Vec<&str> = content.split('|').map(str::trim).collect();
        if parts.is_empty() {
            return TemplateToken::Text(content.to_string());
        }

        let source = if parts[0].contains(':') {
            if let Some(colon_pos) = parts[0].find(':') {
                let var_name = parts[0][..colon_pos].trim();
                let default_value = parts[0][colon_pos + 1..].trim();

                TemplateToken::StringOperation {
                    source: Box::new(TemplateToken::Variable(var_name.to_string())),
                    operations: vec![Operation {
                        operation_type: StringOperationType::DefaultValue,
                        pattern: Some(Self::strip_quotes(default_value).to_string()),
                        param: None,
                    }],
                }
            } else {
                TemplateToken::Variable(parts[0].to_string())
            }
        } else if parts[0].starts_with("cmd('") {
            TemplateToken::Command(parts[0][5..parts[0].len() - 2].to_string())
        } else if parts[0].starts_with('\'') && parts[0].ends_with('\'') {
            TemplateToken::Text(parts[0][1..parts[0].len() - 1].to_string())
        } else {
            TemplateToken::Variable(parts[0].to_string())
        };

        let mut operations = Vec::new();
        for part in parts.iter().skip(1) {
            if let Some(op) = self.parse_operation(part) {
                operations.push(op);
            }
        }

        TemplateToken::StringOperation { source: Box::new(source), operations }
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

    fn resolve_value(&self, condition: &ConditionType, context: &ScopedContext) -> Option<String> {
        match condition {
            ConditionType::Command(cmd) => Some(self.execute_command(cmd)),
            ConditionType::Variable(name) => Some(context.get(name).unwrap_or_default()),
            ConditionType::EnvVariable(name) => Some(env::var(name).unwrap_or_default()),
            ConditionType::Literal(val) => Some(val.to_string()),
            _ => None,
        }
    }

    fn apply_operator(&self, lhs: &str, op: Operator, rhs: &str) -> bool {
        use Operator::*;
        match op {
            IsEmpty => lhs.is_empty(),
            NotEmpty => !lhs.is_empty(),
            Equals => lhs == rhs,
            NotEquals => lhs != rhs,
            EqualsIgnoreCase => lhs.to_lowercase() == rhs.to_lowercase(),
            Contains => lhs.contains(rhs),
            NotContains => !lhs.contains(rhs),
            StartsWith => lhs.starts_with(rhs),
            EndsWith => lhs.ends_with(rhs),
            Matches => Regex::new(rhs).map(|re| re.is_match(lhs)).unwrap_or(false),
            In => rhs.split(',').map(str::trim).any(|x| x == lhs),
            NotIn => !rhs.split(',').map(str::trim).any(|x| x == lhs),
            LengthEquals => lhs.len() == rhs.parse().unwrap_or(0),
            LengthGreater => lhs.len() > rhs.parse().unwrap_or(0),
            LengthLess => lhs.len() < rhs.parse().unwrap_or(0),
            IsNumber => lhs.parse::<f64>().is_ok(),
            IsInteger => lhs.parse::<i64>().is_ok(),

            Greater => self.compare_values(lhs, rhs, |a, b| {
                if let (Ok(a_num), Ok(b_num)) = (a.parse::<f64>(), b.parse::<f64>()) {
                    return a_num > b_num;
                }
                a > b
            }),
            Less => self.compare_values(lhs, rhs, |a, b| {
                if let (Ok(a_num), Ok(b_num)) = (a.parse::<f64>(), b.parse::<f64>()) {
                    return a_num < b_num;
                }
                a < b
            }),
            GreaterEquals => self.compare_values(lhs, rhs, |a, b| {
                if let (Ok(a_num), Ok(b_num)) = (a.parse::<f64>(), b.parse::<f64>()) {
                    return a_num >= b_num;
                }
                a >= b
            }),
            LessEquals => self.compare_values(lhs, rhs, |a, b| {
                if let (Ok(a_num), Ok(b_num)) = (a.parse::<f64>(), b.parse::<f64>()) {
                    return a_num <= b_num;
                }
                a <= b
            }),
        }
    }

    fn evaluate_condition(&self, condition: &ConditionType, operator: &str, comparison: &str, context: &ScopedContext) -> bool {
        if !operator.is_empty() && operator != "is_truthy" {
            let comparison_condition = if comparison.starts_with('$') {
                ConditionType::EnvVariable(comparison[1..].to_string())
            } else {
                let unquoted = Self::strip_quotes(comparison);
                if let Some(var_value) = context.get(unquoted) {
                    ConditionType::Literal(var_value)
                } else {
                    ConditionType::Literal(unquoted.to_string())
                }
            };

            return self.evaluate_condition_internal(
                &ConditionType::Compare {
                    lhs: Box::new(condition.to_owned()),
                    operator: operator.to_string(),
                    rhs: Box::new(comparison_condition),
                },
                context,
            );
        }

        self.evaluate_condition_internal(condition, context)
    }

    fn evaluate_condition_internal(&self, condition: &ConditionType, context: &ScopedContext) -> bool {
        match condition {
            ConditionType::Or(conditions) => conditions.iter().any(|cond| self.evaluate_condition_internal(cond, context)),
            ConditionType::And(conditions) => conditions.iter().all(|cond| self.evaluate_condition_internal(cond, context)),

            ConditionType::StringOperation { source, operations } => {
                let mut result = self.resolve_value(source, context).unwrap_or_default();
                for op in operations {
                    result = self.apply_operation(&result, op);
                }
                Self::is_truthy(&result)
            }

            ConditionType::Compare { lhs, operator, rhs } => {
                let lhs_val = self.resolve_value(lhs, context);

                match operator.as_str() {
                    "is_empty" | "not_empty" | "is_number" | "is_integer" => {
                        if let Some(lhs) = lhs_val {
                            return self.apply_operator(&lhs, Operator::from_str(operator).unwrap_or(Operator::Equals), "");
                        }
                        false
                    }
                    _ => {
                        let rhs_val = self.resolve_value(rhs, context);
                        if let (Some(lhs), Some(rhs)) = (lhs_val, rhs_val) {
                            self.apply_operator(&lhs, Operator::from_str(operator).unwrap_or(Operator::Equals), &rhs)
                        } else {
                            false
                        }
                    }
                }
            }
            ConditionType::Boolean(inner, negate) => {
                let result = if let Some(val) = self.resolve_value(inner, context) {
                    Self::is_truthy(&val)
                } else {
                    self.evaluate_condition_internal(inner, context)
                };
                if *negate {
                    !result
                } else {
                    result
                }
            }
            _ => {
                if let Some(val) = self.resolve_value(condition, context) {
                    Self::is_truthy(&val)
                } else {
                    false
                }
            }
        }
    }

    fn compare_values<F>(&self, value: &str, comparison: &str, compare_fn: F) -> bool
    where
        F: Fn(&str, &str) -> bool + Copy,
    {
        if let (Ok(v1), Ok(v2)) = (value.trim().parse::<i64>(), comparison.trim().parse::<i64>()) {
            return compare_fn(&v1.to_string(), &v2.to_string());
        }

        if let (Ok(v1), Ok(v2)) = (value.trim().parse::<f64>(), comparison.trim().parse::<f64>()) {
            if !v1.is_nan() && !v2.is_nan() {
                return compare_fn(&v1.to_string(), &v2.to_string());
            }
        }

        let looks_like_version = |s: &str| s.split('.').all(|part| part.parse::<u32>().is_ok());
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

    fn parse_conditional(&self, content: &str, state: &mut State) -> TemplateToken {
        let mut content_remaining = content;
        let mut if_chain = Vec::new();

        loop {
            if let Some(condition_end) = self.find_next_condition_end(content_remaining) {
                let condition_str = &content_remaining[..condition_end];
                let condition = self.parse_condition_expression(condition_str);

                let (body_content, remaining) = self.extract_next_body(&content_remaining[condition_end..]);
                let body = self.parse_tokens(&body_content, state);

                if_chain.push((condition, body));
                content_remaining = remaining.trim();

                if let Some(else_if_pos) = content_remaining.find("else if") {
                    content_remaining = &content_remaining[else_if_pos + 7..];
                    continue;
                }
                break;
            }
            break;
        }

        let else_body = if let Some(else_pos) = content_remaining.find("else") {
            let else_content = &content_remaining[else_pos + 4..];
            let else_block = self.extract_block(else_content);
            Some(self.parse_tokens(&else_block, state))
        } else {
            None
        };

        if if_chain.is_empty() {
            return TemplateToken::Text(String::new());
        }

        let (last_condition, last_body) = if_chain.pop().unwrap();

        let mut result = TemplateToken::Conditional {
            condition: last_condition,
            operator: String::new(),
            comparison: String::new(),
            if_body: last_body,
            else_body,
        };

        while let Some((condition, body)) = if_chain.pop() {
            result = TemplateToken::Conditional {
                condition,
                operator: String::new(),
                comparison: String::new(),
                if_body: body,
                else_body: Some(vec![result]),
            };
        }

        result
    }

    fn find_next_condition_end(&self, content: &str) -> Option<usize> {
        let mut depth = 0;
        let mut in_quotes = false;
        let mut in_parens = false;

        for (i, c) in content.chars().enumerate() {
            match c {
                '\'' | '"' => in_quotes = !in_quotes,
                '(' if !in_quotes => {
                    in_parens = true;
                    depth += 1;
                }
                ')' if !in_quotes => {
                    depth -= 1;
                    if depth == 0 {
                        in_parens = false;
                    }
                }
                '{' if !in_quotes && !in_parens => {
                    return Some(i);
                }
                _ => {}
            }
        }
        None
    }

    fn extract_next_body<'a>(&self, content: &'a str) -> (String, &'a str) {
        let mut depth = 0;
        let mut in_quotes = false;
        let mut body_start = 0;
        let mut body_end = 0;
        let mut started = false;

        for (i, c) in content.chars().enumerate() {
            match c {
                '\'' | '"' => in_quotes = !in_quotes,
                '{' if !in_quotes => {
                    depth += 1;
                    if depth == 1 {
                        body_start = i + 1;
                        started = true;
                    }
                }
                '}' if !in_quotes => {
                    depth -= 1;
                    if depth == 0 && started {
                        body_end = i;
                        break;
                    }
                }
                _ => {}
            }
        }

        if body_end > body_start {
            let body = content[body_start..body_end].trim().to_string();
            let remaining = if body_end + 1 < content.len() { &content[body_end + 1..] } else { "" };
            (body, remaining)
        } else {
            (String::new(), content)
        }
    }

    fn parse_condition_expression(&self, expr: &str) -> ConditionType {
        let expr = expr.trim();

        let or_parts: Vec<&str> = expr.split("||").map(str::trim).collect();
        if or_parts.len() > 1 {
            return ConditionType::Or(or_parts.iter().map(|part| self.parse_and_expression(part)).collect());
        }

        self.parse_and_expression(expr)
    }

    fn parse_and_expression(&self, expr: &str) -> ConditionType {
        let expr = expr.trim();

        let and_parts: Vec<&str> = expr.split("&&").map(str::trim).collect();
        if and_parts.len() > 1 {
            return ConditionType::And(and_parts.iter().map(|part| self.parse_single_condition(part)).collect());
        }

        self.parse_single_condition(expr)
    }

    fn is_operator_boundary(c: char) -> bool { c.is_whitespace() || c == '(' || c == ')' || c == '{' || c == '}' || c == '.' || c == ',' || c == ';' }

    fn parse_single_condition(&self, expr: &str) -> ConditionType {
        let clean_expr = expr.trim();
        let clean_expr = if clean_expr.starts_with("if ") { clean_expr[3..].trim() } else { clean_expr };

        match clean_expr.to_lowercase().as_str() {
            "true" | "yes" | "on" => return ConditionType::Boolean(Box::new(ConditionType::Literal("true".to_string())), false),
            "false" | "no" | "off" => return ConditionType::Boolean(Box::new(ConditionType::Literal("false".to_string())), false),
            _ => {}
        }

        if clean_expr.starts_with('!') {
            let inner = self.parse_single_condition(&clean_expr[1..]);
            return ConditionType::Boolean(Box::new(inner), true);
        }

        if clean_expr.contains('|') {
            let parts: Vec<&str> = clean_expr.split('|').map(str::trim).collect();
            let source = parts[0].trim();
            let mut operations = Vec::new();

            for part in parts.iter().skip(1) {
                if let Some(op) = self.parse_operation(part) {
                    operations.push(op);
                }
            }

            return ConditionType::StringOperation {
                source: Box::new(if source.starts_with('$') {
                    ConditionType::EnvVariable(source[1..].to_string())
                } else {
                    ConditionType::Variable(source.to_string())
                }),
                operations,
            };
        } else if clean_expr.contains(':') {
            let parts: Vec<&str> = clean_expr.split(':').collect();
            if parts.len() == 2 {
                let var_name = parts[0].trim();
                let default_value = parts[1].trim();
                return ConditionType::StringOperation {
                    source: Box::new(ConditionType::Variable(var_name.to_string())),
                    operations: vec![Operation {
                        operation_type: StringOperationType::DefaultValue,
                        pattern: Some(Self::strip_quotes(default_value).to_string()),
                        param: None,
                    }],
                };
            }
        }

        if clean_expr.starts_with("cmd('") && clean_expr.ends_with("')") {
            return ConditionType::Command(clean_expr[5..clean_expr.len() - 2].to_string());
        }

        let clean_expr = clean_expr.trim_matches('(').trim_matches(')').trim();

        if (clean_expr.starts_with('\'') && clean_expr.ends_with('\'')) || (clean_expr.starts_with('"') && clean_expr.ends_with('"')) {
            return ConditionType::Literal(Self::strip_quotes(clean_expr).to_string());
        }

        let mut operators = Operator::all_operators().to_vec();
        operators.sort_by(|a, b| b.len().cmp(&a.len()));

        for &op in operators.iter() {
            let mut in_quotes = false;
            let mut quote_char = None;
            let mut potential_split = None;

            for (i, c) in clean_expr.chars().enumerate() {
                match c {
                    '\'' | '"' => {
                        if let Some(q) = quote_char {
                            if c == q {
                                in_quotes = false;
                                quote_char = None;
                            }
                        } else {
                            in_quotes = true;
                            quote_char = Some(c);
                        }
                    }
                    _ => {
                        if !in_quotes {
                            if clean_expr[i..].starts_with(op) {
                                let before_ok = i == 0 || Self::is_operator_boundary(clean_expr.chars().nth(i - 1).unwrap());
                                let after_idx = i + op.len();
                                let after_ok = after_idx >= clean_expr.len() || Self::is_operator_boundary(clean_expr.chars().nth(after_idx).unwrap());

                                if before_ok && after_ok {
                                    potential_split = Some(i);
                                    break;
                                }
                            }
                        }
                    }
                }
            }

            if let Some(index) = potential_split {
                let (lhs, rhs) = clean_expr.split_at(index);
                let rhs = &rhs[op.len()..];

                match op {
                    "is_empty" | "not_empty" | "is_number" | "is_integer" => {
                        return ConditionType::Compare {
                            lhs: Box::new(self.parse_single_condition(lhs.trim())),
                            operator: op.to_string(),
                            rhs: Box::new(ConditionType::Literal(String::new())),
                        };
                    }
                    _ => {
                        return ConditionType::Compare {
                            lhs: Box::new(self.parse_single_condition(lhs.trim())),
                            operator: op.to_string(),
                            rhs: Box::new(self.parse_single_condition(rhs.trim())),
                        };
                    }
                }
            }
        }

        if clean_expr.parse::<f64>().is_ok() {
            return ConditionType::Literal(clean_expr.to_string());
        }

        match clean_expr {
            expr if expr.starts_with('$') => ConditionType::EnvVariable(expr[1..].to_string()),
            expr => ConditionType::Variable(expr.to_string()),
        }
    }

    fn strip_quotes(s: &str) -> &str {
        if (s.starts_with('\'') && s.ends_with('\'')) || (s.starts_with('"') && s.ends_with('"')) {
            if s.len() >= 2 {
                &s[1..s.len() - 1]
            } else {
                s
            }
        } else {
            s
        }
    }

    fn is_truthy(value: &str) -> bool {
        !value.is_empty()
            && match value.to_lowercase().as_str() {
                "false" | "no" | "0" | "off" => false,
                other => other.parse::<f64>().map_or(true, |num| num != 0.0),
            }
    }

    fn extract_block(&self, content: &str) -> String {
        let mut depth = 0;
        let mut in_quotes = false;
        let mut quote_char = None;
        let mut start_pos = None;
        let mut end_pos = None;

        for (i, c) in content.chars().enumerate() {
            match c {
                '\'' | '"' if !in_quotes => {
                    in_quotes = true;
                    quote_char = Some(c);
                }
                c if Some(c) == quote_char => {
                    in_quotes = false;
                    quote_char = None;
                }
                '{' if !in_quotes => {
                    if depth == 0 {
                        start_pos = Some(i + 1);
                    }
                    depth += 1;
                }
                '}' if !in_quotes => {
                    depth -= 1;
                    if depth == 0 {
                        end_pos = Some(i);
                        break;
                    }
                }
                _ => {}
            }
        }

        if let (Some(start), Some(end)) = (start_pos, end_pos) {
            content[start..end].trim().to_string()
        } else {
            String::new()
        }
    }
}
