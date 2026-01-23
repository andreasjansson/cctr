//! Expression language parser and evaluator for cctr constraints.
//!
//! Supports:
//! - Numbers: `42`, `-3.14`, `0.5`
//! - Strings: `"hello"`, `"with \"escapes\""`
//! - Booleans: `true`, `false`
//! - Arrays: `[1, 2, 3]`, `["a", "b"]`
//! - Objects: `{"key": value, ...}`
//! - Arithmetic: `+`, `-`, `*`, `/`, `^`
//! - Comparison: `==`, `!=`, `<`, `<=`, `>`, `>=`
//! - Logical: `and`, `or`, `not`
//! - String ops: `contains`, `startswith`, `endswith`, `matches`
//! - Membership: `in`
//! - Array/object access: `a[0]`, `obj["key"]`, `obj.key`
//! - Functions: `len(s)`, `type(v)`, `keys(obj)`
//! - Quantifiers: `expr forall x in arr`
//!
//! # Example
//!
//! ```
//! use cctr_expr::{eval_bool, Value};
//! use std::collections::HashMap;
//!
//! let mut vars = HashMap::new();
//! vars.insert("n".to_string(), Value::Number(42.0));
//!
//! assert!(eval_bool("n > 0 and n < 100", &vars).unwrap());
//! ```

use std::collections::HashMap;
use thiserror::Error;
use winnow::ascii::{digit1, multispace0};
use winnow::combinator::{alt, delimited, opt, preceded, repeat, separated, terminated};
use winnow::error::ContextError;
use winnow::prelude::*;
use winnow::token::{any, none_of, one_of, take_while};

// ============ Value Types ============

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Number(f64),
    String(String),
    Bool(bool),
    Array(Vec<Value>),
    Object(HashMap<String, Value>),
    Type(String),
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

    pub fn as_object(&self) -> Result<&HashMap<String, Value>, EvalError> {
        match self {
            Value::Object(o) => Ok(o),
            _ => Err(EvalError::TypeError {
                expected: "object",
                got: self.type_name(),
            }),
        }
    }

    pub fn type_name(&self) -> &'static str {
        match self {
            Value::Number(_) => "number",
            Value::String(_) => "string",
            Value::Bool(_) => "bool",
            Value::Array(_) => "json_array",
            Value::Object(_) => "json_object",
            Value::Type(_) => "type",
        }
    }
}

// ============ AST Types ============

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Number(f64),
    String(String),
    Bool(bool),
    Var(String),
    Array(Vec<Expr>),
    Object(Vec<(String, Expr)>),
    TypeLiteral(String),
    UnaryOp {
        op: UnaryOp,
        expr: Box<Expr>,
    },
    BinaryOp {
        op: BinaryOp,
        left: Box<Expr>,
        right: Box<Expr>,
    },
    FuncCall {
        name: String,
        args: Vec<Expr>,
    },
    Index {
        expr: Box<Expr>,
        index: Box<Expr>,
    },
    Property {
        expr: Box<Expr>,
        name: String,
    },
    ForAll {
        predicate: Box<Expr>,
        var: String,
        iterable: Box<Expr>,
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
    #[error("undefined function: {0}")]
    UndefinedFunction(String),
    #[error("invalid regex: {0}")]
    InvalidRegex(String),
    #[error("division by zero")]
    DivisionByZero,
    #[error("parse error: {0}")]
    ParseError(String),
    #[error("wrong number of arguments for {func}: expected {expected}, got {got}")]
    WrongArgCount {
        func: String,
        expected: usize,
        got: usize,
    },
    #[error("index out of bounds: {index} >= {len}")]
    IndexOutOfBounds { index: usize, len: usize },
    #[error("key not found: {0}")]
    KeyNotFound(String),
}

// ============ Parser ============

fn ws<'a, P, O>(p: P) -> impl Parser<&'a str, O, ContextError>
where
    P: Parser<&'a str, O, ContextError>,
{
    delimited(multispace0, p, multispace0)
}

fn number(input: &mut &str) -> ModalResult<Expr> {
    let neg: Option<char> = opt('-').parse_next(input)?;
    let int_part: &str = digit1.parse_next(input)?;
    let frac_part: Option<&str> = opt(preceded('.', digit1)).parse_next(input)?;

    let mut s = String::new();
    if neg.is_some() {
        s.push('-');
    }
    s.push_str(int_part);
    if let Some(frac) = frac_part {
        s.push('.');
        s.push_str(frac);
    }

    Ok(Expr::Number(s.parse().unwrap()))
}

fn string_char(input: &mut &str) -> ModalResult<char> {
    let c: char = none_of('"').parse_next(input)?;
    if c == '\\' {
        let escaped: char = any.parse_next(input)?;
        Ok(match escaped {
            'n' => '\n',
            't' => '\t',
            'r' => '\r',
            '"' => '"',
            '\\' => '\\',
            c => c,
        })
    } else {
        Ok(c)
    }
}

fn string_literal(input: &mut &str) -> ModalResult<Expr> {
    let chars: String = delimited(
        '"',
        repeat(0.., string_char).fold(String::new, |mut s, c| {
            s.push(c);
            s
        }),
        '"',
    )
    .parse_next(input)?;
    Ok(Expr::String(chars))
}

fn regex_literal(input: &mut &str) -> ModalResult<Expr> {
    '/'.parse_next(input)?;
    let mut s = String::new();
    loop {
        let c: char = any.parse_next(input)?;
        if c == '/' {
            break;
        }
        if c == '\\' {
            let escaped: char = any.parse_next(input)?;
            s.push('\\');
            s.push(escaped);
        } else {
            s.push(c);
        }
    }
    Ok(Expr::String(s))
}

fn ident(input: &mut &str) -> ModalResult<String> {
    let first: char = one_of(|c: char| c.is_ascii_alphabetic() || c == '_').parse_next(input)?;
    let rest: &str =
        take_while(0.., |c: char| c.is_ascii_alphanumeric() || c == '_').parse_next(input)?;
    Ok(format!("{}{}", first, rest))
}

fn var_or_bool_or_func(input: &mut &str) -> ModalResult<Expr> {
    let name = ident.parse_next(input)?;

    let _ = multispace0.parse_next(input)?;
    if input.starts_with('(') {
        '('.parse_next(input)?;
        let _ = multispace0.parse_next(input)?;
        let args: Vec<Expr> = separated(0.., ws(expr), ws(',')).parse_next(input)?;
        let _ = multispace0.parse_next(input)?;
        ')'.parse_next(input)?;
        return Ok(Expr::FuncCall { name, args });
    }

    match name.as_str() {
        "true" => Ok(Expr::Bool(true)),
        "false" => Ok(Expr::Bool(false)),
        _ => Ok(Expr::Var(name)),
    }
}

fn array(input: &mut &str) -> ModalResult<Expr> {
    let elements: Vec<Expr> = delimited(
        ('[', multispace0),
        separated(0.., ws(expr), ws(',')),
        (multispace0, ']'),
    )
    .parse_next(input)?;
    Ok(Expr::Array(elements))
}

fn object_key(input: &mut &str) -> ModalResult<String> {
    alt((
        // Quoted key: "foo"
        delimited(
            '"',
            repeat(0.., string_char).fold(String::new, |mut s, c| {
                s.push(c);
                s
            }),
            '"',
        ),
        // Unquoted identifier key
        ident,
    ))
    .parse_next(input)
}

fn object_entry(input: &mut &str) -> ModalResult<(String, Expr)> {
    let key = ws(object_key).parse_next(input)?;
    ws(':').parse_next(input)?;
    let value = ws(expr).parse_next(input)?;
    Ok((key, value))
}

fn object(input: &mut &str) -> ModalResult<Expr> {
    let entries: Vec<(String, Expr)> = delimited(
        ('{', multispace0),
        separated(0.., object_entry, ws(',')),
        (multispace0, '}'),
    )
    .parse_next(input)?;
    Ok(Expr::Object(entries))
}

const TYPE_KEYWORDS: &[&str] = &["number", "string", "bool", "json_array", "json_object"];

fn type_literal(input: &mut &str) -> ModalResult<Expr> {
    for &kw in TYPE_KEYWORDS {
        if input.starts_with(kw) {
            let after = &(*input)[kw.len()..];
            let next_char = after.chars().next();
            if next_char.map(|c| c.is_ascii_alphanumeric() || c == '_').unwrap_or(false) {
                continue;
            }
            *input = after;
            return Ok(Expr::TypeLiteral(kw.to_string()));
        }
    }
    Err(winnow::error::ErrMode::Backtrack(ContextError::new()))
}

fn atom(input: &mut &str) -> ModalResult<Expr> {
    let _ = multispace0.parse_next(input)?;
    alt((
        delimited(('(', multispace0), expr, (multispace0, ')')),
        array,
        object,
        string_literal,
        regex_literal,
        number,
        type_literal,
        var_or_bool_or_func,
    ))
    .parse_next(input)
}

fn postfix(input: &mut &str) -> ModalResult<Expr> {
    let mut base = atom.parse_next(input)?;
    loop {
        let _ = multispace0.parse_next(input)?;
        if input.starts_with('[') {
            '['.parse_next(input)?;
            let _ = multispace0.parse_next(input)?;
            let index = expr.parse_next(input)?;
            let _ = multispace0.parse_next(input)?;
            ']'.parse_next(input)?;
            base = Expr::Index {
                expr: Box::new(base),
                index: Box::new(index),
            };
        } else if input.starts_with('.') {
            '.'.parse_next(input)?;
            let name = ident.parse_next(input)?;
            base = Expr::Property {
                expr: Box::new(base),
                name,
            };
        } else {
            break;
        }
    }
    Ok(base)
}

fn unary(input: &mut &str) -> ModalResult<Expr> {
    let _ = multispace0.parse_next(input)?;
    let neg: Option<char> = opt('-').parse_next(input)?;
    if neg.is_some() {
        let e = unary.parse_next(input)?;
        return Ok(Expr::UnaryOp {
            op: UnaryOp::Neg,
            expr: Box::new(e),
        });
    }
    postfix(input)
}

fn pow(input: &mut &str) -> ModalResult<Expr> {
    let base = unary.parse_next(input)?;
    let _ = multispace0.parse_next(input)?;
    let caret: Option<char> = opt('^').parse_next(input)?;
    if caret.is_some() {
        let _ = multispace0.parse_next(input)?;
        let exp = pow.parse_next(input)?;
        Ok(Expr::BinaryOp {
            op: BinaryOp::Pow,
            left: Box::new(base),
            right: Box::new(exp),
        })
    } else {
        Ok(base)
    }
}

fn term(input: &mut &str) -> ModalResult<Expr> {
    let init = pow.parse_next(input)?;

    repeat(0.., (ws(one_of(['*', '/'])), pow))
        .fold(
            move || init.clone(),
            |acc, (op_char, val): (char, Expr)| {
                let op = if op_char == '*' {
                    BinaryOp::Mul
                } else {
                    BinaryOp::Div
                };
                Expr::BinaryOp {
                    op,
                    left: Box::new(acc),
                    right: Box::new(val),
                }
            },
        )
        .parse_next(input)
}

fn arith(input: &mut &str) -> ModalResult<Expr> {
    let init = term.parse_next(input)?;

    repeat(0.., (ws(one_of(['+', '-'])), term))
        .fold(
            move || init.clone(),
            |acc, (op_char, val): (char, Expr)| {
                let op = if op_char == '+' {
                    BinaryOp::Add
                } else {
                    BinaryOp::Sub
                };
                Expr::BinaryOp {
                    op,
                    left: Box::new(acc),
                    right: Box::new(val),
                }
            },
        )
        .parse_next(input)
}

fn peek_non_ident(input: &mut &str) -> ModalResult<()> {
    let next = input.chars().next();
    if next
        .map(|c| c.is_ascii_alphanumeric() || c == '_')
        .unwrap_or(false)
    {
        Err(winnow::error::ErrMode::Backtrack(ContextError::new()))
    } else {
        Ok(())
    }
}

fn cmp_op(input: &mut &str) -> ModalResult<BinaryOp> {
    alt((
        "==".value(BinaryOp::Eq),
        "!=".value(BinaryOp::Ne),
        "<=".value(BinaryOp::Le),
        ">=".value(BinaryOp::Ge),
        "<".value(BinaryOp::Lt),
        ">".value(BinaryOp::Gt),
        terminated("in", peek_non_ident).value(BinaryOp::In),
        terminated("contains", peek_non_ident).value(BinaryOp::Contains),
        terminated("startswith", peek_non_ident).value(BinaryOp::StartsWith),
        terminated("endswith", peek_non_ident).value(BinaryOp::EndsWith),
        terminated("matches", peek_non_ident).value(BinaryOp::Matches),
    ))
    .parse_next(input)
}

fn comparison(input: &mut &str) -> ModalResult<Expr> {
    let left = arith.parse_next(input)?;
    let _ = multispace0.parse_next(input)?;

    let op_opt: Option<BinaryOp> = opt(cmp_op).parse_next(input)?;
    match op_opt {
        Some(op) => {
            let _ = multispace0.parse_next(input)?;
            let right = arith.parse_next(input)?;
            Ok(Expr::BinaryOp {
                op,
                left: Box::new(left),
                right: Box::new(right),
            })
        }
        None => Ok(left),
    }
}

fn not_expr(input: &mut &str) -> ModalResult<Expr> {
    let _ = multispace0.parse_next(input)?;
    let not_kw: Option<&str> = opt(terminated("not", peek_non_ident)).parse_next(input)?;
    if not_kw.is_some() {
        let _ = multispace0.parse_next(input)?;
        let e = not_expr.parse_next(input)?;
        Ok(Expr::UnaryOp {
            op: UnaryOp::Not,
            expr: Box::new(e),
        })
    } else {
        comparison(input)
    }
}

fn and_expr(input: &mut &str) -> ModalResult<Expr> {
    let init = not_expr.parse_next(input)?;

    repeat(
        0..,
        preceded((multispace0, "and", peek_non_ident, multispace0), not_expr),
    )
    .fold(
        move || init.clone(),
        |acc, val| Expr::BinaryOp {
            op: BinaryOp::And,
            left: Box::new(acc),
            right: Box::new(val),
        },
    )
    .parse_next(input)
}

fn or_expr(input: &mut &str) -> ModalResult<Expr> {
    let init = and_expr.parse_next(input)?;

    repeat(
        0..,
        preceded((multispace0, "or", peek_non_ident, multispace0), and_expr),
    )
    .fold(
        move || init.clone(),
        |acc, val| Expr::BinaryOp {
            op: BinaryOp::Or,
            left: Box::new(acc),
            right: Box::new(val),
        },
    )
    .parse_next(input)
}

fn forall_expr(input: &mut &str) -> ModalResult<Expr> {
    let predicate = or_expr.parse_next(input)?;
    let _ = multispace0.parse_next(input)?;

    let forall_kw: Option<&str> = opt(terminated("forall", peek_non_ident)).parse_next(input)?;
    if forall_kw.is_some() {
        let _ = multispace0.parse_next(input)?;
        let var = ident.parse_next(input)?;
        let _ = multispace0.parse_next(input)?;
        terminated("in", peek_non_ident).parse_next(input)?;
        let _ = multispace0.parse_next(input)?;
        let iterable = or_expr.parse_next(input)?;
        Ok(Expr::ForAll {
            predicate: Box::new(predicate),
            var,
            iterable: Box::new(iterable),
        })
    } else {
        Ok(predicate)
    }
}

fn expr(input: &mut &str) -> ModalResult<Expr> {
    forall_expr(input)
}

pub fn parse(input: &str) -> Result<Expr, EvalError> {
    let mut input = input.trim();
    match expr.parse_next(&mut input) {
        Ok(e) => {
            let remaining = input.trim();
            if remaining.is_empty() {
                Ok(e)
            } else {
                Err(EvalError::ParseError(format!(
                    "unexpected trailing input: {:?}",
                    remaining
                )))
            }
        }
        Err(e) => Err(EvalError::ParseError(format!("{:?}", e))),
    }
}

// ============ Evaluator ============

pub fn evaluate(expr: &Expr, vars: &HashMap<String, Value>) -> Result<Value, EvalError> {
    match expr {
        Expr::Number(n) => Ok(Value::Number(*n)),
        Expr::String(s) => Ok(Value::String(s.clone())),
        Expr::Bool(b) => Ok(Value::Bool(*b)),
        Expr::TypeLiteral(t) => Ok(Value::Type(t.clone())),
        Expr::Var(name) => vars
            .get(name)
            .cloned()
            .ok_or_else(|| EvalError::UndefinedVariable(name.clone())),
        Expr::Array(elements) => {
            let values: Result<Vec<_>, _> = elements.iter().map(|e| evaluate(e, vars)).collect();
            Ok(Value::Array(values?))
        }
        Expr::Object(entries) => {
            let mut map = HashMap::new();
            for (key, val_expr) in entries {
                map.insert(key.clone(), evaluate(val_expr, vars)?);
            }
            Ok(Value::Object(map))
        }
        Expr::UnaryOp { op, expr } => {
            let val = evaluate(expr, vars)?;
            match op {
                UnaryOp::Not => Ok(Value::Bool(!val.as_bool()?)),
                UnaryOp::Neg => Ok(Value::Number(-val.as_number()?)),
            }
        }
        Expr::BinaryOp { op, left, right } => eval_binary_op(*op, left, right, vars),
        Expr::FuncCall { name, args } => eval_func_call(name, args, vars),
        Expr::Index { expr, index } => {
            let base = evaluate(expr, vars)?;
            let idx = evaluate(index, vars)?;
            match &base {
                Value::Array(arr) => {
                    let i = idx.as_number()? as usize;
                    arr.get(i)
                        .cloned()
                        .ok_or(EvalError::IndexOutOfBounds { index: i, len: arr.len() })
                }
                Value::Object(obj) => {
                    let key = idx.as_string()?;
                    obj.get(key)
                        .cloned()
                        .ok_or_else(|| EvalError::KeyNotFound(key.to_string()))
                }
                _ => Err(EvalError::TypeError {
                    expected: "array or object",
                    got: base.type_name(),
                }),
            }
        }
        Expr::Property { expr, name } => {
            let base = evaluate(expr, vars)?;
            let obj = base.as_object()?;
            obj.get(name)
                .cloned()
                .ok_or_else(|| EvalError::KeyNotFound(name.clone()))
        }
        Expr::ForAll { predicate, var, iterable } => {
            let iter_val = evaluate(iterable, vars)?;
            let items = match &iter_val {
                Value::Array(arr) => arr.clone(),
                Value::Object(obj) => obj.values().cloned().collect(),
                _ => {
                    return Err(EvalError::TypeError {
                        expected: "array or object",
                        got: iter_val.type_name(),
                    });
                }
            };
            for item in items {
                let mut local_vars = vars.clone();
                local_vars.insert(var.clone(), item);
                let result = evaluate(predicate, &local_vars)?;
                if !result.as_bool()? {
                    return Ok(Value::Bool(false));
                }
            }
            Ok(Value::Bool(true))
        }
    }
}

fn eval_func_call(
    name: &str,
    args: &[Expr],
    vars: &HashMap<String, Value>,
) -> Result<Value, EvalError> {
    match name {
        "len" => {
            if args.len() != 1 {
                return Err(EvalError::WrongArgCount {
                    func: name.to_string(),
                    expected: 1,
                    got: args.len(),
                });
            }
            let val = evaluate(&args[0], vars)?;
            match val {
                Value::String(s) => Ok(Value::Number(s.len() as f64)),
                Value::Array(a) => Ok(Value::Number(a.len() as f64)),
                Value::Object(o) => Ok(Value::Number(o.len() as f64)),
                _ => Err(EvalError::TypeError {
                    expected: "string, array, or object",
                    got: val.type_name(),
                }),
            }
        }
        "type" => {
            if args.len() != 1 {
                return Err(EvalError::WrongArgCount {
                    func: name.to_string(),
                    expected: 1,
                    got: args.len(),
                });
            }
            let val = evaluate(&args[0], vars)?;
            Ok(Value::Type(val.type_name().to_string()))
        }
        "keys" => {
            if args.len() != 1 {
                return Err(EvalError::WrongArgCount {
                    func: name.to_string(),
                    expected: 1,
                    got: args.len(),
                });
            }
            let val = evaluate(&args[0], vars)?;
            let obj = val.as_object()?;
            let keys: Vec<Value> = obj.keys().map(|k| Value::String(k.clone())).collect();
            Ok(Value::Array(keys))
        }
        _ => Err(EvalError::UndefinedFunction(name.to_string())),
    }
}

fn eval_binary_op(
    op: BinaryOp,
    left: &Expr,
    right: &Expr,
    vars: &HashMap<String, Value>,
) -> Result<Value, EvalError> {
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
        (Value::Object(a), Value::Object(b)) => {
            a.len() == b.len()
                && a.iter().all(|(k, v)| b.get(k).map(|bv| values_equal(v, bv)).unwrap_or(false))
        }
        (Value::Type(a), Value::Type(b)) => a == b,
        _ => false,
    }
}

// ============ Public API ============

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
        assert_eq!(parse("0.5").unwrap(), Expr::Number(0.5));
    }

    #[test]
    fn test_string_parsing() {
        assert_eq!(
            parse(r#""hello""#).unwrap(),
            Expr::String("hello".to_string())
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
        assert!(eval_bool("1 + 2 * 3 == 7", &v).unwrap());
        assert!(eval_bool("(1 + 2) * 3 == 9", &v).unwrap());
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
    }

    #[test]
    fn test_in_operator() {
        let v = vars(&[("n", Value::Number(2.0))]);
        assert!(eval_bool("n in [1, 2, 3]", &v).unwrap());
        assert!(!eval_bool("n in [4, 5, 6]", &v).unwrap());
    }

    #[test]
    fn test_string_operators() {
        let v = vars(&[("s", Value::String("hello world".to_string()))]);
        assert!(eval_bool(r#"s contains "world""#, &v).unwrap());
        assert!(eval_bool(r#"s startswith "hello""#, &v).unwrap());
        assert!(eval_bool(r#"s endswith "world""#, &v).unwrap());
    }

    #[test]
    fn test_regex_matches() {
        let v = vars(&[("s", Value::String("hello123".to_string()))]);
        assert!(eval_bool(r#"s matches /^hello\d+$/"#, &v).unwrap());
    }

    #[test]
    fn test_len_function() {
        let v = vars(&[("s", Value::String("hello".to_string()))]);
        assert!(eval_bool("len(s) == 5", &v).unwrap());
    }

    #[test]
    fn test_backslash_in_string() {
        // Test that backslash is parsed correctly
        let v = vars(&[("p", Value::String("C:\\Users\\test".to_string()))]);

        // Should contain "test"
        assert!(eval_bool(r#"p contains "test""#, &v).unwrap());

        // Should contain backslash (escaped in the expression)
        assert!(eval_bool(r#"p contains "\\""#, &v).unwrap());

        // Should contain "Users"
        assert!(eval_bool(r#"p contains "Users""#, &v).unwrap());
    }
}
