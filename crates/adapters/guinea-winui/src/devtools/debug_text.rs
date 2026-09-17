//! Reads back what `{:#?}` printed.
//!
//! Not a Rust parser: enough of the derived `Debug` shape to walk a value -
//! structs, tuple structs, lists and atoms.

#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Struct {
        name: String,
        fields: Vec<(String, Value)>,
    },
    Tuple {
        name: String,
        items: Vec<Value>,
    },
    List(Vec<Value>),
    Atom(String),
}

impl Value {
    pub fn name(&self) -> Option<&str> {
        match self {
            Value::Struct { name, .. } | Value::Tuple { name, .. } => Some(name),
            _ => None,
        }
    }

    pub fn field(&self, wanted: &str) -> Option<&Value> {
        match self {
            Value::Struct { fields, .. } => fields
                .iter()
                .find(|(name, _)| name == wanted)
                .map(|(_, value)| value),
            _ => None,
        }
    }

    /// The only item of a one-item tuple struct.
    pub fn single(&self) -> Option<&Value> {
        match self {
            Value::Tuple { items, .. } if items.len() == 1 => items.first(),
            _ => None,
        }
    }

    /// On one line, for a property.
    pub fn compact(&self) -> String {
        match self {
            Value::Atom(atom) => atom.clone(),
            Value::List(items) => format!(
                "[{}]",
                items.iter().map(Value::compact).collect::<Vec<_>>().join(", ")
            ),
            Value::Tuple { name, items } => format!(
                "{name}({})",
                items.iter().map(Value::compact).collect::<Vec<_>>().join(", ")
            ),
            Value::Struct { name, fields } => format!(
                "{name} {{ {} }}",
                fields
                    .iter()
                    .map(|(key, value)| format!("{key}: {}", value.compact()))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
enum Token {
    Open(char),
    Close(char),
    Colon,
    Comma,
    Word(String),
}

fn tokens(text: &str) -> Vec<Token> {
    let mut out = Vec::new();
    let mut chars = text.chars().peekable();
    while let Some(&c) = chars.peek() {
        match c {
            c if c.is_whitespace() => {
                chars.next();
            }
            '{' | '(' | '[' => {
                chars.next();
                out.push(Token::Open(c));
            }
            '}' | ')' | ']' => {
                chars.next();
                out.push(Token::Close(c));
            }
            ':' => {
                chars.next();
                if chars.peek() == Some(&':') {
                    chars.next();
                    push_word(&mut out, "::");
                } else {
                    out.push(Token::Colon);
                }
            }
            ',' => {
                chars.next();
                out.push(Token::Comma);
            }
            '"' | '\'' => {
                let quote = c;
                let mut word = String::new();
                word.push(quote);
                chars.next();
                while let Some(c) = chars.next() {
                    word.push(c);
                    if c == '\\' {
                        if let Some(escaped) = chars.next() {
                            word.push(escaped);
                        }
                    } else if c == quote {
                        break;
                    }
                }
                out.push(Token::Word(word));
            }
            _ => {
                let mut word = String::new();
                while let Some(&c) = chars.peek() {
                    if c.is_whitespace() || "{}()[]:,".contains(c) {
                        break;
                    }
                    word.push(c);
                    chars.next();
                }
                push_word(&mut out, &word);
            }
        }
    }
    out
}

/// `a::b` arrives as three pieces; glue them back.
fn push_word(out: &mut Vec<Token>, word: &str) {
    if let Some(Token::Word(last)) = out.last_mut()
        && (last.ends_with("::") || word == "::")
    {
        last.push_str(word);
        return;
    }
    out.push(Token::Word(word.to_string()));
}

struct Parser {
    tokens: Vec<Token>,
    at: usize,
}

impl Parser {
    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.at)
    }

    fn next(&mut self) -> Option<Token> {
        let token = self.tokens.get(self.at).cloned();
        self.at += 1;
        token
    }

    fn value(&mut self) -> Option<Value> {
        match self.next()? {
            Token::Word(word) => match self.peek() {
                Some(Token::Open('{')) => {
                    self.next();
                    Some(Value::Struct {
                        name: word,
                        fields: self.fields(),
                    })
                }
                Some(Token::Open('(')) => {
                    self.next();
                    Some(Value::Tuple {
                        name: word,
                        items: self.items(')'),
                    })
                }
                _ => Some(Value::Atom(word)),
            },
            Token::Open('[') => Some(Value::List(self.items(']'))),
            Token::Open('(') => Some(Value::Tuple {
                name: String::new(),
                items: self.items(')'),
            }),
            Token::Open('{') => Some(Value::Struct {
                name: String::new(),
                fields: self.fields(),
            }),
            _ => None,
        }
    }

    fn items(&mut self, close: char) -> Vec<Value> {
        let mut items = Vec::new();
        loop {
            match self.peek() {
                None => return items,
                Some(Token::Close(c)) if *c == close => {
                    self.next();
                    return items;
                }
                Some(Token::Comma) => {
                    self.next();
                }
                _ => match self.value() {
                    Some(value) => items.push(value),
                    None => return items,
                },
            }
        }
    }

    fn fields(&mut self) -> Vec<(String, Value)> {
        let mut fields = Vec::new();
        loop {
            match self.next() {
                None | Some(Token::Close('}')) => return fields,
                Some(Token::Comma) => {}
                Some(Token::Word(name)) => {
                    if name == ".." {
                        continue;
                    }
                    if self.peek() == Some(&Token::Colon) {
                        self.next();
                    }
                    match self.value() {
                        Some(value) => fields.push((name, value)),
                        None => return fields,
                    }
                }
                Some(_) => return fields,
            }
        }
    }
}

pub fn parse(text: &str) -> Option<Value> {
    Parser {
        tokens: tokens(text),
        at: 0,
    }
    .value()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_nested_dump_reads_back() {
        let value = parse(
            r#"View(
                Children {
                    control: StackPanel(StackPanel { spacing: Set(12.0), children: [] }),
                    children: [
                        KeyedView { key: Key(String("a, b")), view: View(Component("app::pages::Processes")) },
                    ],
                },
            )"#,
        )
        .unwrap();

        let children = value.single().unwrap();
        assert_eq!(children.name(), Some("Children"));
        let control = children.field("control").unwrap().single().unwrap();
        assert_eq!(control.field("spacing").unwrap().compact(), "Set(12.0)");

        let Some(Value::List(items)) = children.field("children") else {
            panic!("children is a list");
        };
        let keyed = &items[0];
        assert_eq!(keyed.field("key").unwrap().compact(), r#"Key(String("a, b"))"#);
        let component = keyed.field("view").unwrap().single().unwrap();
        assert_eq!(
            component.single().unwrap(),
            &Value::Atom(r#""app::pages::Processes""#.to_string())
        );
    }

    #[test]
    fn a_path_stays_one_word_and_an_elision_is_skipped() {
        let value = parse("Thing { kind: a::b::C, .. }").unwrap();
        assert_eq!(value.field("kind"), Some(&Value::Atom("a::b::C".to_string())));
    }
}
