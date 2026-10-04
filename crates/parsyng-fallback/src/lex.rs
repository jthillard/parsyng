//! A Rust lexer producing [`TokenStream`]s, following rustc's tokenization
//! (and `proc_macro2`'s, for doc comments and punctuation spacing).

use crate::{
    Delimiter, Group, Ident, LexError, Literal, PUNCT_CHARS, Punct, Spacing, Span, TokenStream,
    TokenTree, is_ident_continue, is_ident_start,
};

/// Lex a whole token stream.
pub fn lex(src: &str) -> Result<TokenStream, LexError> {
    let mut lexer = Lexer { src, pos: 0 };
    // The trees of each open group, outermost first, with their delimiter
    // and the position of their opening delimiter.
    let mut open: Vec<(Delimiter, usize, Vec<TokenTree>)> = Vec::new();
    let mut trees = Vec::new();
    loop {
        lexer.skip_trivia(&mut trees)?;
        let start = lexer.pos;
        let Some(ch) = lexer.peek() else {
            if let Some((_, start, _)) = open.last() {
                return Err(error(*start..*start + 1));
            }
            return Ok(TokenStream::from_vec(trees));
        };
        let delimiter = match ch {
            '(' | ')' => Some(Delimiter::Parenthesis),
            '[' | ']' => Some(Delimiter::Bracket),
            '{' | '}' => Some(Delimiter::Brace),
            _ => None,
        };
        if let Some(delimiter) = delimiter {
            lexer.pos += 1;
            if matches!(ch, '(' | '[' | '{') {
                open.push((delimiter, start, core::mem::take(&mut trees)));
            } else {
                let Some((open_delimiter, open_start, parent)) = open.pop() else {
                    return Err(error(start..lexer.pos));
                };
                if open_delimiter != delimiter {
                    return Err(error(start..lexer.pos));
                }
                let inner = core::mem::replace(&mut trees, parent);
                let mut group = Group::new(delimiter, TokenStream::from_vec(inner));
                group.set_span(Span::new(open_start..lexer.pos));
                trees.push(group.into());
            }
            continue;
        }
        lexer.token(&mut trees)?;
    }
}

/// Lex a single literal, optionally preceded by `-`.
pub fn lex_literal(src: &str) -> Result<Literal, LexError> {
    let mut lexer = Lexer { src, pos: 0 };
    let mut trees = Vec::new();
    lexer.skip_trivia(&mut trees)?;
    let start = lexer.pos;
    let negative = lexer.eat('-');
    lexer.skip_trivia(&mut trees)?;
    let literal_start = lexer.pos;
    if !trees.is_empty() || !lexer.literal(&mut trees)? {
        return Err(error(start..src.len()));
    }
    let end = lexer.pos;
    lexer.skip_trivia(&mut trees)?;
    match trees.as_slice() {
        [TokenTree::Literal(_)] if lexer.pos == src.len() => {}
        _ => return Err(error(start..src.len())),
    }
    let repr = if negative {
        format!("-{}", &src[literal_start..end])
    } else {
        src[literal_start..end].to_owned()
    };
    Ok(Literal::new_lexed(&repr, Span::new(start..end)))
}

fn error(range: core::ops::Range<usize>) -> LexError {
    LexError {
        span: Span::new(range),
    }
}

struct Lexer<'a> {
    src: &'a str,
    pos: usize,
}

/// Rust's `Pattern_White_Space`.
const fn is_whitespace(ch: char) -> bool {
    matches!(
        ch,
        ' ' | '\t'
            | '\n'
            | '\r'
            | '\x0B'
            | '\x0C'
            | '\u{85}'
            | '\u{200E}'
            | '\u{200F}'
            | '\u{2028}'
            | '\u{2029}'
    )
}

impl<'a> Lexer<'a> {
    fn rest(&self) -> &'a str {
        &self.src[self.pos..]
    }

    fn peek(&self) -> Option<char> {
        self.rest().chars().next()
    }

    fn peek_nth(&self, n: usize) -> Option<char> {
        self.rest().chars().nth(n)
    }

    fn eat(&mut self, ch: char) -> bool {
        let found = self.peek() == Some(ch);
        if found {
            self.pos += ch.len_utf8();
        }
        found
    }

    fn bump(&mut self) -> Option<char> {
        let ch = self.peek()?;
        self.pos += ch.len_utf8();
        Some(ch)
    }

    /// Skip whitespace and comments, turning doc comments into
    /// `#[doc = "..."]` attributes appended to `trees`.
    fn skip_trivia(&mut self, trees: &mut Vec<TokenTree>) -> Result<(), LexError> {
        loop {
            let start = self.pos;
            let rest = self.rest();
            if let Some(ch) = rest.chars().next()
                && is_whitespace(ch)
            {
                self.pos += ch.len_utf8();
            } else if rest.starts_with("//") {
                let end = rest.find('\n').unwrap_or(rest.len());
                self.pos += end;
                let line = rest[..end].strip_suffix('\r').unwrap_or(&rest[..end]);
                if let Some(text) = line.strip_prefix("//!") {
                    doc_comment(trees, text, true, Span::new(start..self.pos));
                } else if let Some(text) = line.strip_prefix("///")
                    && !text.starts_with('/')
                {
                    doc_comment(trees, text, false, Span::new(start..self.pos));
                }
            } else if rest.starts_with("/*") {
                let len = block_comment_len(rest).ok_or_else(|| error(start..self.src.len()))?;
                self.pos += len;
                let comment = &rest[..len];
                let body = &comment[3.min(len - 2)..len - 2];
                if comment.starts_with("/*!") {
                    doc_comment(trees, body, true, Span::new(start..self.pos));
                } else if comment.starts_with("/**") && !comment.starts_with("/***") && len > 4 {
                    doc_comment(trees, body, false, Span::new(start..self.pos));
                }
            } else {
                return Ok(());
            }
        }
    }

    /// Lex one non-delimiter token into `trees`.
    fn token(&mut self, trees: &mut Vec<TokenTree>) -> Result<(), LexError> {
        let start = self.pos;
        if self.literal(trees)? {
            return Ok(());
        }
        let ch = self.peek().expect("not at the end");
        if ch == '\'' {
            // Not a character literal: a lifetime or label.
            self.pos += 1;
            let raw = self.rest().starts_with("r#") && self.peek_nth(2).is_some_and(is_ident_start);
            if raw {
                self.pos += 2;
            }
            let ident_start = self.pos;
            if !self.ident_chars() || self.peek() == Some('\'') {
                return Err(error(start..self.pos));
            }
            let mut quote = Punct::new('\'', Spacing::Joint);
            quote.set_span(Span::new(start..start + 1));
            trees.push(quote.into());
            let span = Span::new(ident_start..self.pos);
            trees.push(Ident::new_lexed(&self.src[ident_start..self.pos], raw, span).into());
            return Ok(());
        }
        if ch == 'r'
            && self.rest().starts_with("r#")
            && self.peek_nth(2).is_some_and(is_ident_start)
        {
            self.pos += 2;
            let ident_start = self.pos;
            self.ident_chars();
            let span = Span::new(start..self.pos);
            trees.push(Ident::new_lexed(&self.src[ident_start..self.pos], true, span).into());
            return Ok(());
        }
        if is_ident_start(ch) {
            self.ident_chars();
            let span = Span::new(start..self.pos);
            trees.push(Ident::new_lexed(&self.src[start..self.pos], false, span).into());
            return Ok(());
        }
        if PUNCT_CHARS.contains(ch) {
            self.pos += 1;
            let rest = self.rest();
            let joint = !rest.starts_with("//")
                && !rest.starts_with("/*")
                && rest
                    .chars()
                    .next()
                    .is_some_and(|next| PUNCT_CHARS.contains(next));
            let mut punct = Punct::new(
                ch,
                if joint {
                    Spacing::Joint
                } else {
                    Spacing::Alone
                },
            );
            punct.set_span(Span::new(start..self.pos));
            trees.push(punct.into());
            return Ok(());
        }
        Err(error(start..start + ch.len_utf8()))
    }

    /// Consume identifier characters; `false` if there is none.
    fn ident_chars(&mut self) -> bool {
        if !self.peek().is_some_and(is_ident_start) {
            return false;
        }
        while let Some(ch) = self.peek()
            && is_ident_continue(ch)
        {
            self.pos += ch.len_utf8();
        }
        true
    }

    /// Lex a literal into `trees` if one starts here.
    fn literal(&mut self, trees: &mut Vec<TokenTree>) -> Result<bool, LexError> {
        let start = self.pos;
        let rest = self.rest();
        let lexed = if rest.starts_with('"') {
            self.pos += 1;
            self.quoted('"')
        } else if rest.starts_with("b\"") || rest.starts_with("c\"") {
            self.pos += 2;
            self.quoted('"')
        } else if rest.starts_with("b'") {
            self.pos += 2;
            self.quoted('\'')
        } else if rest.starts_with("r\"") || rest.starts_with("r#\"") || rest.starts_with("r##") {
            self.pos += 1;
            self.raw_string()
        } else if ["br\"", "br#", "cr\"", "cr#"]
            .iter()
            .any(|prefix| rest.starts_with(prefix))
        {
            self.pos += 2;
            self.raw_string()
        } else if rest.starts_with('\'') && self.is_char_literal() {
            self.pos += 1;
            self.quoted('\'')
        } else if rest.starts_with(|ch: char| ch.is_ascii_digit()) {
            self.number();
            true
        } else {
            return Ok(false);
        };
        if !lexed {
            return Err(error(start..self.src.len()));
        }
        // A suffix, e.g. `1u8` or `"a"suffix`.
        self.ident_chars();
        let span = Span::new(start..self.pos);
        trees.push(Literal::new_lexed(&self.src[start..self.pos], span).into());
        Ok(true)
    }

    /// Whether the `'` here starts a character literal rather than a
    /// lifetime: `'\...'`, or a single character followed by `'`.
    fn is_char_literal(&self) -> bool {
        match self.peek_nth(1) {
            Some('\\') => true,
            Some(_) => self.peek_nth(2) == Some('\''),
            None => false,
        }
    }

    /// Consume the rest of a string or character literal up to its closing
    /// `quote` (escapes included); `false` if it is unterminated.
    fn quoted(&mut self, quote: char) -> bool {
        while let Some(ch) = self.bump() {
            if ch == '\\' {
                self.bump();
            } else if ch == quote {
                return true;
            }
        }
        false
    }

    /// Consume a raw string after its prefix: `#*"..."#*`.
    fn raw_string(&mut self) -> bool {
        let hashes = self.rest().bytes().take_while(|&b| b == b'#').count();
        self.pos += hashes;
        if !self.eat('"') {
            return false;
        }
        let closing = format!("\"{}", "#".repeat(hashes));
        match self.rest().find(&closing) {
            Some(end) => {
                self.pos += end + closing.len();
                true
            }
            None => false,
        }
    }

    /// Consume a number (without its suffix), the way rustc does.
    fn number(&mut self) {
        let digits = |lexer: &mut Self, hex: bool| {
            while let Some(ch) = lexer.peek()
                && (ch.is_ascii_digit() || ch == '_' || (hex && ch.is_ascii_hexdigit()))
            {
                lexer.pos += 1;
            }
        };
        let rest = self.rest();
        if rest.starts_with("0x") || rest.starts_with("0o") || rest.starts_with("0b") {
            self.pos += 2;
            digits(self, rest.starts_with("0x"));
            return;
        }
        digits(self, false);
        // A fraction, unless the `.` starts a range, a method call or a
        // field access (`1..2`, `1.max(2)`).
        if self.peek() == Some('.')
            && !self
                .peek_nth(1)
                .is_some_and(|next| next == '.' || is_ident_start(next))
        {
            self.pos += 1;
            if self.peek().is_some_and(|ch| ch.is_ascii_digit()) {
                digits(self, false);
            }
        }
        // An exponent.
        if matches!(self.peek(), Some('e' | 'E')) {
            let sign = usize::from(matches!(self.peek_nth(1), Some('+' | '-')));
            if self
                .peek_nth(1 + sign)
                .is_some_and(|ch| ch.is_ascii_digit() || ch == '_')
            {
                self.pos += 1 + sign;
                digits(self, false);
            }
        }
    }
}

/// The length of the (nested) block comment at the start of `rest`.
const fn block_comment_len(rest: &str) -> Option<usize> {
    let bytes = rest.as_bytes();
    let mut depth = 0usize;
    let mut i = 0;
    while i + 1 < bytes.len() {
        match (bytes[i], bytes[i + 1]) {
            (b'/', b'*') => {
                depth += 1;
                i += 2;
            }
            (b'*', b'/') => {
                depth -= 1;
                i += 2;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => i += 1,
        }
    }
    None
}

/// `#[doc = "<text>"]`, or `#![doc = "<text>"]` for an inner doc comment.
fn doc_comment(trees: &mut Vec<TokenTree>, text: &str, inner: bool, span: Span) {
    let mut pound = Punct::new('#', Spacing::Alone);
    pound.set_span(span);
    trees.push(pound.into());
    if inner {
        let mut bang = Punct::new('!', Spacing::Alone);
        bang.set_span(span);
        trees.push(bang.into());
    }
    let mut eq = Punct::new('=', Spacing::Alone);
    eq.set_span(span);
    let mut literal = Literal::string(text);
    literal.set_span(span);
    let body = TokenStream::from_vec(vec![
        Ident::new_lexed("doc", false, span).into(),
        eq.into(),
        literal.into(),
    ]);
    let mut group = Group::new(Delimiter::Bracket, body);
    group.set_span(span);
    trees.push(group.into());
}
