// SPDX-License-Identifier: MIT
// SPDX-FileCopyrightText: 2026 Brice LECOLE

//! Parsing and evaluation of A2L formula expressions.
//!
//! The same grammar serves two places: a `FORM` COMPU_METHOD, where `X1` is the
//! raw value being converted, and a `VIRTUAL_CHARACTERISTIC`, where `X1`, `X2`,
//! … are the physical values of the parameters it reads.
//!
//! Anything the grammar does not cover is rejected at parse time. A formula is
//! either understood completely or reported as unavailable — never guessed at,
//! since a plausible-looking wrong number is far worse here than a blank.

use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Op {
    Add,
    Sub,
    Mul,
    Div,
    Pow,
}

/// The functions ASAP2 defines for formulas.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Func {
    Sin,
    Cos,
    Tan,
    Sinh,
    Cosh,
    Tanh,
    Asin,
    Acos,
    Atan,
    Exp,
    /// Natural logarithm, which is what ASAP2 means by `log`.
    Log,
    Log10,
    Sqrt,
    Abs,
    Pow,
    Min,
    Max,
}

impl Func {
    fn from_name(name: &str) -> Option<Func> {
        Some(match name.to_ascii_lowercase().as_str() {
            "sin" => Func::Sin,
            "cos" => Func::Cos,
            "tan" => Func::Tan,
            "sinh" => Func::Sinh,
            "cosh" => Func::Cosh,
            "tanh" => Func::Tanh,
            "asin" | "arcsin" => Func::Asin,
            "acos" | "arccos" => Func::Acos,
            "atan" | "arctan" => Func::Atan,
            "exp" => Func::Exp,
            "log" | "ln" => Func::Log,
            "log10" => Func::Log10,
            "sqrt" => Func::Sqrt,
            "abs" => Func::Abs,
            "pow" => Func::Pow,
            "min" => Func::Min,
            "max" => Func::Max,
            _ => return None,
        })
    }

    fn arity(self) -> usize {
        match self {
            Func::Pow | Func::Min | Func::Max => 2,
            _ => 1,
        }
    }
}

#[derive(Debug, Clone)]
enum Expr {
    Num(f64),
    /// `X1` is index 0.
    Var(usize),
    SysConst(String),
    Neg(Box<Expr>),
    Bin(Op, Box<Expr>, Box<Expr>),
    Call(Func, Vec<Expr>),
}

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Num(f64),
    Ident(String),
    Op(char),
    LParen,
    RParen,
    Comma,
}

fn tokenize(src: &str) -> Result<Vec<Tok>, String> {
    let chars: Vec<char> = src.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() {
            i += 1;
        } else if c.is_ascii_digit() || (c == '.' && i + 1 < chars.len() && chars[i + 1].is_ascii_digit())
        {
            let start = i;
            while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.') {
                i += 1;
            }
            // Exponent form, e.g. 1.5e-3.
            if i < chars.len() && (chars[i] == 'e' || chars[i] == 'E') {
                let save = i;
                i += 1;
                if i < chars.len() && (chars[i] == '+' || chars[i] == '-') {
                    i += 1;
                }
                if i < chars.len() && chars[i].is_ascii_digit() {
                    while i < chars.len() && chars[i].is_ascii_digit() {
                        i += 1;
                    }
                } else {
                    i = save;
                }
            }
            let text: String = chars[start..i].iter().collect();
            out.push(Tok::Num(
                text.parse::<f64>().map_err(|_| format!("bad number '{text}'"))?,
            ));
        } else if c.is_alphabetic() || c == '_' {
            let start = i;
            while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_' || chars[i] == '.')
            {
                i += 1;
            }
            out.push(Tok::Ident(chars[start..i].iter().collect()));
        } else {
            i += 1;
            match c {
                '(' => out.push(Tok::LParen),
                ')' => out.push(Tok::RParen),
                ',' => out.push(Tok::Comma),
                '+' | '-' | '*' | '/' | '^' => out.push(Tok::Op(c)),
                _ => return Err(format!("unexpected character '{c}'")),
            }
        }
    }
    Ok(out)
}

struct Parser {
    toks: Vec<Tok>,
    pos: usize,
}

impl Parser {
    fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.pos)
    }

    fn eat_op(&mut self, ops: &[char]) -> Option<char> {
        if let Some(Tok::Op(c)) = self.peek() {
            let c = *c;
            if ops.contains(&c) {
                self.pos += 1;
                return Some(c);
            }
        }
        None
    }

    /// expr := term (('+' | '-') term)*
    fn expr(&mut self) -> Result<Expr, String> {
        let mut lhs = self.term()?;
        while let Some(c) = self.eat_op(&['+', '-']) {
            let rhs = self.term()?;
            let op = if c == '+' { Op::Add } else { Op::Sub };
            lhs = Expr::Bin(op, Box::new(lhs), Box::new(rhs));
        }
        Ok(lhs)
    }

    /// term := unary (('*' | '/') unary)*
    fn term(&mut self) -> Result<Expr, String> {
        let mut lhs = self.unary()?;
        while let Some(c) = self.eat_op(&['*', '/']) {
            let rhs = self.unary()?;
            let op = if c == '*' { Op::Mul } else { Op::Div };
            lhs = Expr::Bin(op, Box::new(lhs), Box::new(rhs));
        }
        Ok(lhs)
    }

    /// unary := ('-' | '+') unary | power
    fn unary(&mut self) -> Result<Expr, String> {
        if let Some(c) = self.eat_op(&['-', '+']) {
            let inner = self.unary()?;
            return Ok(if c == '-' {
                Expr::Neg(Box::new(inner))
            } else {
                inner
            });
        }
        self.power()
    }

    /// power := atom ('^' unary)?  — right-associative, and the exponent may
    /// itself be signed, as in `2^-1`.
    fn power(&mut self) -> Result<Expr, String> {
        let base = self.atom()?;
        if self.eat_op(&['^']).is_some() {
            let exp = self.unary()?;
            return Ok(Expr::Bin(Op::Pow, Box::new(base), Box::new(exp)));
        }
        Ok(base)
    }

    fn atom(&mut self) -> Result<Expr, String> {
        match self.peek().cloned() {
            Some(Tok::Num(v)) => {
                self.pos += 1;
                Ok(Expr::Num(v))
            }
            Some(Tok::LParen) => {
                self.pos += 1;
                let inner = self.expr()?;
                match self.peek() {
                    Some(Tok::RParen) => {
                        self.pos += 1;
                        Ok(inner)
                    }
                    _ => Err("expected ')'".to_string()),
                }
            }
            Some(Tok::Ident(name)) => {
                self.pos += 1;
                self.ident(name)
            }
            other => Err(format!("unexpected {other:?}")),
        }
    }

    fn ident(&mut self, name: String) -> Result<Expr, String> {
        // sysc(NAME) takes a bare identifier, not an expression.
        if name.eq_ignore_ascii_case("sysc") {
            if self.peek() != Some(&Tok::LParen) {
                return Err("sysc must be called as sysc(NAME)".to_string());
            }
            self.pos += 1;
            let Some(Tok::Ident(cname)) = self.peek().cloned() else {
                return Err("sysc expects a system constant name".to_string());
            };
            self.pos += 1;
            if self.peek() != Some(&Tok::RParen) {
                return Err("expected ')' after the system constant name".to_string());
            }
            self.pos += 1;
            return Ok(Expr::SysConst(cname));
        }

        // A call, if a parenthesis follows.
        if self.peek() == Some(&Tok::LParen) {
            let Some(f) = Func::from_name(&name) else {
                return Err(format!("unknown function '{name}'"));
            };
            self.pos += 1;
            let mut args = vec![self.expr()?];
            while self.peek() == Some(&Tok::Comma) {
                self.pos += 1;
                args.push(self.expr()?);
            }
            if self.peek() != Some(&Tok::RParen) {
                return Err(format!("expected ')' closing '{name}'"));
            }
            self.pos += 1;
            if args.len() != f.arity() {
                return Err(format!(
                    "'{name}' takes {} argument(s), got {}",
                    f.arity(),
                    args.len()
                ));
            }
            return Ok(Expr::Call(f, args));
        }

        // Otherwise a variable: X1, X2, …
        if let Some(rest) = name.strip_prefix(['X', 'x']) {
            if let Ok(n) = rest.parse::<usize>() {
                if n >= 1 {
                    return Ok(Expr::Var(n - 1));
                }
            }
        }
        Err(format!("unknown identifier '{name}'"))
    }
}

/// Values a formula may refer to.
pub struct Context<'a> {
    /// `X1` is `vars[0]`.
    pub vars: &'a [f64],
    pub constants: &'a HashMap<String, f64>,
}

/// A parsed A2L formula, ready to evaluate.
#[derive(Debug, Clone)]
pub struct Formula {
    root: Expr,
    source: String,
}

impl Formula {
    pub fn parse(src: &str) -> Result<Formula, String> {
        let toks = tokenize(src)?;
        if toks.is_empty() {
            return Err("empty formula".to_string());
        }
        let mut p = Parser { toks, pos: 0 };
        let root = p.expr()?;
        if p.pos != p.toks.len() {
            return Err(format!("trailing input at token {}", p.pos));
        }
        Ok(Formula {
            root,
            source: src.to_string(),
        })
    }

    pub fn source(&self) -> &str {
        &self.source
    }

    /// Highest variable index referenced, so a caller knows how many inputs a
    /// formula actually needs.
    pub fn max_var(&self) -> Option<usize> {
        fn walk(e: &Expr, best: &mut Option<usize>) {
            match e {
                Expr::Var(i) => *best = Some(best.map_or(*i, |b: usize| b.max(*i))),
                Expr::Neg(a) => walk(a, best),
                Expr::Bin(_, a, b) => {
                    walk(a, best);
                    walk(b, best);
                }
                Expr::Call(_, args) => args.iter().for_each(|a| walk(a, best)),
                _ => {}
            }
        }
        let mut best = None;
        walk(&self.root, &mut best);
        best
    }

    pub fn eval(&self, ctx: &Context) -> Result<f64, String> {
        eval(&self.root, ctx)
    }
}

fn eval(e: &Expr, ctx: &Context) -> Result<f64, String> {
    Ok(match e {
        Expr::Num(v) => *v,
        Expr::Var(i) => *ctx
            .vars
            .get(*i)
            .ok_or_else(|| format!("X{} has no value", i + 1))?,
        Expr::SysConst(name) => *ctx
            .constants
            .get(name)
            .ok_or_else(|| format!("system constant '{name}' is not a number"))?,
        Expr::Neg(a) => -eval(a, ctx)?,
        Expr::Bin(op, a, b) => {
            let (x, y) = (eval(a, ctx)?, eval(b, ctx)?);
            match op {
                Op::Add => x + y,
                Op::Sub => x - y,
                Op::Mul => x * y,
                Op::Div => {
                    if y == 0.0 {
                        return Err("division by zero".to_string());
                    }
                    x / y
                }
                Op::Pow => x.powf(y),
            }
        }
        Expr::Call(f, args) => {
            let a = eval(&args[0], ctx)?;
            match f {
                Func::Sin => a.sin(),
                Func::Cos => a.cos(),
                Func::Tan => a.tan(),
                Func::Sinh => a.sinh(),
                Func::Cosh => a.cosh(),
                Func::Tanh => a.tanh(),
                Func::Asin => a.asin(),
                Func::Acos => a.acos(),
                Func::Atan => a.atan(),
                Func::Exp => a.exp(),
                Func::Log => a.ln(),
                Func::Log10 => a.log10(),
                Func::Sqrt => a.sqrt(),
                Func::Abs => a.abs(),
                Func::Pow => a.powf(eval(&args[1], ctx)?),
                Func::Min => a.min(eval(&args[1], ctx)?),
                Func::Max => a.max(eval(&args[1], ctx)?),
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(src: &str, vars: &[f64]) -> f64 {
        let constants = HashMap::new();
        Formula::parse(src)
            .unwrap_or_else(|e| panic!("parse {src:?}: {e}"))
            .eval(&Context { vars, constants: &constants })
            .unwrap_or_else(|e| panic!("eval {src:?}: {e}"))
    }

    /// Every formula the ASAM demo file actually contains.
    #[test]
    fn evaluates_the_demo_files_formulas() {
        assert_eq!(ev("X1+4", &[10.0]), 14.0);
        assert_eq!(ev("X1-4", &[10.0]), 6.0);
        assert_eq!(ev("4*X1", &[10.0]), 40.0);
        assert_eq!(ev("X1 - 9", &[6.0]), -3.0);
        assert_eq!(ev("X1 + 19", &[20.0]), 39.0);
        assert_eq!(ev("X1 + X2", &[11.0, 45.0]), 56.0);
        assert_eq!(ev("X1 * 2", &[16.55]), 33.1);
        assert_eq!(ev("X1", &[7.0]), 7.0);
    }

    #[test]
    fn respects_operator_precedence() {
        assert_eq!(ev("2+3*4", &[]), 14.0);
        assert_eq!(ev("(2+3)*4", &[]), 20.0);
        assert_eq!(ev("10-2-3", &[]), 5.0, "subtraction is left-associative");
        assert_eq!(ev("100/10/2", &[]), 5.0, "division is left-associative");
    }

    #[test]
    fn power_is_right_associative() {
        assert_eq!(ev("2^3^2", &[]), 512.0, "2^(3^2), not (2^3)^2");
        assert_eq!(ev("2^-1", &[]), 0.5, "a signed exponent parses");
        assert_eq!(ev("-2^2", &[]), -4.0, "the power binds tighter than unary minus");
    }

    #[test]
    fn handles_unary_and_nested_signs() {
        assert_eq!(ev("-X1", &[5.0]), -5.0);
        assert_eq!(ev("--5", &[]), 5.0);
        assert_eq!(ev("3 - -2", &[]), 5.0);
    }

    #[test]
    fn parses_number_forms() {
        assert_eq!(ev("1.5", &[]), 1.5);
        assert_eq!(ev(".5", &[]), 0.5);
        assert_eq!(ev("1.5e2", &[]), 150.0);
        assert_eq!(ev("1.5E-2", &[]), 0.015);
    }

    #[test]
    fn resolves_system_constants() {
        let mut constants = HashMap::new();
        constants.insert("System_Constant_1".to_string(), -3.45);
        let f = Formula::parse("X1 + sysc(System_Constant_1)").expect("parse");
        let v = f
            .eval(&Context { vars: &[20.0], constants: &constants })
            .expect("eval");
        assert!((v - 16.55).abs() < 1e-12, "got {v}");
    }

    #[test]
    fn a_missing_system_constant_is_an_error_not_a_zero() {
        let constants = HashMap::new();
        let f = Formula::parse("sysc(Nope)").expect("parse");
        assert!(f.eval(&Context { vars: &[], constants: &constants }).is_err());
    }

    #[test]
    fn evaluates_functions() {
        assert_eq!(ev("abs(-3)", &[]), 3.0);
        assert_eq!(ev("sqrt(16)", &[]), 4.0);
        assert_eq!(ev("pow(2,10)", &[]), 1024.0);
        assert_eq!(ev("min(3,7)", &[]), 3.0);
        assert_eq!(ev("max(3,7)", &[]), 7.0);
        assert!((ev("exp(0)", &[]) - 1.0).abs() < 1e-12);
        assert!((ev("log(1)", &[])).abs() < 1e-12, "log is the natural log");
        assert!((ev("log10(100)", &[]) - 2.0).abs() < 1e-12);
    }

    #[test]
    fn max_var_reports_how_many_inputs_are_needed() {
        assert_eq!(Formula::parse("X1 + X2").unwrap().max_var(), Some(1));
        assert_eq!(Formula::parse("X1").unwrap().max_var(), Some(0));
        assert_eq!(Formula::parse("42").unwrap().max_var(), None);
        assert_eq!(Formula::parse("X3 * 2").unwrap().max_var(), Some(2));
    }

    /// Anything not understood must fail loudly at parse time, so a formula is
    /// never half-evaluated into a plausible but wrong number.
    #[test]
    fn rejects_what_it_does_not_understand() {
        for bad in [
            "X1 +",          // dangling operator
            "X1 $ 2",        // unknown character
            "frobnicate(1)", // unknown function
            "pow(2)",        // wrong arity
            "(1 + 2",        // unclosed paren
            "1 + 2)",        // trailing input
            "",              // empty
            "Y1",            // unknown identifier
            "X0",            // variables are 1-based
            "sysc()",        // missing constant name
        ] {
            assert!(
                Formula::parse(bad).is_err(),
                "{bad:?} should not have parsed"
            );
        }
    }

    #[test]
    fn missing_variable_and_division_by_zero_are_errors() {
        let constants = HashMap::new();
        let f = Formula::parse("X2").expect("parse");
        assert!(f.eval(&Context { vars: &[1.0], constants: &constants }).is_err());

        let f = Formula::parse("1/0").expect("parse");
        assert!(f.eval(&Context { vars: &[], constants: &constants }).is_err());
    }
}
