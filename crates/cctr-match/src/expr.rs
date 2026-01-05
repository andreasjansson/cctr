//! Expression parser and evaluator for constraint checking.

use std::collections::HashMap;
use thiserror::Error;

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Number(f64),
    String(String),
    Bool(bool),
    Array(Vec<Value>),
}

impl Value {
    pub fn as_bool(&self) -> Result<bool, EvalError> {
        match self {
            Value::Bool(b) => Ok(*b),
            _ => Err(EvalError::TypeError {
                expected: "bool",
                got: self.type_name(),
            }),
        }
    }

    pub fn as_number(&self) -> Result<f64, EvalError> {
        match self {
            Value::Number(n) => Ok(*n),
            _ => Err(EvalError::TypeError {
                expected: "number",
                got: self.type_name(),
            }),
        }
    }

    pub fn as_string(&self) -> Result<&str, EvalError> {
        match self {
            Value::String(s) => Ok(s),
            _ => Err(EvalError::TypeError {
                expected: "string",
                got: self.type_name(),
            }),
        }
    }

    pub fn as_array(&self) -> Result<&[Value], EvalError> {
        match self {
            Value::Array(a) => Ok(a),
            _ => Err(EvalError::TypeError {
                expected: "array",
                got: self.type_name(),
            }),
        }
    }

    fn type_name(&self) -> &'static str {
        match self {
            Value::Number(_) => "number",
            Value::String(_) => "string",
            Value::Bool(_) => "bool",
            Value::Array(_) => "array",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Number(f64),
    String(String),
    Bool(bool),
    Var(String),
    Array(Vec<Expr>),
    UnaryOp {
        op: UnaryOp,
        expr: Box<Expr>,
    },
    BinaryOp {
        op: BinaryOp,
        left: Box<Expr>,
        right: Box<Expr>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum UnaryOp {
    Not,
    Neg,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    Pow,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    And,
    Or,
    In,
    Contains,
    StartsWith,
    EndsWith,
    Matches,
}

#[derive(Error, Debug, Clone, PartialEq)]
pub enum EvalError {
    #[error("type error: expected {expected}, got {got}")]
    TypeError {
        expected: &'static str,
        got: &'static str,
    },
    #[error("undefined variable: {0}")]
    UndefinedVariable(String),
    #[error("invalid regex: {0}")]
    InvalidRegex(String),
    #[error("division by zero")]
    DivisionByZero,
    #[error("parse error: {0}")]
    ParseError(String),
}

// ============ Tokenizer ============

#[derive(Debug, Clone, PartialEq)]
enum Token {
    Number(f64),
    String(String),
    Regex(String),
    Ident(String),
    // Operators
    Plus,
    Minus,
    Star,
    Slash,
    Caret,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    // Keywords
    And,
    Or,
    Not,
    In,
    Contains,
    StartsWith,
    EndsWith,
    Matches,
    True,
    False,
    // Punctuation
    LParen,
    RParen,
    LBracket,
    RBracket,
    Comma,
}

struct Lexer<'a> {
    input: &'a str,
    pos: usize,
}

impl<'a> Lexer<'a> {
    fn new(input: &'a str) -> Self {
        Self { input, pos: 0 }
    }

    fn peek_char(&self) -> Option<char> {
        self.input[self.pos..].chars().next()
    }

    fn advance(&mut self) -> Option<char> {
        let c = self.peek_char()?;
        self.pos += c.len_utf8();
        Some(c)
    }

    fn skip_whitespace(&mut self) {
        while let Some(c) = self.peek_char() {
            if c.is_whitespace() {
                self.advance();
            } else {
                break;
            }
        }
    }

    fn read_number(&mut self) -> Token {
        let start = self.pos;
        if self.peek_char() == Some('-') {
            self.advance();
        }
        while let Some(c) = self.peek_char() {
            if c.is_ascii_digit() {
                self.advance();
            } else {
                break;
            }
        }
        if self.peek_char() == Some('.') {
            self.advance();
            while let Some(c) = self.peek_char() {
                if c.is_ascii_digit() {
                    self.advance();
                } else {
                    break;
                }
            }
        }
        let s = &self.input[start..self.pos];
        Token::Number(s.parse().unwrap())
    }

    fn read_string(&mut self) -> Result<Token, EvalError> {
        self.advance(); // consume opening quote
        let mut s = String::new();
        loop {
            match self.advance() {
                None => return Err(EvalError::ParseError("unterminated string".to_string())),
                Some('"') => break,
                Some('\\') => match self.advance() {
                    Some('n') => s.push('\n'),
                    Some('t') => s.push('\t'),
                    Some('r') => s.push('\r'),
                    Some('"') => s.push('"'),
                    Some('\\') => s.push('\\'),
                    Some(c) => {
                        s.push('\\');
                        s.push(c);
                    }
                    None => return Err(EvalError::ParseError("unterminated escape".to_string())),
                },
                Some(c) => s.push(c),
            }
        }
        Ok(Token::String(s))
    }

    fn read_regex(&mut self) -> Result<Token, EvalError> {
        self.advance(); // consume opening /
        let mut s = String::new();
        loop {
            match self.advance() {
                None => return Err(EvalError::ParseError("unterminated regex".to_string())),
                Some('/') => break,
                Some('\\') => {
                    s.push('\\');
                    if let Some(c) = self.advance() {
                        s.push(c);
                    }
                }
                Some(c) => s.push(c),
            }
        }
        Ok(Token::Regex(s))
    }

    fn read_ident(&mut self) -> Token {
        let start = self.pos;
        while let Some(c) = self.peek_char() {
            if c.is_ascii_alphanumeric() || c == '_' {
                self.advance();
            } else {
                break;
            }
        }
        let s = &self.input[start..self.pos];
        match s {
            "and" => Token::And,
            "or" => Token::Or,
            "not" => Token::Not,
            "in" => Token::In,
            "contains" => Token::Contains,
            "startswith" => Token::StartsWith,
            "endswith" => Token::EndsWith,
            "matches" => Token::Matches,
            "true" => Token::True,
            "false" => Token::False,
            _ => Token::Ident(s.to_string()),
        }
    }

    fn next_token(&mut self) -> Result<Option<Token>, EvalError> {
        self.skip_whitespace();
        let c = match self.peek_char() {
            None => return Ok(None),
            Some(c) => c,
        };

        let token = match c {
            '0'..='9' => self.read_number(),
            '-' => {
                let next_pos = self.pos + 1;
                if next_pos < self.input.len() {
                    let next = self.input[next_pos..].chars().next();
                    if next.map(|c| c.is_ascii_digit()).unwrap_or(false) {
                        self.read_number()
                    } else {
                        self.advance();
                        Token::Minus
                    }
                } else {
                    self.advance();
                    Token::Minus
                }
            }
            '"' => self.read_string()?,
            '/' => self.read_regex()?,
            'a'..='z' | 'A'..='Z' | '_' => self.read_ident(),
            '+' => {
                self.advance();
                Token::Plus
            }
            '*' => {
                self.advance();
                Token::Star
            }
            '^' => {
                self.advance();
                Token::Caret
            }
            '(' => {
                self.advance();
                Token::LParen
            }
            ')' => {
                self.advance();
                Token::RParen
            }
            '[' => {
                self.advance();
                Token::LBracket
            }
            ']' => {
                self.advance();
                Token::RBracket
            }
            ',' => {
                self.advance();
                Token::Comma
            }
            '=' => {
                self.advance();
                if self.peek_char() == Some('=') {
                    self.advance();
                    Token::Eq
                } else {
                    return Err(EvalError::ParseError("expected '==' ".to_string()));
                }
            }
            '!' => {
                self.advance();
                if self.peek_char() == Some('=') {
                    self.advance();
                    Token::Ne
                } else {
                    return Err(EvalError::ParseError("expected '!='".to_string()));
                }
            }
            '<' => {
                self.advance();
                if self.peek_char() == Some('=') {
                    self.advance();
                    Token::Le
                } else {
                    Token::Lt
                }
            }
            '>' => {
                self.advance();
                if self.peek_char() == Some('=') {
                    self.advance();
                    Token::Ge
                } else {
                    Token::Gt
                }
            }
            _ => {
                return Err(EvalError::ParseError(format!(
                    "unexpected character: {}",
                    c
                )))
            }
        };
        Ok(Some(token))
    }

    fn tokenize(&mut self) -> Result<Vec<Token>, EvalError> {
        let mut tokens = Vec::new();
        while let Some(tok) = self.next_token()? {
            tokens.push(tok);
        }
        Ok(tokens)
    }
}

// ============ Parser ============

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}

impl Parser {
    fn new(tokens: Vec<Token>) -> Self {
        Self { tokens, pos: 0 }
    }

    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.pos)
    }

    fn advance(&mut self) -> Option<&Token> {
        let tok = self.tokens.get(self.pos);
        if tok.is_some() {
            self.pos += 1;
        }
        tok
    }

    fn expect(&mut self, expected: &Token) -> Result<(), EvalError> {
        match self.advance() {
            Some(tok) if tok == expected => Ok(()),
            Some(tok) => Err(EvalError::ParseError(format!(
                "expected {:?}, got {:?}",
                expected, tok
            ))),
            None => Err(EvalError::ParseError(format!(
                "expected {:?}, got EOF",
                expected
            ))),
        }
    }

    fn parse_expr(&mut self) -> Result<Expr, EvalError> {
        self.parse_or()
    }

    fn parse_or(&mut self) -> Result<Expr, EvalError> {
        let mut left = self.parse_and()?;
        while self.peek() == Some(&Token::Or) {
            self.advance();
            let right = self.parse_and()?;
            left = Expr::BinaryOp {
                op: BinaryOp::Or,
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    fn parse_and(&mut self) -> Result<Expr, EvalError> {
        let mut left = self.parse_not()?;
        while self.peek() == Some(&Token::And) {
            self.advance();
            let right = self.parse_not()?;
            left = Expr::BinaryOp {
                op: BinaryOp::And,
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    fn parse_not(&mut self) -> Result<Expr, EvalError> {
        if self.peek() == Some(&Token::Not) {
            self.advance();
            let expr = self.parse_not()?;
            Ok(Expr::UnaryOp {
                op: UnaryOp::Not,
                expr: Box::new(expr),
            })
        } else {
            self.parse_comparison()
        }
    }

    fn parse_comparison(&mut self) -> Result<Expr, EvalError> {
        let left = self.parse_additive()?;
        let op = match self.peek() {
            Some(Token::Eq) => BinaryOp::Eq,
            Some(Token::Ne) => BinaryOp::Ne,
            Some(Token::Lt) => BinaryOp::Lt,
            Some(Token::Le) => BinaryOp::Le,
            Some(Token::Gt) => BinaryOp::Gt,
            Some(Token::Ge) => BinaryOp::Ge,
            Some(Token::In) => BinaryOp::In,
            Some(Token::Contains) => BinaryOp::Contains,
            Some(Token::StartsWith) => BinaryOp::StartsWith,
            Some(Token::EndsWith) => BinaryOp::EndsWith,
            Some(Token::Matches) => BinaryOp::Matches,
            _ => return Ok(left),
        };
        self.advance();
        let right = self.parse_additive()?;
        Ok(Expr::BinaryOp {
            op,
            left: Box::new(left),
            right: Box::new(right),
        })
    }

    fn parse_additive(&mut self) -> Result<Expr, EvalError> {
        let mut left = self.parse_multiplicative()?;
        loop {
            let op = match self.peek() {
                Some(Token::Plus) => BinaryOp::Add,
                Some(Token::Minus) => BinaryOp::Sub,
                _ => break,
            };
            self.advance();
            let right = self.parse_multiplicative()?;
            left = Expr::BinaryOp {
                op,
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    fn parse_multiplicative(&mut self) -> Result<Expr, EvalError> {
        let mut left = self.parse_power()?;
        loop {
            let op = match self.peek() {
                Some(Token::Star) => BinaryOp::Mul,
                Some(Token::Slash) => BinaryOp::Div,
                _ => break,
            };
            self.advance();
            let right = self.parse_power()?;
            left = Expr::BinaryOp {
                op,
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    fn parse_power(&mut self) -> Result<Expr, EvalError> {
        let base = self.parse_unary()?;
        if self.peek() == Some(&Token::Caret) {
            self.advance();
            let exp = self.parse_power()?; // right associative
            Ok(Expr::BinaryOp {
                op: BinaryOp::Pow,
                left: Box::new(base),
                right: Box::new(exp),
            })
        } else {
            Ok(base)
        }
    }

    fn parse_unary(&mut self) -> Result<Expr, EvalError> {
        if self.peek() == Some(&Token::Minus) {
            self.advance();
            let expr = self.parse_unary()?;
            Ok(Expr::UnaryOp {
                op: UnaryOp::Neg,
                expr: Box::new(expr),
            })
        } else {
            self.parse_atom()
        }
    }

    fn parse_atom(&mut self) -> Result<Expr, EvalError> {
        match self.advance() {
            Some(Token::Number(n)) => Ok(Expr::Number(*n)),
            Some(Token::String(s)) => Ok(Expr::String(s.clone())),
            Some(Token::Regex(r)) => Ok(Expr::String(r.clone())),
            Some(Token::Ident(name)) => Ok(Expr::Var(name.clone())),
            Some(Token::True) => Ok(Expr::Bool(true)),
            Some(Token::False) => Ok(Expr::Bool(false)),
            Some(Token::LParen) => {
                let expr = self.parse_expr()?;
                self.expect(&Token::RParen)?;
                Ok(expr)
            }
            Some(Token::LBracket) => {
                let mut elements = Vec::new();
                if self.peek() != Some(&Token::RBracket) {
                    elements.push(self.parse_expr()?);
                    while self.peek() == Some(&Token::Comma) {
                        self.advance();
                        if self.peek() == Some(&Token::RBracket) {
                            break; // trailing comma
                        }
                        elements.push(self.parse_expr()?);
                    }
                }
                self.expect(&Token::RBracket)?;
                Ok(Expr::Array(elements))
            }
            Some(tok) => Err(EvalError::ParseError(format!(
                "unexpected token: {:?}",
                tok
            ))),
            None => Err(EvalError::ParseError("unexpected end of input".to_string())),
        }
    }
}

// ============ Evaluator ============

pub fn evaluate(expr: &Expr, vars: &HashMap<String, Value>) -> Result<Value, EvalError> {
    match expr {
        Expr::Number(n) => Ok(Value::Number(*n)),
        Expr::String(s) => Ok(Value::String(s.clone())),
        Expr::Bool(b) => Ok(Value::Bool(*b)),
        Expr::Var(name) => vars
            .get(name)
            .cloned()
            .ok_or_else(|| EvalError::UndefinedVariable(name.clone())),
        Expr::Array(elements) => {
            let values: Result<Vec<_>, _> = elements.iter().map(|e| evaluate(e, vars)).collect();
            Ok(Value::Array(values?))
        }
        Expr::UnaryOp { op, expr } => {
            let val = evaluate(expr, vars)?;
            match op {
                UnaryOp::Not => Ok(Value::Bool(!val.as_bool()?)),
                UnaryOp::Neg => Ok(Value::Number(-val.as_number()?)),
            }
        }
        Expr::BinaryOp { op, left, right } => eval_binary_op(*op, left, right, vars),
    }
}

fn eval_binary_op(
    op: BinaryOp,
    left: &Expr,
    right: &Expr,
    vars: &HashMap<String, Value>,
) -> Result<Value, EvalError> {
    // Short-circuit evaluation for and/or
    if op == BinaryOp::And {
        let l = evaluate(left, vars)?.as_bool()?;
        if !l {
            return Ok(Value::Bool(false));
        }
        return Ok(Value::Bool(evaluate(right, vars)?.as_bool()?));
    }
    if op == BinaryOp::Or {
        let l = evaluate(left, vars)?.as_bool()?;
        if l {
            return Ok(Value::Bool(true));
        }
        return Ok(Value::Bool(evaluate(right, vars)?.as_bool()?));
    }

    let l = evaluate(left, vars)?;
    let r = evaluate(right, vars)?;

    match op {
        BinaryOp::Add => Ok(Value::Number(l.as_number()? + r.as_number()?)),
        BinaryOp::Sub => Ok(Value::Number(l.as_number()? - r.as_number()?)),
        BinaryOp::Mul => Ok(Value::Number(l.as_number()? * r.as_number()?)),
        BinaryOp::Div => {
            let divisor = r.as_number()?;
            if divisor == 0.0 {
                Err(EvalError::DivisionByZero)
            } else {
                Ok(Value::Number(l.as_number()? / divisor))
            }
        }
        BinaryOp::Pow => Ok(Value::Number(l.as_number()?.powf(r.as_number()?))),
        BinaryOp::Eq => Ok(Value::Bool(values_equal(&l, &r))),
        BinaryOp::Ne => Ok(Value::Bool(!values_equal(&l, &r))),
        BinaryOp::Lt => Ok(Value::Bool(l.as_number()? < r.as_number()?)),
        BinaryOp::Le => Ok(Value::Bool(l.as_number()? <= r.as_number()?)),
        BinaryOp::Gt => Ok(Value::Bool(l.as_number()? > r.as_number()?)),
        BinaryOp::Ge => Ok(Value::Bool(l.as_number()? >= r.as_number()?)),
        BinaryOp::In => {
            let arr = r.as_array()?;
            Ok(Value::Bool(arr.iter().any(|v| values_equal(&l, v))))
        }
        BinaryOp::Contains => {
            let haystack = l.as_string()?;
            let needle = r.as_string()?;
            Ok(Value::Bool(haystack.contains(needle)))
        }
        BinaryOp::StartsWith => {
            let s = l.as_string()?;
            let prefix = r.as_string()?;
            Ok(Value::Bool(s.starts_with(prefix)))
        }
        BinaryOp::EndsWith => {
            let s = l.as_string()?;
            let suffix = r.as_string()?;
            Ok(Value::Bool(s.ends_with(suffix)))
        }
        BinaryOp::Matches => {
            let s = l.as_string()?;
            let pattern = r.as_string()?;
            let re =
                regex::Regex::new(pattern).map_err(|e| EvalError::InvalidRegex(e.to_string()))?;
            Ok(Value::Bool(re.is_match(s)))
        }
        BinaryOp::And | BinaryOp::Or => unreachable!(),
    }
}

fn values_equal(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(a), Value::Number(b)) => (a - b).abs() < f64::EPSILON,
        (Value::String(a), Value::String(b)) => a == b,
        (Value::Bool(a), Value::Bool(b)) => a == b,
        (Value::Array(a), Value::Array(b)) => {
            a.len() == b.len() && a.iter().zip(b.iter()).all(|(x, y)| values_equal(x, y))
        }
        _ => false,
    }
}

// ============ Public API ============

pub fn parse(input: &str) -> Result<Expr, EvalError> {
    let mut lexer = Lexer::new(input);
    let tokens = lexer.tokenize()?;
    let mut parser = Parser::new(tokens);
    let expr = parser.parse_expr()?;
    if parser.peek().is_some() {
        return Err(EvalError::ParseError(format!(
            "unexpected trailing tokens: {:?}",
            &parser.tokens[parser.pos..]
        )));
    }
    Ok(expr)
}

pub fn eval_bool(expr_str: &str, vars: &HashMap<String, Value>) -> Result<bool, EvalError> {
    let ast = parse(expr_str)?;
    let result = evaluate(&ast, vars)?;
    result.as_bool()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vars(pairs: &[(&str, Value)]) -> HashMap<String, Value> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect()
    }

    #[test]
    fn test_number_parsing() {
        assert_eq!(parse("42").unwrap(), Expr::Number(42.0));
        assert_eq!(parse("-3.14").unwrap(), Expr::Number(-3.14));
        assert_eq!(parse("0.5").unwrap(), Expr::Number(0.5));
    }

    #[test]
    fn test_string_parsing() {
        assert_eq!(
            parse(r#""hello""#).unwrap(),
            Expr::String("hello".to_string())
        );
        assert_eq!(
            parse(r#""hello world""#).unwrap(),
            Expr::String("hello world".to_string())
        );
    }

    #[test]
    fn test_arithmetic() {
        let v = vars(&[]);
        assert!(eval_bool("1 + 2 == 3", &v).unwrap());
        assert!(eval_bool("10 - 3 == 7", &v).unwrap());
        assert!(eval_bool("4 * 5 == 20", &v).unwrap());
        assert!(eval_bool("10 / 2 == 5", &v).unwrap());
        assert!(eval_bool("2 ^ 3 == 8", &v).unwrap());
    }

    #[test]
    fn test_comparisons() {
        let v = vars(&[("n", Value::Number(42.0))]);
        assert!(eval_bool("n > 0", &v).unwrap());
        assert!(eval_bool("n < 100", &v).unwrap());
        assert!(eval_bool("n >= 42", &v).unwrap());
        assert!(eval_bool("n <= 42", &v).unwrap());
        assert!(eval_bool("n == 42", &v).unwrap());
        assert!(eval_bool("n != 0", &v).unwrap());
    }

    #[test]
    fn test_boolean_logic() {
        let v = vars(&[("n", Value::Number(42.0))]);
        assert!(eval_bool("n > 0 and n < 100", &v).unwrap());
        assert!(eval_bool("n < 0 or n > 0", &v).unwrap());
        assert!(eval_bool("not (n < 0)", &v).unwrap());
        assert!(eval_bool("not n < 0", &v).unwrap());
    }

    #[test]
    fn test_in_operator() {
        let v = vars(&[("n", Value::Number(2.0))]);
        assert!(eval_bool("n in [1, 2, 3]", &v).unwrap());
        assert!(!eval_bool("n in [4, 5, 6]", &v).unwrap());

        let v = vars(&[("s", Value::String("bar".to_string()))]);
        assert!(eval_bool(r#"s in ["foo", "bar", "baz"]"#, &v).unwrap());
    }

    #[test]
    fn test_string_operators() {
        let v = vars(&[("s", Value::String("hello world".to_string()))]);
        assert!(eval_bool(r#"s contains "world""#, &v).unwrap());
        assert!(eval_bool(r#"s startswith "hello""#, &v).unwrap());
        assert!(eval_bool(r#"s endswith "world""#, &v).unwrap());
        assert!(!eval_bool(r#"s contains "xyz""#, &v).unwrap());
    }

    #[test]
    fn test_regex_matches() {
        let v = vars(&[("s", Value::String("hello123".to_string()))]);
        assert!(eval_bool(r#"s matches /^hello\d+$/"#, &v).unwrap());
        assert!(!eval_bool(r#"s matches /^world/"#, &v).unwrap());
    }

    #[test]
    fn test_parentheses() {
        let v = vars(&[]);
        assert!(eval_bool("(1 + 2) * 3 == 9", &v).unwrap());
        assert!(eval_bool("1 + 2 * 3 == 7", &v).unwrap());
    }

    #[test]
    fn test_true_false() {
        let v = vars(&[]);
        assert!(eval_bool("true", &v).unwrap());
        assert!(!eval_bool("false", &v).unwrap());
        assert!(!eval_bool("true and false", &v).unwrap());
        assert!(eval_bool("true or false", &v).unwrap());
    }
}
