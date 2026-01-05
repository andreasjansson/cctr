use std::collections::HashMap;
use winnow::ascii::{digit1, multispace0};
use winnow::combinator::{alt, delimited, opt, preceded, repeat, separated};
use winnow::prelude::*;
use winnow::token::{any, take_while};

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
                expected: "bool".to_string(),
                got: self.type_name().to_string(),
            }),
        }
    }

    pub fn as_number(&self) -> Result<f64, EvalError> {
        match self {
            Value::Number(n) => Ok(*n),
            _ => Err(EvalError::TypeError {
                expected: "number".to_string(),
                got: self.type_name().to_string(),
            }),
        }
    }

    pub fn as_string(&self) -> Result<&str, EvalError> {
        match self {
            Value::String(s) => Ok(s),
            _ => Err(EvalError::TypeError {
                expected: "string".to_string(),
                got: self.type_name().to_string(),
            }),
        }
    }

    pub fn as_array(&self) -> Result<&[Value], EvalError> {
        match self {
            Value::Array(a) => Ok(a),
            _ => Err(EvalError::TypeError {
                expected: "array".to_string(),
                got: self.type_name().to_string(),
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
    Regex(String),
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

#[derive(Debug, Clone, PartialEq)]
pub enum EvalError {
    TypeError { expected: String, got: String },
    UndefinedVariable(String),
    InvalidRegex(String),
    DivisionByZero,
    ParseError(String),
}

impl std::fmt::Display for EvalError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EvalError::TypeError { expected, got } => {
                write!(f, "type error: expected {}, got {}", expected, got)
            }
            EvalError::UndefinedVariable(name) => write!(f, "undefined variable: {}", name),
            EvalError::InvalidRegex(e) => write!(f, "invalid regex: {}", e),
            EvalError::DivisionByZero => write!(f, "division by zero"),
            EvalError::ParseError(e) => write!(f, "parse error: {}", e),
        }
    }
}

impl std::error::Error for EvalError {}

// ============ Parser ============

fn ws<'a, O, E: winnow::error::ParserError<&'a str>, P: Parser<&'a str, O, E>>(
    mut p: P,
) -> impl Parser<&'a str, O, E> {
    move |input: &mut &'a str| {
        let _ = multispace0.parse_next(input)?;
        let result = p.parse_next(input)?;
        let _ = multispace0.parse_next(input)?;
        Ok(result)
    }
}

fn number(input: &mut &str) -> PResult<Expr> {
    let neg = opt('-').parse_next(input)?;
    let int_part = digit1.parse_next(input)?;
    let frac_part = opt(('.', digit1)).parse_next(input)?;

    let mut s = String::new();
    if neg.is_some() {
        s.push('-');
    }
    s.push_str(int_part);
    if let Some((_, frac)) = frac_part {
        s.push('.');
        s.push_str(frac);
    }

    let n: f64 = s.parse().unwrap();
    Ok(Expr::Number(n))
}

fn string_literal(input: &mut &str) -> PResult<Expr> {
    '"'.parse_next(input)?;
    let mut s = String::new();
    loop {
        let c = any.parse_next(input)?;
        if c == '"' {
            break;
        }
        if c == '\\' {
            let escaped = any.parse_next(input)?;
            match escaped {
                'n' => s.push('\n'),
                't' => s.push('\t'),
                'r' => s.push('\r'),
                '"' => s.push('"'),
                '\\' => s.push('\\'),
                _ => {
                    s.push('\\');
                    s.push(escaped);
                }
            }
        } else {
            s.push(c);
        }
    }
    Ok(Expr::String(s))
}

fn regex_literal(input: &mut &str) -> PResult<Expr> {
    '/'.parse_next(input)?;
    let mut s = String::new();
    loop {
        let c = any.parse_next(input)?;
        if c == '/' {
            break;
        }
        if c == '\\' {
            let escaped = any.parse_next(input)?;
            s.push('\\');
            s.push(escaped);
        } else {
            s.push(c);
        }
    }
    Ok(Expr::Regex(s))
}

fn ident(input: &mut &str) -> PResult<String> {
    let first = take_while(1, |c: char| c.is_ascii_alphabetic() || c == '_').parse_next(input)?;
    let rest =
        take_while(0.., |c: char| c.is_ascii_alphanumeric() || c == '_').parse_next(input)?;
    Ok(format!("{}{}", first, rest))
}

fn keyword<'a>(kw: &'static str) -> impl Parser<&'a str, &'a str, winnow::error::ContextError> {
    move |input: &mut &'a str| {
        let start = *input;
        let id = ident.parse_next(input)?;
        if id == kw {
            Ok(&start[..kw.len()])
        } else {
            *input = start;
            Err(winnow::error::ErrMode::Backtrack(
                winnow::error::ContextError::new(),
            ))
        }
    }
}

fn var(input: &mut &str) -> PResult<Expr> {
    let name = ident.parse_next(input)?;
    match name.as_str() {
        "true" => Ok(Expr::BinaryOp {
            op: BinaryOp::Eq,
            left: Box::new(Expr::Number(1.0)),
            right: Box::new(Expr::Number(1.0)),
        }),
        "false" => Ok(Expr::BinaryOp {
            op: BinaryOp::Eq,
            left: Box::new(Expr::Number(1.0)),
            right: Box::new(Expr::Number(0.0)),
        }),
        _ => Ok(Expr::Var(name)),
    }
}

fn array(input: &mut &str) -> PResult<Expr> {
    '['.parse_next(input)?;
    let _ = multispace0.parse_next(input)?;
    let elements: Vec<Expr> = separated(0.., ws(expr), ws(',')).parse_next(input)?;
    let _ = multispace0.parse_next(input)?;
    ']'.parse_next(input)?;
    Ok(Expr::Array(elements))
}

fn atom(input: &mut &str) -> PResult<Expr> {
    alt((
        delimited(ws('('), expr, ws(')')),
        array,
        number,
        string_literal,
        regex_literal,
        var,
    ))
    .parse_next(input)
}

fn unary(input: &mut &str) -> PResult<Expr> {
    let _ = multispace0.parse_next(input)?;
    if let Ok(_) = '-'.parse_next(input) {
        let e = unary.parse_next(input)?;
        return Ok(Expr::UnaryOp {
            op: UnaryOp::Neg,
            expr: Box::new(e),
        });
    }
    atom(input)
}

fn pow_expr(input: &mut &str) -> PResult<Expr> {
    let base = unary.parse_next(input)?;
    let _ = multispace0.parse_next(input)?;
    if let Ok(_) = '^'.parse_next(input) {
        let _ = multispace0.parse_next(input)?;
        let exp = pow_expr.parse_next(input)?;
        Ok(Expr::BinaryOp {
            op: BinaryOp::Pow,
            left: Box::new(base),
            right: Box::new(exp),
        })
    } else {
        Ok(base)
    }
}

fn mul_expr(input: &mut &str) -> PResult<Expr> {
    let mut left = pow_expr.parse_next(input)?;
    loop {
        let _ = multispace0.parse_next(input)?;
        let op = alt(('*'.value(BinaryOp::Mul), '/'.value(BinaryOp::Div))).parse_next(input);
        match op {
            Ok(op) => {
                let _ = multispace0.parse_next(input)?;
                let right = pow_expr.parse_next(input)?;
                left = Expr::BinaryOp {
                    op,
                    left: Box::new(left),
                    right: Box::new(right),
                };
            }
            Err(_) => break,
        }
    }
    Ok(left)
}

fn add_expr(input: &mut &str) -> PResult<Expr> {
    let mut left = mul_expr.parse_next(input)?;
    loop {
        let _ = multispace0.parse_next(input)?;
        let op = alt(('+'.value(BinaryOp::Add), '-'.value(BinaryOp::Sub))).parse_next(input);
        match op {
            Ok(op) => {
                let _ = multispace0.parse_next(input)?;
                let right = mul_expr.parse_next(input)?;
                left = Expr::BinaryOp {
                    op,
                    left: Box::new(left),
                    right: Box::new(right),
                };
            }
            Err(_) => break,
        }
    }
    Ok(left)
}

fn cmp_op(input: &mut &str) -> PResult<BinaryOp> {
    alt((
        "==".value(BinaryOp::Eq),
        "!=".value(BinaryOp::Ne),
        "<=".value(BinaryOp::Le),
        ">=".value(BinaryOp::Ge),
        "<".value(BinaryOp::Lt),
        ">".value(BinaryOp::Gt),
        keyword("in").value(BinaryOp::In),
        keyword("contains").value(BinaryOp::Contains),
        keyword("startswith").value(BinaryOp::StartsWith),
        keyword("endswith").value(BinaryOp::EndsWith),
        keyword("matches").value(BinaryOp::Matches),
    ))
    .parse_next(input)
}

fn cmp_expr(input: &mut &str) -> PResult<Expr> {
    let left = add_expr.parse_next(input)?;
    let _ = multispace0.parse_next(input)?;
    if let Ok(op) = cmp_op.parse_next(input) {
        let _ = multispace0.parse_next(input)?;
        let right = add_expr.parse_next(input)?;
        Ok(Expr::BinaryOp {
            op,
            left: Box::new(left),
            right: Box::new(right),
        })
    } else {
        Ok(left)
    }
}

fn not_expr(input: &mut &str) -> PResult<Expr> {
    let _ = multispace0.parse_next(input)?;
    if let Ok(_) = keyword("not").parse_next(input) {
        let _ = multispace0.parse_next(input)?;
        let e = not_expr.parse_next(input)?;
        Ok(Expr::UnaryOp {
            op: UnaryOp::Not,
            expr: Box::new(e),
        })
    } else {
        cmp_expr(input)
    }
}

fn and_expr(input: &mut &str) -> PResult<Expr> {
    let mut left = not_expr.parse_next(input)?;
    loop {
        let _ = multispace0.parse_next(input)?;
        if let Ok(_) = keyword("and").parse_next(input) {
            let _ = multispace0.parse_next(input)?;
            let right = not_expr.parse_next(input)?;
            left = Expr::BinaryOp {
                op: BinaryOp::And,
                left: Box::new(left),
                right: Box::new(right),
            };
        } else {
            break;
        }
    }
    Ok(left)
}

fn or_expr(input: &mut &str) -> PResult<Expr> {
    let mut left = and_expr.parse_next(input)?;
    loop {
        let _ = multispace0.parse_next(input)?;
        if let Ok(_) = keyword("or").parse_next(input) {
            let _ = multispace0.parse_next(input)?;
            let right = and_expr.parse_next(input)?;
            left = Expr::BinaryOp {
                op: BinaryOp::Or,
                left: Box::new(left),
                right: Box::new(right),
            };
        } else {
            break;
        }
    }
    Ok(left)
}

fn expr(input: &mut &str) -> PResult<Expr> {
    or_expr(input)
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
        Expr::Regex(r) => Ok(Value::String(r.clone())),
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
            let pattern = match &r {
                Value::String(p) => p,
                _ => {
                    return Err(EvalError::TypeError {
                        expected: "string or regex".to_string(),
                        got: r.type_name().to_string(),
                    })
                }
            };
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
        assert_eq!(
            parse(r#""line\nbreak""#).unwrap(),
            Expr::String("line\nbreak".to_string())
        );
    }

    #[test]
    fn test_arithmetic() {
        let v = vars(&[]);
        assert_eq!(eval_bool("1 + 2 == 3", &v).unwrap(), true);
        assert_eq!(eval_bool("10 - 3 == 7", &v).unwrap(), true);
        assert_eq!(eval_bool("4 * 5 == 20", &v).unwrap(), true);
        assert_eq!(eval_bool("10 / 2 == 5", &v).unwrap(), true);
        assert_eq!(eval_bool("2 ^ 3 == 8", &v).unwrap(), true);
    }

    #[test]
    fn test_comparisons() {
        let v = vars(&[("n", Value::Number(42.0))]);
        assert_eq!(eval_bool("n > 0", &v).unwrap(), true);
        assert_eq!(eval_bool("n < 100", &v).unwrap(), true);
        assert_eq!(eval_bool("n >= 42", &v).unwrap(), true);
        assert_eq!(eval_bool("n <= 42", &v).unwrap(), true);
        assert_eq!(eval_bool("n == 42", &v).unwrap(), true);
        assert_eq!(eval_bool("n != 0", &v).unwrap(), true);
    }

    #[test]
    fn test_boolean_logic() {
        let v = vars(&[("n", Value::Number(42.0))]);
        assert_eq!(eval_bool("n > 0 and n < 100", &v).unwrap(), true);
        assert_eq!(eval_bool("n < 0 or n > 0", &v).unwrap(), true);
        assert_eq!(eval_bool("not (n < 0)", &v).unwrap(), true);
        assert_eq!(eval_bool("not n < 0", &v).unwrap(), true);
    }

    #[test]
    fn test_in_operator() {
        let v = vars(&[("n", Value::Number(2.0))]);
        assert_eq!(eval_bool("n in [1, 2, 3]", &v).unwrap(), true);
        assert_eq!(eval_bool("n in [4, 5, 6]", &v).unwrap(), false);

        let v = vars(&[("s", Value::String("bar".to_string()))]);
        assert_eq!(
            eval_bool(r#"s in ["foo", "bar", "baz"]"#, &v).unwrap(),
            true
        );
    }

    #[test]
    fn test_string_operators() {
        let v = vars(&[("s", Value::String("hello world".to_string()))]);
        assert_eq!(eval_bool(r#"s contains "world""#, &v).unwrap(), true);
        assert_eq!(eval_bool(r#"s startswith "hello""#, &v).unwrap(), true);
        assert_eq!(eval_bool(r#"s endswith "world""#, &v).unwrap(), true);
        assert_eq!(eval_bool(r#"s contains "xyz""#, &v).unwrap(), false);
    }

    #[test]
    fn test_regex_matches() {
        let v = vars(&[("s", Value::String("hello123".to_string()))]);
        assert_eq!(eval_bool(r#"s matches /^hello\d+$/"#, &v).unwrap(), true);
        assert_eq!(eval_bool(r#"s matches /^world/"#, &v).unwrap(), false);
    }

    #[test]
    fn test_parentheses() {
        let v = vars(&[]);
        assert_eq!(eval_bool("(1 + 2) * 3 == 9", &v).unwrap(), true);
        assert_eq!(eval_bool("1 + 2 * 3 == 7", &v).unwrap(), true);
    }

    #[test]
    fn test_true_false() {
        let v = vars(&[]);
        assert_eq!(eval_bool("true", &v).unwrap(), true);
        assert_eq!(eval_bool("false", &v).unwrap(), false);
        assert_eq!(eval_bool("true and false", &v).unwrap(), false);
        assert_eq!(eval_bool("true or false", &v).unwrap(), true);
    }
}
