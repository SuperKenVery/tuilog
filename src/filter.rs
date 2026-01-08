use anyhow::{anyhow, Result};
use regex::Regex;

#[derive(Debug, Clone)]
pub enum FilterExpr {
    Pattern(Regex),
    And(Box<FilterExpr>, Box<FilterExpr>),
    Or(Box<FilterExpr>, Box<FilterExpr>),
    Not(Box<FilterExpr>),
}

impl FilterExpr {
    pub fn matches(&self, text: &str) -> bool {
        match self {
            FilterExpr::Pattern(re) => re.is_match(text),
            FilterExpr::And(a, b) => a.matches(text) && b.matches(text),
            FilterExpr::Or(a, b) => a.matches(text) || b.matches(text),
            FilterExpr::Not(e) => !e.matches(text),
        }
    }

    pub fn find_all_matches(&self, text: &str) -> Vec<(usize, usize)> {
        let mut matches = Vec::new();
        self.collect_matches(text, &mut matches);
        matches.sort_by_key(|m| m.0);
        merge_overlapping(&mut matches);
        matches
    }

    fn collect_matches(&self, text: &str, matches: &mut Vec<(usize, usize)>) {
        match self {
            FilterExpr::Pattern(re) => {
                for m in re.find_iter(text) {
                    matches.push((m.start(), m.end()));
                }
            }
            FilterExpr::And(a, b) | FilterExpr::Or(a, b) => {
                a.collect_matches(text, matches);
                b.collect_matches(text, matches);
            }
            FilterExpr::Not(e) => e.collect_matches(text, matches),
        }
    }
}

fn merge_overlapping(ranges: &mut Vec<(usize, usize)>) {
    if ranges.is_empty() {
        return;
    }
    let mut write = 0;
    for read in 1..ranges.len() {
        if ranges[read].0 <= ranges[write].1 {
            ranges[write].1 = ranges[write].1.max(ranges[read].1);
        } else {
            write += 1;
            ranges[write] = ranges[read];
        }
    }
    ranges.truncate(write + 1);
}

pub fn parse_filter(input: &str) -> Result<FilterExpr> {
    let input = input.trim();
    if input.is_empty() {
        return Err(anyhow!("Empty filter expression"));
    }
    let tokens = tokenize(input)?;
    let (expr, pos) = parse_or(&tokens, 0)?;
    if pos != tokens.len() {
        return Err(anyhow!("Unexpected token at position {}", pos));
    }
    Ok(expr)
}

#[derive(Debug, Clone, PartialEq)]
enum Token {
    LParen,
    RParen,
    And,
    Or,
    Not,
    Pattern(String),
}

fn tokenize(input: &str) -> Result<Vec<Token>> {
    let mut tokens = Vec::new();
    let mut chars = input.chars().peekable();

    while let Some(&c) = chars.peek() {
        match c {
            ' ' | '\t' => {
                chars.next();
            }
            '(' => {
                tokens.push(Token::LParen);
                chars.next();
            }
            ')' => {
                tokens.push(Token::RParen);
                chars.next();
            }
            '&' => {
                chars.next();
                if chars.next() == Some('&') {
                    tokens.push(Token::And);
                } else {
                    return Err(anyhow!("Expected '&&'"));
                }
            }
            '|' => {
                chars.next();
                if chars.next() == Some('|') {
                    tokens.push(Token::Or);
                } else {
                    return Err(anyhow!("Expected '||'"));
                }
            }
            '!' => {
                tokens.push(Token::Not);
                chars.next();
            }
            '"' | '\'' => {
                let quote = c;
                chars.next();
                let mut pattern = String::new();
                loop {
                    match chars.next() {
                        Some(ch) if ch == quote => break,
                        Some('\\') => {
                            if let Some(escaped) = chars.next() {
                                pattern.push(escaped);
                            }
                        }
                        Some(ch) => pattern.push(ch),
                        None => return Err(anyhow!("Unterminated string")),
                    }
                }
                tokens.push(Token::Pattern(pattern));
            }
            _ => {
                let mut pattern = String::new();
                while let Some(&ch) = chars.peek() {
                    if ch == '(' || ch == ')' || ch == '&' || ch == '|' || ch == '!' || ch == ' ' {
                        break;
                    }
                    pattern.push(ch);
                    chars.next();
                }
                if !pattern.is_empty() {
                    tokens.push(Token::Pattern(pattern));
                }
            }
        }
    }
    Ok(tokens)
}

fn parse_or(tokens: &[Token], pos: usize) -> Result<(FilterExpr, usize)> {
    let (mut left, mut pos) = parse_and(tokens, pos)?;
    while pos < tokens.len() && tokens[pos] == Token::Or {
        let (right, new_pos) = parse_and(tokens, pos + 1)?;
        left = FilterExpr::Or(Box::new(left), Box::new(right));
        pos = new_pos;
    }
    Ok((left, pos))
}

fn parse_and(tokens: &[Token], pos: usize) -> Result<(FilterExpr, usize)> {
    let (mut left, mut pos) = parse_unary(tokens, pos)?;
    while pos < tokens.len() && tokens[pos] == Token::And {
        let (right, new_pos) = parse_unary(tokens, pos + 1)?;
        left = FilterExpr::And(Box::new(left), Box::new(right));
        pos = new_pos;
    }
    Ok((left, pos))
}

fn parse_unary(tokens: &[Token], pos: usize) -> Result<(FilterExpr, usize)> {
    if pos >= tokens.len() {
        return Err(anyhow!("Unexpected end of expression"));
    }
    if tokens[pos] == Token::Not {
        let (expr, new_pos) = parse_unary(tokens, pos + 1)?;
        return Ok((FilterExpr::Not(Box::new(expr)), new_pos));
    }
    parse_primary(tokens, pos)
}

fn parse_primary(tokens: &[Token], pos: usize) -> Result<(FilterExpr, usize)> {
    if pos >= tokens.len() {
        return Err(anyhow!("Unexpected end of expression"));
    }
    match &tokens[pos] {
        Token::LParen => {
            let (expr, new_pos) = parse_or(tokens, pos + 1)?;
            if new_pos >= tokens.len() || tokens[new_pos] != Token::RParen {
                return Err(anyhow!("Missing closing parenthesis"));
            }
            Ok((expr, new_pos + 1))
        }
        Token::Pattern(p) => {
            let re = Regex::new(p).map_err(|e| anyhow!("Invalid regex '{}': {}", p, e))?;
            Ok((FilterExpr::Pattern(re), pos + 1))
        }
        _ => Err(anyhow!("Unexpected token")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_pattern() {
        let filter = parse_filter("error").unwrap();
        assert!(filter.matches("this is an error"));
        assert!(!filter.matches("this is fine"));
    }

    #[test]
    fn test_and() {
        let filter = parse_filter("error && fatal").unwrap();
        assert!(filter.matches("fatal error occurred"));
        assert!(!filter.matches("error occurred"));
    }

    #[test]
    fn test_or() {
        let filter = parse_filter("error || warn").unwrap();
        assert!(filter.matches("error occurred"));
        assert!(filter.matches("warn: something"));
        assert!(!filter.matches("info: ok"));
    }

    #[test]
    fn test_complex() {
        let filter = parse_filter("(error || warn) && !debug").unwrap();
        assert!(filter.matches("error in production"));
        assert!(!filter.matches("debug error message"));
    }

    #[test]
    fn test_negative_pattern() {
        let filter = parse_filter("error && !debug").unwrap();
        assert!(filter.matches("error in production"));
        assert!(filter.matches("fatal error occurred"));
        assert!(!filter.matches("debug error message"));
        assert!(!filter.matches("debug: some info"));
        assert!(!filter.matches("no match here"));
    }

    #[test]
    fn test_negative_with_quoted_spaces() {
        let filter = parse_filter(r#"error && !"debug mode""#).unwrap();
        assert!(filter.matches("error in production"));
        assert!(filter.matches("error debug"));
        assert!(!filter.matches("error in debug mode"));
        assert!(!filter.matches("debug mode error"));
    }
}
