/// Safe arithmetic evaluator. Recursive descent, no eval, no injection.
/// Supports + - * / % ^ parentheses, unary minus, sqrt sin cos tan asin acos
/// atan ln log abs exp, constants pi and e.
#[derive(Debug, Clone)]
enum Tok {
    Num(f64),
    Op(char),
    LParen,
    RParen,
    Name(String),
}

fn tokenize(s: &str) -> Result<Vec<Tok>, String> {
    let chars: Vec<char> = s.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() || c == ',' {
            i += 1;
        } else if c.is_ascii_digit() || c == '.' {
            let mut j = i;
            while j < chars.len() && (chars[j].is_ascii_digit() || chars[j] == '.') {
                j += 1;
            }
            let num: String = chars[i..j].iter().collect();
            match num.parse::<f64>() {
                Ok(v) => out.push(Tok::Num(v)),
                Err(_) => return Err(format!("bad number {num}")),
            }
            i = j;
        } else if "+-*/%^".contains(c) {
            out.push(Tok::Op(c));
            i += 1;
        } else if c == '(' {
            out.push(Tok::LParen);
            i += 1;
        } else if c == ')' {
            out.push(Tok::RParen);
            i += 1;
        } else if c.is_ascii_alphabetic() {
            let mut j = i;
            while j < chars.len() && chars[j].is_ascii_alphanumeric() {
                j += 1;
            }
            out.push(Tok::Name(
                chars[i..j].iter().collect::<String>().to_lowercase(),
            ));
            i = j;
        } else {
            return Err(format!("unexpected char {c}"));
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

    fn expr(&mut self) -> Result<f64, String> {
        let mut v = self.term()?;
        loop {
            match self.peek() {
                Some(Tok::Op('+')) => {
                    self.pos += 1;
                    v += self.term()?;
                }
                Some(Tok::Op('-')) => {
                    self.pos += 1;
                    v -= self.term()?;
                }
                _ => break,
            }
        }
        Ok(v)
    }

    fn term(&mut self) -> Result<f64, String> {
        let mut v = self.factor()?;
        loop {
            match self.peek() {
                Some(Tok::Op('*')) => {
                    self.pos += 1;
                    v *= self.factor()?;
                }
                Some(Tok::Op('/')) => {
                    self.pos += 1;
                    let d = self.factor()?;
                    if d == 0.0 {
                        return Err("division by zero".to_string());
                    }
                    v /= d;
                }
                Some(Tok::Op('%')) => {
                    self.pos += 1;
                    let d = self.factor()?;
                    if d == 0.0 {
                        return Err("modulo by zero".to_string());
                    }
                    v %= d;
                }
                _ => break,
            }
        }
        Ok(v)
    }

    fn factor(&mut self) -> Result<f64, String> {
        let base = self.unary()?;
        if matches!(self.peek(), Some(Tok::Op('^'))) {
            self.pos += 1;
            let exp = self.factor()?;
            return Ok(base.powf(exp));
        }
        Ok(base)
    }

    fn unary(&mut self) -> Result<f64, String> {
        if matches!(self.peek(), Some(Tok::Op('-'))) {
            self.pos += 1;
            return Ok(-self.unary()?);
        }
        if matches!(self.peek(), Some(Tok::Op('+'))) {
            self.pos += 1;
            return self.unary();
        }
        self.primary()
    }

    fn primary(&mut self) -> Result<f64, String> {
        match self.peek().cloned() {
            Some(Tok::Num(v)) => {
                self.pos += 1;
                Ok(v)
            }
            Some(Tok::LParen) => {
                self.pos += 1;
                let v = self.expr()?;
                match self.peek() {
                    Some(Tok::RParen) => {
                        self.pos += 1;
                        Ok(v)
                    }
                    _ => Err("missing closing parenthesis".to_string()),
                }
            }
            Some(Tok::Name(n)) => {
                self.pos += 1;
                match n.as_str() {
                    "pi" => Ok(std::f64::consts::PI),
                    "e" => Ok(std::f64::consts::E),
                    _ => {
                        // Function call with or without parentheses.
                        let arg = if matches!(self.peek(), Some(Tok::LParen)) {
                            self.pos += 1;
                            let v = self.expr()?;
                            match self.peek() {
                                Some(Tok::RParen) => {
                                    self.pos += 1;
                                    v
                                }
                                _ => return Err("missing closing parenthesis".to_string()),
                            }
                        } else {
                            self.unary()?
                        };
                        apply_func(&n, arg)
                    }
                }
            }
            _ => Err("expected a number or ( expression )".to_string()),
        }
    }
}

fn apply_func(name: &str, x: f64) -> Result<f64, String> {
    match name {
        "sqrt" => {
            if x < 0.0 {
                return Err("sqrt of negative number".to_string());
            }
            Ok(x.sqrt())
        }
        "sin" => Ok(x.sin()),
        "cos" => Ok(x.cos()),
        "tan" => Ok(x.tan()),
        "asin" => Ok(x.asin()),
        "acos" => Ok(x.acos()),
        "atan" => Ok(x.atan()),
        "ln" => {
            if x <= 0.0 {
                return Err("ln of non positive number".to_string());
            }
            Ok(x.ln())
        }
        "log" => {
            if x <= 0.0 {
                return Err("log of non positive number".to_string());
            }
            Ok(x.log10())
        }
        "abs" => Ok(x.abs()),
        "exp" => Ok(x.exp()),
        _ => Err(format!("unknown function or constant {name}")),
    }
}

/// Evaluate a full expression string. The whole string must parse.
pub fn eval(expr: &str) -> Result<f64, String> {
    let toks = tokenize(expr)?;
    if toks.is_empty() {
        return Err("empty expression".to_string());
    }
    let mut p = Parser { toks, pos: 0 };
    let v = p.expr()?;
    if p.pos != p.toks.len() {
        return Err("trailing input after expression".to_string());
    }
    if !v.is_finite() {
        return Err("result is not finite".to_string());
    }
    Ok(v)
}

pub fn format_num(v: f64) -> String {
    if v.fract() == 0.0 && v.abs() < 1e15 {
        return format!("{}", v as i64);
    }
    let s = format!("{v:.6}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

fn is_date_or_version(s: &str) -> bool {
    let t = s.trim();
    // YYYY-MM-DD dates and x.y.z versions must not parse as math.
    let parts: Vec<&str> = t.split(['-', '.', '/']).collect();
    if parts.len() == 3
        && parts
            .iter()
            .all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()))
    {
        return true;
    }
    false
}

/// Extract a math expression from a natural query.
/// Returns the expression when the whole trimmed remainder parses as math.
pub fn extract_expr(query: &str) -> Option<String> {
    let mut q = query.trim().to_string();
    // Strip trigger words in Indonesian and English.
    for prefix in [
        "hitung",
        "berapa",
        "kalkulasi",
        "calc",
        "calculate",
        "compute",
        "eval",
        "math",
    ] {
        if let Some(rest) = q.strip_prefix(prefix) {
            let rest = rest.trim_start_matches([':', '=', ' ']).trim();
            if !rest.is_empty() {
                q = rest.to_string();
                break;
            }
        }
    }
    let q = q.trim_end_matches(['?', '=']).trim();
    if q.len() < 3 || is_date_or_version(q) {
        return None;
    }
    // Must contain at least one operator or function call, plus only math vocabulary.
    if !q.chars().any(|c| "+-*/%^()".contains(c)) {
        return None;
    }
    let allowed = q
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || "+-*/%^()., \t".contains(c));
    if !allowed {
        return None;
    }
    // Whole string must parse and must not be a bare word like "a-b".
    if q.chars().any(|c| c.is_ascii_digit()) && eval(q).is_ok() {
        return Some(q.to_string());
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arithmetic_precedence() {
        assert_eq!(eval("2+3*4").unwrap(), 14.0);
        assert_eq!(eval("(2+3)*4").unwrap(), 20.0);
        assert_eq!(eval("2^3^2").unwrap(), 512.0);
        assert_eq!(eval("-3+10/2").unwrap(), 2.0);
        assert_eq!(eval("10%3").unwrap(), 1.0);
    }

    #[test]
    fn functions_and_constants() {
        assert!((eval("sqrt(16)").unwrap() - 4.0).abs() < 1e-9);
        assert!((eval("sin(0)+cos(0)").unwrap() - 1.0).abs() < 1e-9);
        assert!((eval("2*pi").unwrap() - std::f64::consts::TAU).abs() < 1e-9);
    }

    #[test]
    fn errors() {
        assert!(eval("1/0").is_err());
        assert!(eval("2+").is_err());
        assert!(eval("foo(1)").is_err());
        assert!(eval("").is_err());
    }

    #[test]
    fn extraction_guards() {
        assert_eq!(extract_expr("hitung 2+3*4").as_deref(), Some("2+3*4"));
        assert_eq!(
            extract_expr("berapa (10-2)/4?").as_deref(),
            Some("(10-2)/4")
        );
        assert_eq!(extract_expr("2026-10-06"), None);
        assert_eq!(extract_expr("1.1.0"), None);
        assert_eq!(extract_expr("hello world"), None);
        assert_eq!(extract_expr("a-b"), None);
    }
}
