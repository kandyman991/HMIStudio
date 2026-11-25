//! Bounded, read-only expression contract 1 candidate.
use std::collections::{BTreeMap, BTreeSet};
pub const CONTRACT_VERSION: u32 = 1;
pub const MAX_SOURCE_UTF16: usize = 1024;
const MAX_DEPTH: usize = 64;
const MAX_TEXT: usize = 1024 * 1024;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Type {
    Bool,
    Number,
    Text,
}
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Bool(bool),
    Number(f64),
    Text(String),
}
impl Value {
    fn ty(&self) -> Type {
        match self {
            Self::Bool(_) => Type::Bool,
            Self::Number(_) => Type::Number,
            Self::Text(_) => Type::Text,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Quality {
    Good,
    Uncertain,
    Bad,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Reading {
    pub value: Option<Value>,
    pub quality: Quality,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Evaluation {
    pub value: Option<Value>,
    pub quality: Quality,
    pub error: Option<String>,
}
impl Evaluation {
    fn good(v: Value, q: Quality) -> Self {
        Self {
            value: Some(v),
            quality: q,
            error: None,
        }
    }
    fn bad(s: &str) -> Self {
        Self {
            value: None,
            quality: Quality::Bad,
            error: Some(s.into()),
        }
    }
    fn number(n: f64, q: Quality) -> Self {
        if n.is_finite() {
            Self::good(Value::Number(n), q)
        } else {
            Self::bad("non-finite result")
        }
    }
    fn text(s: String, q: Quality) -> Self {
        if s.len() <= MAX_TEXT {
            Self::good(Value::Text(s), q)
        } else {
            Self::bad("text output limit")
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Problem {
    pub position: usize,
    pub message: String,
}
fn err(p: usize, m: &str) -> Problem {
    Problem {
        position: p,
        message: m.into(),
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Op {
    Or,
    And,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    Not,
}
impl Op {
    fn prec(self) -> u8 {
        match self {
            Self::Or => 1,
            Self::And => 2,
            Self::Eq | Self::Ne => 3,
            Self::Lt | Self::Le | Self::Gt | Self::Ge => 4,
            Self::Add | Self::Sub => 5,
            Self::Mul | Self::Div | Self::Rem => 6,
            Self::Not => 0,
        }
    }
}
#[derive(Clone, Debug, PartialEq)]
enum Tok {
    Lit(Value),
    Name(String, bool),
    Op(Op),
    Open,
    Close,
    Q,
    Colon,
    Comma,
    End,
}
fn lex(s: &str) -> Result<Vec<(Tok, usize)>, Problem> {
    if s.encode_utf16().count() > MAX_SOURCE_UTF16 {
        return Err(err(MAX_SOURCE_UTF16, "expression too long"));
    }
    let c: Vec<char> = s.chars().collect();
    let mut offsets = Vec::new();
    let mut off = 0;
    for ch in &c {
        offsets.push(off);
        off += ch.len_utf16();
    }
    offsets.push(off);
    let mut i = 0;
    let mut out = Vec::new();
    while i < c.len() {
        let ch = c[i];
        let start = i;
        let pos = offsets[i];
        if ch.is_whitespace() {
            i += 1;
            continue;
        }
        let t = if ch.is_ascii_digit()
            || (ch == '.' && c.get(i + 1).is_some_and(char::is_ascii_digit))
        {
            while i < c.len() && (c[i].is_ascii_digit() || c[i] == '.') {
                i += 1;
            }
            if matches!(c.get(i), Some('e' | 'E')) {
                i += 1;
                if matches!(c.get(i), Some('+' | '-')) {
                    i += 1;
                }
                while i < c.len() && c[i].is_ascii_digit() {
                    i += 1;
                }
            }
            let n: String = c[start..i].iter().collect();
            Tok::Lit(Value::Number(
                n.parse::<f64>()
                    .ok()
                    .filter(|x| x.is_finite())
                    .ok_or_else(|| err(pos, "invalid number"))?,
            ))
        } else if matches!(ch, '"' | '\'') {
            i += 1;
            let mut v = String::new();
            loop {
                let a = *c.get(i).ok_or_else(|| err(pos, "unclosed text"))?;
                i += 1;
                if a == ch {
                    break;
                }
                if a == '\\' {
                    let b = *c.get(i).ok_or_else(|| err(pos, "unclosed text"))?;
                    i += 1;
                    v.push(match b {
                        'n' => '\n',
                        't' => '\t',
                        x => x,
                    });
                } else {
                    v.push(a);
                }
            }
            Tok::Lit(Value::Text(v))
        } else if ch == '`' {
            i += 1;
            let from = i;
            while i < c.len() && c[i] != '`' {
                i += 1;
            }
            if i == c.len() || i == from {
                return Err(err(pos, "empty or unclosed reference"));
            }
            let v = c[from..i].iter().collect();
            i += 1;
            Tok::Name(v, true)
        } else if ch.is_alphabetic() || ch == '_' {
            i += 1;
            while i < c.len()
                && (c[i].is_alphanumeric()
                    || c[i] == '_'
                    || (c[i] == '.'
                        && c.get(i + 1).is_some_and(|x| x.is_alphabetic() || *x == '_')))
            {
                i += 1;
            }
            let v: String = c[start..i].iter().collect();
            match v.as_str() {
                "true" => Tok::Lit(Value::Bool(true)),
                "false" => Tok::Lit(Value::Bool(false)),
                _ => Tok::Name(v, false),
            }
        } else {
            i += 1;
            let pair = match (ch, c.get(i).copied()) {
                ('&', Some('&')) => Some(Op::And),
                ('|', Some('|')) => Some(Op::Or),
                ('=', Some('=')) => Some(Op::Eq),
                ('!', Some('=')) => Some(Op::Ne),
                ('<', Some('=')) => Some(Op::Le),
                ('>', Some('=')) => Some(Op::Ge),
                _ => None,
            };
            if let Some(op) = pair {
                i += 1;
                Tok::Op(op)
            } else {
                match ch {
                    '+' => Tok::Op(Op::Add),
                    '-' => Tok::Op(Op::Sub),
                    '*' => Tok::Op(Op::Mul),
                    '/' => Tok::Op(Op::Div),
                    '%' => Tok::Op(Op::Rem),
                    '<' => Tok::Op(Op::Lt),
                    '>' => Tok::Op(Op::Gt),
                    '!' => Tok::Op(Op::Not),
                    '(' => Tok::Open,
                    ')' => Tok::Close,
                    '?' => Tok::Q,
                    ':' => Tok::Colon,
                    ',' => Tok::Comma,
                    _ => return Err(err(pos, "unexpected character")),
                }
            }
        };
        out.push((t, pos));
    }
    out.push((Tok::End, off));
    Ok(out)
}
#[derive(Clone, Debug)]
struct Expr {
    p: usize,
    k: Kind,
}
#[derive(Clone, Debug)]
enum Kind {
    Lit(Value),
    Ref(String),
    Unary(Op, Box<Expr>),
    Binary(Op, Box<Expr>, Box<Expr>),
    Cond(Box<Expr>, Box<Expr>, Box<Expr>),
    Call(String, Vec<Expr>),
}
struct Parser {
    t: Vec<(Tok, usize)>,
    i: usize,
}
impl Parser {
    fn peek(&self) -> &(Tok, usize) {
        &self.t[self.i]
    }
    fn take(&mut self) -> (Tok, usize) {
        let t = self.peek().clone();
        if t.0 != Tok::End {
            self.i += 1;
        }
        t
    }
    fn eat(&mut self, t: Tok) -> bool {
        if self.peek().0 == t {
            self.take();
            true
        } else {
            false
        }
    }
    fn expect(&mut self, t: Tok) -> Result<(), Problem> {
        if self.eat(t) {
            Ok(())
        } else {
            Err(err(self.peek().1, "expected delimiter"))
        }
    }
    fn cond(&mut self, d: usize) -> Result<Expr, Problem> {
        if d > MAX_DEPTH {
            return Err(err(self.peek().1, "expression nested too deeply"));
        }
        let a = self.binary(1, d)?;
        let p = self.peek().1;
        if self.eat(Tok::Q) {
            let b = self.cond(d + 1)?;
            self.expect(Tok::Colon)?;
            let c = self.cond(d + 1)?;
            Ok(Expr {
                p,
                k: Kind::Cond(Box::new(a), Box::new(b), Box::new(c)),
            })
        } else {
            Ok(a)
        }
    }
    fn binary(&mut self, min: u8, d: usize) -> Result<Expr, Problem> {
        let mut a = self.primary(d)?;
        while let Tok::Op(op) = self.peek().0 {
            let prec = op.prec();
            if prec < min {
                break;
            }
            let p = self.take().1;
            let b = self.binary(prec + 1, d)?;
            a = Expr {
                p,
                k: Kind::Binary(op, Box::new(a), Box::new(b)),
            };
        }
        Ok(a)
    }
    fn primary(&mut self, d: usize) -> Result<Expr, Problem> {
        if d > MAX_DEPTH {
            return Err(err(self.peek().1, "expression nested too deeply"));
        }
        let (t, p) = self.take();
        let k = match t {
            Tok::Lit(v) => Kind::Lit(v),
            Tok::Op(op @ (Op::Not | Op::Sub)) => Kind::Unary(op, Box::new(self.primary(d + 1)?)),
            Tok::Open => {
                let a = self.cond(d + 1)?;
                self.expect(Tok::Close)?;
                return Ok(a);
            }
            Tok::Name(n, false) if self.eat(Tok::Open) => {
                let mut a = Vec::new();
                if !self.eat(Tok::Close) {
                    loop {
                        a.push(self.cond(d + 1)?);
                        if !self.eat(Tok::Comma) {
                            break;
                        }
                    }
                    self.expect(Tok::Close)?;
                }
                Kind::Call(n, a)
            }
            Tok::Name(n, _) => Kind::Ref(n),
            _ => return Err(err(p, "expected expression")),
        };
        Ok(Expr { p, k })
    }
}
fn infer(
    e: &Expr,
    types: &BTreeMap<String, Type>,
    refs: &mut Vec<String>,
    seen: &mut BTreeSet<String>,
    errors: &mut Vec<Problem>,
) -> Option<Type> {
    let mut child = |e: &Expr| infer(e, types, refs, seen, errors);
    let r = match &e.k {
        Kind::Lit(v) => return Some(v.ty()),
        Kind::Ref(id) => {
            if seen.insert(id.clone()) {
                refs.push(id.clone());
            }
            if let Some(t) = types.get(id) {
                return Some(*t);
            }
            errors.push(err(e.p, &format!("unknown or unsupported variable {id}")));
            return None;
        }
        Kind::Unary(op, a) => child(a).and_then(|t| match (op, t) {
            (Op::Not, Type::Bool) => Some(Type::Bool),
            (Op::Sub, Type::Number) => Some(Type::Number),
            _ => None,
        }),
        Kind::Binary(op, a, b) => {
            let l = child(a);
            let r = child(b);
            match (l, r) {
                (Some(l), Some(r)) => match op {
                    Op::And | Op::Or if l == Type::Bool && l == r => Some(Type::Bool),
                    Op::Eq | Op::Ne if l == r => Some(Type::Bool),
                    Op::Lt | Op::Le | Op::Gt | Op::Ge if l == Type::Number && l == r => {
                        Some(Type::Bool)
                    }
                    Op::Add if l == Type::Text || r == Type::Text => Some(Type::Text),
                    Op::Add | Op::Sub | Op::Mul | Op::Div | Op::Rem
                        if l == Type::Number && l == r =>
                    {
                        Some(Type::Number)
                    }
                    _ => None,
                },
                _ => return None,
            }
        }
        Kind::Cond(c, a, b) => {
            let c = child(c);
            let a = child(a);
            let b = child(b);
            if c == Some(Type::Bool) && a == b {
                a
            } else {
                None
            }
        }
        Kind::Call(n, a) => {
            let t: Vec<_> = a.iter().map(&mut child).collect();
            if t.iter().any(Option::is_none) {
                return None;
            }
            let numeric = t.iter().all(|x| *x == Some(Type::Number));
            match (n.as_str(), t.len()) {
                ("text", 1) => Some(Type::Text),
                ("isGood", 1) => Some(Type::Bool),
                ("ifBad", 2) if t[0] == t[1] => t[0],
                ("abs" | "floor" | "ceil", 1)
                | ("min" | "max", 2)
                | ("round", 1 | 2)
                | ("clamp", 3)
                | ("scale", 5)
                    if numeric =>
                {
                    Some(Type::Number)
                }
                _ => None,
            }
        }
    };
    if r.is_none() {
        errors.push(err(
            e.p,
            "invalid operand types, function or argument count",
        ));
    }
    r
}
/// Only successfully checked expressions can be evaluated. Reads are snapshots,
/// not callbacks; the evaluator cannot perform device operations.
#[derive(Clone, Debug)]
pub struct CompiledExpression {
    root: Expr,
    result: Type,
    refs: Vec<String>,
    types: BTreeMap<String, Type>,
}
impl CompiledExpression {
    pub fn result_type(&self) -> Type {
        self.result
    }
    pub fn references(&self) -> &[String] {
        &self.refs
    }
    pub fn evaluate(&self, values: &BTreeMap<String, Reading>) -> Evaluation {
        eval(&self.root, &self.types, values)
    }
}
pub fn compile(
    source: &str,
    types: &BTreeMap<String, Type>,
) -> Result<CompiledExpression, Vec<Problem>> {
    let mut p = Parser {
        t: lex(source).map_err(|e| vec![e])?,
        i: 0,
    };
    let root = p.cond(1).map_err(|e| vec![e])?;
    if p.peek().0 != Tok::End {
        return Err(vec![err(p.peek().1, "unexpected trailing input")]);
    }
    let mut refs = Vec::new();
    let mut errors = Vec::new();
    let result = infer(&root, types, &mut refs, &mut BTreeSet::new(), &mut errors);
    match result {
        Some(result) if errors.is_empty() => {
            let types = refs
                .iter()
                .filter_map(|id| types.get(id).map(|t| (id.clone(), *t)))
                .collect();
            Ok(CompiledExpression {
                root,
                result,
                refs,
                types,
            })
        }
        _ => Err(errors),
    }
}
fn text(v: &Value) -> String {
    match v {
        Value::Bool(b) => {
            if *b {
                "TRUE".into()
            } else {
                "FALSE".into()
            }
        }
        Value::Text(s) => s.clone(),
        Value::Number(n) => {
            if n.fract() == 0.0 && n.abs() < 1e15 {
                return (*n as i64).to_string();
            }
            // JVM Double.toString selects the nearest shortest decimal,
            // with at least two significant digits for scientific notation.
            // Rust's default shortest formatting differs at ties and at
            // the smallest subnormal values. Select the first nearest
            // rounded decimal that round-trips to the original binary64.
            let scientific = shortest_jvm_decimal(*n);
            let Some((mantissa, exponent)) = scientific.split_once('e') else {
                return scientific;
            };
            if n.abs() >= 1e7 || n.abs() < 1e-3 {
                return format!("{mantissa}E{exponent}");
            }
            // For the JVM plain-decimal interval, shift the decimal point
            // without reparsing or re-rounding the selected digits.
            let exponent = exponent.parse::<i32>().unwrap_or(0);
            let unsigned = mantissa.trim_start_matches('-');
            let digits = unsigned.replace('.', "");
            let point = 1 + exponent;
            let mut out = if point <= 0 {
                format!("0.{}{digits}", "0".repeat((-point) as usize))
            } else if point as usize >= digits.len() {
                format!("{digits}{}", "0".repeat(point as usize - digits.len()))
            } else {
                let (left, right) = digits.split_at(point as usize);
                format!("{left}.{right}")
            };
            if out.contains('.') {
                out = out.trim_end_matches('0').trim_end_matches('.').to_owned();
            }
            if n.is_sign_negative() {
                out.insert(0, '-');
            }
            out
        }
    }
}
/// Select the shortest round-tripping decimal, then the nearest candidate
/// at that precision. At a binary power-of-two boundary the rounding
/// interval is asymmetric, so the nearest rounded decimal can be outside
/// the interval while its immediate decimal neighbor is inside.
fn shortest_jvm_decimal(n: f64) -> String {
    let matches = |s: &str| s.parse::<f64>().is_ok_and(|v| v.to_bits() == n.to_bits());
    for precision in 1..=16 {
        let nearest = format!("{n:.precision$e}");
        if matches(&nearest) {
            return nearest;
        }
        let Some((m, exp)) = nearest.split_once('e') else {
            continue;
        };
        let Ok(exponent) = exp.parse::<i32>() else {
            continue;
        };
        let negative = m.starts_with('-');
        let Ok(significand) = m.trim_start_matches('-').replace('.', "").parse::<u64>() else {
            continue;
        };
        for adjacent in [significand.saturating_sub(1), significand + 1] {
            if adjacent == 0 {
                continue;
            }
            let digits = adjacent.to_string();
            let adjusted_exp = exponent + digits.len() as i32 - (precision as i32 + 1);
            let fraction = &digits[1..];
            let candidate = format!(
                "{}{}.{}e{adjusted_exp}",
                if negative { "-" } else { "" },
                &digits[..1],
                if fraction.is_empty() { "0" } else { fraction }
            );
            if matches(&candidate) {
                return candidate;
            }
        }
    }
    format!("{n:.16e}")
}
fn equal(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(a), Value::Number(b)) => a.to_bits() == b.to_bits(),
        _ => a == b,
    }
}
fn eval(
    e: &Expr,
    types: &BTreeMap<String, Type>,
    values: &BTreeMap<String, Reading>,
) -> Evaluation {
    let run = |e: &Expr| eval(e, types, values);
    match &e.k {
        Kind::Lit(v) => Evaluation::good(v.clone(), Quality::Good),
        Kind::Ref(id) => {
            let Some(r) = values.get(id) else {
                return Evaluation::bad("missing variable value");
            };
            if r.quality == Quality::Bad {
                return Evaluation::bad("bad variable quality");
            }
            let Some(v) = &r.value else {
                return Evaluation::bad("missing variable value");
            };
            if types.get(id) != Some(&v.ty()) {
                return Evaluation::bad("variable type mismatch");
            }
            match v {
                Value::Number(n) => Evaluation::number(*n, r.quality),
                Value::Text(s) if s.len() > MAX_TEXT => Evaluation::bad("text input limit"),
                _ => Evaluation::good(v.clone(), r.quality),
            }
        }
        Kind::Unary(op, a) => {
            let a = run(a);
            match (op, &a.value) {
                (_, None) => a,
                (Op::Not, Some(Value::Bool(b))) => Evaluation::good(Value::Bool(!b), a.quality),
                (Op::Sub, Some(Value::Number(n))) => Evaluation::number(-n, a.quality),
                _ => Evaluation::bad("invalid unary operand"),
            }
        }
        Kind::Cond(c, a, b) => {
            let c = run(c);
            match c.value {
                Some(Value::Bool(v)) => {
                    let mut r = run(if v { a } else { b });
                    r.quality = r.quality.max(c.quality);
                    r
                }
                None => c,
                _ => Evaluation::bad("invalid condition"),
            }
        }
        Kind::Binary(op, a, b) => {
            let a = run(a);
            if a.value.is_none()
                || (*op == Op::And && a.value == Some(Value::Bool(false)))
                || (*op == Op::Or && a.value == Some(Value::Bool(true)))
            {
                return a;
            }
            let b = run(b);
            let q = a.quality.max(b.quality);
            let (Some(l), Some(r)) = (&a.value, &b.value) else {
                return b;
            };
            let boolean = |v| Evaluation::good(Value::Bool(v), q);
            match (op, l, r) {
                (Op::Eq, _, _) => boolean(equal(l, r)),
                (Op::Ne, _, _) => boolean(!equal(l, r)),
                (Op::And | Op::Or, Value::Bool(_), Value::Bool(v)) => boolean(*v),
                (Op::Add, Value::Text(_), _) | (Op::Add, _, Value::Text(_)) => {
                    let mut s = text(l);
                    let r = text(r);
                    if s.len() + r.len() > MAX_TEXT {
                        return Evaluation::bad("text output limit");
                    }
                    s.push_str(&r);
                    Evaluation::text(s, q)
                }
                (op, Value::Number(l), Value::Number(r)) => match op {
                    Op::Lt => boolean(l < r),
                    Op::Le => boolean(l <= r),
                    Op::Gt => boolean(l > r),
                    Op::Ge => boolean(l >= r),
                    Op::Add => Evaluation::number(l + r, q),
                    Op::Sub => Evaluation::number(l - r, q),
                    Op::Mul => Evaluation::number(l * r, q),
                    Op::Div | Op::Rem if *r == 0.0 => Evaluation::bad("division by zero"),
                    Op::Div => Evaluation::number(l / r, q),
                    Op::Rem => Evaluation::number(l % r, q),
                    _ => Evaluation::bad("invalid operands"),
                },
                _ => Evaluation::bad("invalid operands"),
            }
        }
        Kind::Call(n, a) => {
            if n == "isGood" {
                let a = run(&a[0]);
                return Evaluation::good(
                    Value::Bool(a.value.is_some() && a.quality == Quality::Good),
                    Quality::Good,
                );
            }
            if n == "ifBad" {
                let v = run(&a[0]);
                return if v.value.is_some() && v.quality != Quality::Bad {
                    v
                } else {
                    run(&a[1])
                };
            }
            let args: Vec<_> = a.iter().map(run).collect();
            if let Some(bad) = args.iter().find(|x| x.value.is_none()) {
                return bad.clone();
            }
            let q = args
                .iter()
                .map(|x| x.quality)
                .max()
                .unwrap_or(Quality::Good);
            if n == "text" {
                return match &args[0].value {
                    Some(v) => Evaluation::text(text(v), q),
                    None => Evaluation::bad("missing argument"),
                };
            }
            let numbers: Option<Vec<_>> = args
                .iter()
                .map(|x| match x.value {
                    Some(Value::Number(n)) => Some(n),
                    _ => None,
                })
                .collect();
            let Some(v) = numbers else {
                return Evaluation::bad("numeric arguments required");
            };
            let result = match (n.as_str(), v.as_slice()) {
                ("abs", [x]) => x.abs(),
                ("floor", [x]) => x.floor(),
                ("ceil", [x]) => x.ceil(),
                ("min", [a, b]) => {
                    if *a == 0.0 && *b == 0.0 {
                        if a.is_sign_negative() || b.is_sign_negative() {
                            -0.0
                        } else {
                            0.0
                        }
                    } else {
                        a.min(*b)
                    }
                }
                ("max", [a, b]) => {
                    if *a == 0.0 && *b == 0.0 {
                        if a.is_sign_positive() || b.is_sign_positive() {
                            0.0
                        } else {
                            -0.0
                        }
                    } else {
                        a.max(*b)
                    }
                }
                ("round", [x]) => legacy_round(*x),
                ("round", [x, d]) if d.fract() == 0.0 && (0.0..=9.0).contains(d) => {
                    let f = 10_f64.powi(*d as i32);
                    legacy_round(x * f) / f
                }
                ("round", _) => return Evaluation::bad("invalid rounding digits"),
                ("clamp", [x, lo, hi]) if lo <= hi => x.clamp(*lo, *hi),
                ("clamp", _) => return Evaluation::bad("invalid clamp range"),
                ("scale", [x, lo, hi, a, b]) if lo != hi => a + (x - lo) * (b - a) / (hi - lo),
                ("scale", _) => return Evaluation::bad("empty scale range"),
                _ => return Evaluation::bad("invalid function"),
            };
            Evaluation::number(result, q)
        }
    }
}

impl Reading {
    /// Convert a canonical scalar snapshot to contract-1 values. Contract 1
    /// is f64-based; refuse integer readings that would lose precision.
    /// BAD or missing readings never expose stale values.
    pub fn from_canonical_scalar(
        value: Option<&serde_json::Value>,
        ty: &crate::model::DataType,
        quality: Quality,
    ) -> Result<Self, &'static str> {
        use crate::model::DataType as D;
        let Some(value) = value.filter(|_| quality != Quality::Bad) else {
            return Ok(Self {
                value: None,
                quality: Quality::Bad,
            });
        };
        let result = match ty {
            D::Bool => Value::Bool(value.as_bool().ok_or("expected boolean")?),
            D::String => Value::Text(value.as_str().ok_or("expected text")?.to_owned()),
            D::Float32 | D::Float64 => {
                let n = value.as_f64().ok_or("expected number")?;
                if value.as_i64().is_some_and(|v| n as i128 != i128::from(v))
                    || value.as_u64().is_some_and(|v| n as i128 != i128::from(v))
                {
                    return Err("integer would lose precision");
                }
                let n = if *ty == D::Float32 {
                    f64::from(n as f32)
                } else {
                    n
                };
                if !n.is_finite() {
                    return Err("non-finite scalar");
                }
                Value::Number(n)
            }
            d if d.is_integer() => {
                let v = value
                    .as_i64()
                    .map(i128::from)
                    .or_else(|| value.as_u64().map(i128::from))
                    .ok_or("expected exact integer")?;
                let (lo, hi) = match d {
                    D::Int8 => (i8::MIN.into(), i8::MAX.into()),
                    D::Int16 => (i16::MIN.into(), i16::MAX.into()),
                    D::Int32 => (i32::MIN.into(), i32::MAX.into()),
                    D::Int64 => (i64::MIN.into(), i64::MAX.into()),
                    D::UInt8 => (0, u8::MAX.into()),
                    D::UInt16 => (0, u16::MAX.into()),
                    D::UInt32 => (0, u32::MAX.into()),
                    D::UInt64 => (0, u64::MAX.into()),
                    _ => return Err("unsupported integer"),
                };
                if !(lo..=hi).contains(&v) {
                    return Err("integer outside declared type");
                }
                let n = v as f64;
                if n as i128 != v {
                    return Err("integer would lose precision");
                }
                Value::Number(n)
            }
            _ => return Err("unsupported scalar type"),
        };
        if matches!(&result,Value::Text(s) if s.len()>MAX_TEXT) {
            return Err("text input limit");
        }
        Ok(Self {
            value: Some(result),
            quality,
        })
    }
}

fn legacy_round(x: f64) -> f64 {
    if x < 0.0 {
        -(-x + 0.5).floor()
    } else {
        (x + 0.5).floor()
    }
}
