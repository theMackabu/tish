use std::collections::HashMap;

pub struct Template<'c> {
    start: String,
    end: String,
    template: String,
    context: HashMap<&'c str, String>,
}

impl<'c> Template<'c> {
    pub fn new(template: &str) -> Self {
        Self {
            context: HashMap::new(),
            start: "{t.".to_string(),
            end: "}".to_string(),
            template: template.to_string(),
        }
    }

    pub fn insert(&mut self, key: &'c str, value: String) {
        self.context.insert(key, value);
    }

    pub fn render(&self) -> String {
        let mut result = self.template.clone();
        for (key, value) in &self.context {
            let placeholder = format!("{}{}{}", self.start, key, self.end);
            result = result.replace(&placeholder, value);
        }
        result
    }
}
