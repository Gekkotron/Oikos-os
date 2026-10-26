//! The template grammar: literal text, `{{`/`}}` escapes and `{expr[:N]}` placeholders.
//!
//! Inside braces, tokens are split on whitespace and `(`/`)` stand alone; a token that is exactly
//! `+`, `-`, `*` or `/` is an operator, `-?\d+(\.\d+)?` a number, `trigger` the trigger value and
//! anything else an entity id (live ids hold `-`, `,` and accents, hence the spaced operators).

use super::{BinOp, Expr, Part, Template, TemplateError};

/// The most decimals `:N` may ask for.
const MAX_DECIMALS: u32 = 6;

/// Parses a notify title or body.
pub fn parse(text: &str) -> Result<Template, TemplateError> {
    let mut parts = Vec::new();
    let mut literal = String::new();
    let mut chars = text.char_indices().peekable();
    while let Some((at, c)) = chars.next() {
        match c {
            '{' if chars.peek().is_some_and(|&(_, next)| next == '{') => {
                chars.next();
                literal.push('{');
            }
            '}' if chars.peek().is_some_and(|&(_, next)| next == '}') => {
                chars.next();
                literal.push('}');
            }
            '}' => return Err(TemplateError::new("unexpected }")),
            '{' => {
                let start = at + 1;
                let end = text[start..]
                    .find(['{', '}'])
                    .map(|i| start + i)
                    .filter(|&i| text[i..].starts_with('}'))
                    .ok_or_else(|| TemplateError::new("unclosed {"))?;
                if !literal.is_empty() {
                    parts.push(Part::Text(std::mem::take(&mut literal)));
                }
                parts.push(placeholder(&text[start..end])?);
                while chars.next_if(|&(i, _)| i <= end).is_some() {}
            }
            c => literal.push(c),
        }
    }
    if !literal.is_empty() {
        parts.push(Part::Text(literal));
    }
    Ok(Template { parts })
}

/// One `{…}`: an expression with an optional trailing `:N`.
fn placeholder(inner: &str) -> Result<Part, TemplateError> {
    let mut inner = inner.trim();
    let mut decimals = None;
    if let Some(colon) = inner.rfind(':') {
        let digits = &inner[colon + 1..];
        if !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()) {
            match digits.parse::<u32>() {
                Ok(n) if n <= MAX_DECIMALS => decimals = Some(n as u8),
                _ => return Err(TemplateError::new("decimals must be 0 to 6")),
            }
            inner = inner[..colon].trim_end();
        }
    }
    let tokens = tokenize(inner);
    if tokens.is_empty() {
        return Err(TemplateError::new("empty {}"));
    }
    let mut parser = Parser { tokens, at: 0 };
    let expr = parser.expr()?;
    if let Some(token) = parser.peek() {
        return Err(match token {
            ")" => TemplateError::new("unbalanced parentheses"),
            _ => parser.missing_operator(token),
        });
    }
    Ok(Part::Value { expr, decimals })
}

fn tokenize(inner: &str) -> Vec<&str> {
    let mut tokens = Vec::new();
    for word in inner.split_whitespace() {
        let mut rest = word;
        while let Some(i) = rest.find(['(', ')']) {
            if i > 0 {
                tokens.push(&rest[..i]);
            }
            tokens.push(&rest[i..i + 1]);
            rest = &rest[i + 1..];
        }
        if !rest.is_empty() {
            tokens.push(rest);
        }
    }
    tokens
}

fn operator(token: &str) -> Option<BinOp> {
    match token {
        "+" => Some(BinOp::Add),
        "-" => Some(BinOp::Sub),
        "*" => Some(BinOp::Mul),
        "/" => Some(BinOp::Div),
        _ => None,
    }
}

/// `-?\d+(\.\d+)?`
fn number(token: &str) -> Option<f64> {
    let digits = token.strip_prefix('-').unwrap_or(token);
    let (whole, fraction) = match digits.split_once('.') {
        Some((whole, fraction)) => (whole, Some(fraction)),
        None => (digits, None),
    };
    let all_digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    (all_digits(whole) && fraction.is_none_or(all_digits)).then(|| token.parse().ok())?
}

struct Parser<'a> {
    tokens: Vec<&'a str>,
    at: usize,
}

impl<'a> Parser<'a> {
    fn peek(&self) -> Option<&'a str> {
        self.tokens.get(self.at).copied()
    }

    fn missing_operator(&self, token: &str) -> TemplateError {
        let before = self.tokens[self.at - 1];
        TemplateError::new(format!("missing operator between `{before}` and `{token}`"))
    }

    /// `term (('+'|'-') term)*`
    fn expr(&mut self) -> Result<Expr, TemplateError> {
        let mut left = self.term()?;
        while let Some(op) = self
            .peek()
            .and_then(operator)
            .filter(|op| matches!(op, BinOp::Add | BinOp::Sub))
        {
            self.at += 1;
            left = Expr::Bin(Box::new(left), op, Box::new(self.term()?));
        }
        Ok(left)
    }

    /// `atom (('*'|'/') atom)*`
    fn term(&mut self) -> Result<Expr, TemplateError> {
        let mut left = self.atom()?;
        while let Some(op) = self
            .peek()
            .and_then(operator)
            .filter(|op| matches!(op, BinOp::Mul | BinOp::Div))
        {
            self.at += 1;
            left = Expr::Bin(Box::new(left), op, Box::new(self.atom()?));
        }
        Ok(left)
    }

    /// `number | trigger | id | '(' expr ')'`
    fn atom(&mut self) -> Result<Expr, TemplateError> {
        let Some(token) = self.peek() else {
            let before = self.tokens[self.at - 1];
            return Err(TemplateError::new(format!(
                "missing value after `{before}`"
            )));
        };
        if operator(token).is_some() || token == ")" {
            return Err(match self.at {
                0 => TemplateError::new(format!("missing value before `{token}`")),
                _ => {
                    let before = self.tokens[self.at - 1];
                    TemplateError::new(format!("missing value after `{before}`"))
                }
            });
        }
        self.at += 1;
        if token == "(" {
            let inner = self.expr()?;
            return match self.peek() {
                Some(")") => {
                    self.at += 1;
                    Ok(inner)
                }
                None => Err(TemplateError::new("unbalanced parentheses")),
                Some(other) => Err(self.missing_operator(other)),
            };
        }
        Ok(match number(token) {
            Some(n) => Expr::Number(n),
            None if token == "trigger" => Expr::Trigger,
            None => Expr::Entity(token.to_owned()),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_text_has_no_entities() {
        let t = parse("Door open").unwrap();
        assert!(t.is_plain());
        assert!(t.entities().is_empty());
    }

    #[test]
    fn ids_with_dashes_commas_and_accents_are_one_entity() {
        assert_eq!(
            parse("{double_take_rémi}").unwrap().entities(),
            vec!["double_take_rémi"]
        );
        assert_eq!(
            parse("{0x00_c,28-0a-18_5_zigbee2mqtt}").unwrap().entities(),
            vec!["0x00_c,28-0a-18_5_zigbee2mqtt"]
        );
        assert_eq!(parse("{a-b}").unwrap().entities(), vec!["a-b"]);
        assert_eq!(parse("{a - b}").unwrap().entities(), vec!["a", "b"]);
    }

    #[test]
    fn trigger_numbers_and_decimals() {
        let t = parse("{trigger} {(weather_today_max - weather_today_min):1} {x * -2}").unwrap();
        assert_eq!(
            t.entities(),
            vec!["weather_today_max", "weather_today_min", "x"]
        );
        assert!(parse("{x:6}").is_ok());
        assert!(
            parse("{x:7}")
                .unwrap_err()
                .reason
                .contains("decimals must be 0 to 6")
        );
    }

    #[test]
    fn escaped_braces_are_text() {
        assert!(parse("{{ok}}").unwrap().is_plain());
    }

    #[test]
    fn bad_templates_say_why() {
        for (text, why) in [
            ("Temp {x", "unclosed {"),
            ("a } b", "unexpected }"),
            ("{ }", "empty {}"),
            ("{a b}", "missing operator between `a` and `b`"),
            ("{a -b}", "missing operator between `a` and `-b`"),
            ("{a -}", "missing value after `-`"),
            ("{(a + b}", "unbalanced parentheses"),
            ("{a + b)}", "unbalanced parentheses"),
        ] {
            assert!(parse(text).unwrap_err().reason.contains(why), "{text}");
        }
    }
}
