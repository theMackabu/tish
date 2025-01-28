use mlua::{Function, Result, Value};
use std::collections::HashMap;

pub fn create_function(lua: &mlua::Lua) -> Result<Function> {
    lua.create_function(|lua, value: Value| {
        let mut inspector = Inspector::new(20);
        inspector.inspect_value(lua, &value)?;
        Ok(inspector.buf.concat())
    })
}

struct Inspector {
    buf: Vec<String>,
    ids: HashMap<String, i32>,
    depth: i32,
    level: i32,
    newline: String,
    indent: String,
}

impl Inspector {
    fn new(depth: i32) -> Self {
        Inspector {
            buf: Vec::new(),
            ids: HashMap::new(),
            depth,
            level: 0,
            newline: "\n".to_string(),
            indent: "  ".to_string(),
        }
    }

    fn puts(&mut self, s: String) {
        self.buf.push(s);
    }

    fn get_id(&mut self, _: &Value, addr: &str) -> i32 {
        if let Some(id) = self.ids.get(addr) {
            *id
        } else {
            let next_id = (self.ids.len() + 1) as i32;
            self.ids.insert(addr.to_string(), next_id);
            next_id
        }
    }

    fn tabify(&mut self) {
        self.puts(format!("{}{}", self.newline, self.indent.repeat(self.level as usize)));
    }

    fn is_identifier(s: &str) -> bool {
        let keywords = [
            "and", "break", "do", "else", "elseif", "end", "false", "for", "function", "goto", "if", "in", "local", "nil", "not", "or", "repeat", "return", "then", "true", "until", "while",
        ];

        if keywords.contains(&s) {
            return false;
        }

        if let Some(first_char) = s.chars().next() {
            if !first_char.is_ascii_alphabetic() && first_char != '_' {
                return false;
            }
        } else {
            return false;
        }

        s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
    }

    fn inspect_value(&mut self, lua: &mlua::Lua, v: &Value) -> Result<()> {
        match v {
            Value::String(s) => {
                let s_str = s.to_str()?;
                let quoted = if s_str.contains('\"') && !s_str.contains('\'') {
                    format!("'{}'", s_str)
                } else {
                    format!("\"{}\"", s_str.replace("\"", "\\\""))
                };
                self.puts(quoted);
            }
            Value::Integer(i) => self.puts(i.to_string()),
            Value::Number(n) => self.puts(n.to_string()),
            Value::Boolean(b) => self.puts(b.to_string()),
            Value::Nil => self.puts("nil".to_string()),
            Value::Table(t) => {
                let addr = format!("{:p}", t.to_pointer());

                if self.level >= self.depth {
                    self.puts("{...}".to_string());
                    return Ok(());
                }

                self.get_id(v, &addr);
                self.puts("{".to_string());
                self.level += 1;

                let mut first = true;
                let pairs: Function = lua.globals().get("pairs")?;
                let iter: Function = pairs.call((t,))?;
                let mut state = Value::Nil;

                loop {
                    let (key, value) = iter.call((t.clone(), state))?;
                    if key == Value::Nil {
                        break;
                    }

                    if !first {
                        self.puts(",".to_string());
                    }
                    first = false;

                    self.tabify();

                    match &key {
                        Value::String(s) => {
                            let key_str = s.to_str()?;
                            if Self::is_identifier(&key_str) {
                                self.puts(key_str.to_string());
                            } else {
                                self.puts("[".to_string());
                                self.inspect_value(lua, &key)?;
                                self.puts("]".to_string());
                            }
                        }
                        _ => {
                            self.puts("[".to_string());
                            self.inspect_value(lua, &key)?;
                            self.puts("]".to_string());
                        }
                    }

                    self.puts(" = ".to_string());
                    self.inspect_value(lua, &value)?;

                    state = key;
                }

                if let Some(mt) = t.metatable() {
                    if !first {
                        self.puts(",".to_string());
                    }
                    self.tabify();
                    self.puts("<metatable> = ".to_string());
                    self.inspect_value(lua, &Value::Table(mt))?;
                }

                self.level -= 1;
                if !first {
                    self.tabify();
                }
                self.puts("}".to_string());
            }
            Value::Function(f) => self.puts(format!("<function {:?}>", f.to_pointer())),
            Value::Thread(t) => self.puts(format!("<thread {:?}>", t.to_pointer())),
            Value::UserData(u) => self.puts(format!("<userdata {:?}>", u.to_pointer())),
            Value::LightUserData(p) => self.puts(format!("<lightuserdata {:?}>", p)),
            Value::Error(e) => self.puts(format!("<error {}>", e)),
            _ => self.puts(format!("<{}>", v.type_name())),
        }
        Ok(())
    }
}
