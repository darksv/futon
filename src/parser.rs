use super::{Keyword, Lexer, PunctKind, Token, TokenType};
use crate::arena::Arena;
use crate::ast::{Argument, Expression, Field, Item, Operator};
use crate::multi_peek::MultiPeek;
use crate::ty::{Ty, TyS};
use std::fmt;

pub struct Parser<'lex, 'tcx> {
    peek: MultiPeek<Token<'lex>, Lexer<'lex>>,
    ty: &'tcx Arena<TyS<'tcx>>,
}

pub enum ParseError {
    UnexpectedToken(TokenType, usize, usize, Option<TokenType>),
    Custom(&'static str),
}

impl fmt::Debug for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            ParseError::UnexpectedToken(actual, line, column, None) => {
                write!(f, "Unexpected {:?} at {}:{}", actual, line, column)?
            }
            ParseError::UnexpectedToken(actual, line, column, Some(expected)) => write!(
                f,
                "Unexpected {:?} at {}:{}, expected {:?}",
                actual, line, column, expected
            )?,
            ParseError::Custom(msg) => write!(f, "{}", msg)?,
        }
        Ok(())
    }
}

type ParseResult<T> = Result<T, ParseError>;

impl<'lex, 'tcx> Parser<'lex, 'tcx> {
    pub(crate) fn new(lex: Lexer<'lex>, arena: &'tcx Arena<TyS<'tcx>>) -> Parser<'lex, 'tcx> {
        Parser {
            peek: MultiPeek::new(lex),
            ty: arena,
        }
    }

    pub(crate) fn parse(&mut self) -> ParseResult<Vec<Item<'tcx>>> {
        let mut items = vec![];
        loop {
            let token = self.peek(0);
            let item = match token.get_type() {
                TokenType::Keyword(Keyword::Extern) => self.parse_fn(true),
                TokenType::Keyword(Keyword::Fn) => self.parse_fn(false),
                TokenType::Keyword(Keyword::Struct) => self.parse_struct(),
                TokenType::EndOfSource => break,
                token_type => unimplemented!("{:?}", token_type),
            };
            items.push(item?);
        }
        Ok(items)
    }

    fn parse_stmts(&mut self) -> ParseResult<Vec<Item<'tcx>>> {
        let mut items = vec![];
        loop {
            let token = self.peek(0);
            let item = match token.get_type() {
                TokenType::Keyword(Keyword::Let) => self.parse_let(),
                TokenType::Keyword(Keyword::Loop) => self.parse_loop(),
                TokenType::Keyword(Keyword::For) => self.parse_for(),
                TokenType::Keyword(Keyword::Extern) => self.parse_fn(true),
                TokenType::Keyword(Keyword::Fn) => self.parse_fn(false),
                TokenType::Keyword(Keyword::If) => self.parse_if(),
                TokenType::Keyword(Keyword::Yield) => self.parse_yield(),
                TokenType::Keyword(Keyword::Return) => self.parse_return(),
                TokenType::Keyword(Keyword::Break) => {
                    self.advance();
                    self.expect_one(';')?;
                    Ok(Item::Break)
                }
                TokenType::Punct('*') => {
                    let lhs = self
                        .parse_expr_opt(0)?
                        .ok_or(ParseError::Custom("expected expression"))?;
                    self.parse_assign_or_expr(lhs)
                }
                TokenType::Identifier => {
                    let lhs = self
                        .parse_expr_opt(0)?
                        .unwrap_or_else(|| Expression::Identifier(token.as_string()));
                    self.parse_assign_or_expr(lhs)
                }
                _ => break,
            };
            items.push(item?);
        }
        Ok(items)
    }

    fn parse_assign_or_expr(&mut self, lhs: Expression) -> ParseResult<Item<'tcx>> {
        let item = if self.match_many(&['+', '=']) {
            Item::Assignment {
                lhs,
                operator: Some(Operator::Add),
                expr: self.parse_expr(0)?,
            }
        } else if self.match_one('=') {
            Item::Assignment {
                lhs,
                operator: None,
                expr: self.parse_expr(0)?,
            }
        } else {
            Item::Expr { expr: lhs }
        };
        self.expect_one(';')?;
        Ok(item)
    }

    fn parse_expr(&mut self, precedence: isize) -> ParseResult<Expression> {
        Ok(self
            .parse_expr_opt(precedence)?
            .ok_or(ParseError::Custom("missing expression"))?)
    }

    fn parse_expr_opt(&mut self, precedence: isize) -> ParseResult<Option<Expression>> {
        let token = self.peek(0);
        let lhs = match token.get_type() {
            TokenType::Punct('-') | TokenType::Punct('&') | TokenType::Punct('*') => {
                self.advance();
                let op = match token.get_type() {
                    TokenType::Punct('-') => Operator::Negate,
                    TokenType::Punct('&') => Operator::Ref,
                    TokenType::Punct('*') => Operator::Deref,
                    _ => unreachable!(),
                };
                let operand = self.parse_expr(10)?;
                Expression::Prefix(op, Box::new(operand))
            }
            TokenType::Keyword(Keyword::Range) => {
                self.advance();
                let operand = self.parse_expr(10)?;
                Expression::Range(Box::new(operand), None)
            }
            TokenType::Identifier => {
                self.advance();
                Expression::Identifier(token.as_string())
            }
            TokenType::IntegralNumber => {
                self.advance();
                Expression::Integer(token.get_integer().unwrap())
            }
            TokenType::FloatingNumber => {
                self.advance();
                Expression::Float(token.get_float().unwrap())
            }
            TokenType::Keyword(Keyword::True) => {
                self.advance();
                Expression::Bool(true)
            }
            TokenType::Keyword(Keyword::False) => {
                self.advance();
                Expression::Bool(false)
            }
            TokenType::Punct('(') => {
                self.advance();
                let values = self.parse_comma_separated_exprs()?;
                self.expect_one(')')?;
                match values.len() {
                    1 => values.into_iter().next().unwrap(),
                    _ => Expression::Tuple(values),
                }
            }
            TokenType::Punct('[') => {
                self.advance();
                let mut values = vec![];
                loop {
                    if self.match_one(']') {
                        break;
                    }
                    values.push(self.parse_expr(0)?);
                    self.match_one(',');
                }
                Expression::Array(values)
            }
            _other => {
                return Ok(None);
            }
        };

        let mut expr = lhs;
        loop {
            let token = self.peek(0);

            let new_precedence = Self::get_precedence(&token);
            if new_precedence < precedence
                || new_precedence == precedence && Self::is_left_associative(&token)
            {
                break;
            }

            expr = match token.get_type() {
                TokenType::Punct('+')
                | TokenType::Punct('-')
                | TokenType::Punct('*')
                | TokenType::Punct('/') => {
                    if self.peek(1).get_type() == TokenType::Punct('=') {
                        break;
                    }

                    let op = match self.advance().get_type() {
                        TokenType::Punct('+') => Operator::Add,
                        TokenType::Punct('-') => Operator::Sub,
                        TokenType::Punct('*') => Operator::Mul,
                        TokenType::Punct('/') => Operator::Div,
                        _ => unreachable!(),
                    };

                    let rhs = self.parse_expr(new_precedence)?;
                    Expression::Infix(op, Box::new(expr), Box::new(rhs))
                }
                TokenType::Punct('.') => {
                    self.advance();
                    let rhs = self.parse_expr(new_precedence)?;
                    Expression::Place(Box::new(expr), Box::new(rhs))
                }
                TokenType::Punct('<')
                | TokenType::Punct('>')
                | TokenType::Punct('!')
                | TokenType::Punct('=') => {
                    let first = self.peek(0);
                    let second = self.peek(1);
                    let is_joint = match first.get_punct() {
                        Some((_, PunctKind::Joint)) => true,
                        _ => false,
                    };
                    let op = match (first.get_type(), second.get_type(), is_joint) {
                        (TokenType::Punct('<'), TokenType::Punct('='), true) => Operator::LessEqual,
                        (TokenType::Punct('<'), TokenType::Punct('>'), true) => Operator::NotEqual,
                        (TokenType::Punct('>'), TokenType::Punct('='), true) => {
                            Operator::GreaterEqual
                        }
                        (TokenType::Punct('<'), _, _) => Operator::Less,
                        (TokenType::Punct('>'), _, _) => Operator::Greater,
                        (TokenType::Punct('='), TokenType::Punct('='), true) => Operator::Equal,
                        (TokenType::Punct('!'), TokenType::Punct('='), true) => Operator::NotEqual,
                        (TokenType::Punct('='), _, _) => return Ok(Some(expr)),
                        _ => {
                            return Err(ParseError::UnexpectedToken(
                                second.get_type(),
                                second.line(),
                                second.column(),
                                None,
                            ))
                        }
                    };
                    if let Operator::Less | Operator::Greater = op {
                        self.advance();
                    } else {
                        self.advance();
                        self.advance();
                    }
                    let rhs = self.parse_expr(new_precedence)?;
                    Expression::Infix(op, Box::new(expr), Box::new(rhs))
                }
                TokenType::Punct('(') => {
                    self.advance();
                    let args = self.parse_comma_separated_exprs()?;
                    self.expect_one(')')?;
                    Expression::Call(Box::new(expr), args)
                }
                _ => break,
            };
        }

        Ok(Some(expr))
    }

    fn parse_comma_separated_exprs(&mut self) -> ParseResult<Vec<Expression>> {
        let mut values = vec![];
        loop {
            let value = self.parse_expr(0)?;
            values.push(value);
            if !self.match_one(',') {
                break;
            }
        }
        Ok(values)
    }

    fn get_precedence(token: &Token) -> isize {
        match token.get_type() {
            TokenType::Punct('+') => 1,
            TokenType::Punct('-') => 1,
            TokenType::Punct('*') => 2,
            TokenType::Punct('/') => 2,
            TokenType::Punct('.') => 3,
            _ => 0,
        }
    }

    fn is_left_associative(token: &Token) -> bool {
        match token.get_type() {
            TokenType::Punct('-') => true,
            TokenType::Punct('.') => true,
            _ => false,
        }
    }

    fn parse_struct(&mut self) -> ParseResult<Item<'tcx>> {
        self.expect_keyword(Keyword::Struct)?;
        let identifier = self.expect_identifier()?.as_string();
        self.expect_one('{')?;
        let mut fields = vec![];
        while let Some(t) = self.match_identifier() {
            self.expect_one(':')?;
            let ty = self.parse_ty()?;
            fields.push(Field {
                name: t.as_string(),
                ty,
            });
            if !self.match_one(',') {
                break;
            }
        }
        self.expect_one('}')?;
        Ok(Item::Struct {
            name: identifier,
            fields,
        })
    }

    fn parse_fn(&mut self, is_extern: bool) -> ParseResult<Item<'tcx>> {
        if is_extern {
            self.expect_keyword(Keyword::Extern)?;
        }
        self.expect_keyword(Keyword::Fn)?;
        let identifier = self.expect_identifier()?.as_string();
        self.expect_one('(')?;
        let mut args = vec![];
        while let Some(t) = self.match_identifier() {
            self.expect_one(':')?;
            let ty = self.parse_ty()?;
            args.push(Argument {
                name: t.as_string(),
                ty,
            });
            if !self.match_one(',') {
                break;
            }
        }
        self.expect_one(')')?;

        let ty = if self.match_many(&['-', '>']) {
            self.parse_ty()?
        } else {
            self.make_ty(TyS::Unit)
        };

        let body = if is_extern {
            self.expect_one(';')?;
            vec![]
        } else {
            self.expect_one('{')?;
            let body = self.parse_stmts()?;
            self.expect_one('}')?;
            body
        };

        Ok(Item::Function {
            name: identifier,
            is_extern,
            args,
            body,
            ty,
        })
    }

    fn parse_let(&mut self) -> ParseResult<Item<'tcx>> {
        self.expect_keyword(Keyword::Let)?;
        let identifier = self.expect_identifier()?.as_string();
        let ty = if self.match_one(':') {
            Some(self.parse_ty()?)
        } else {
            None
        };

        let expr = if self.match_one('=') {
            Some(self.parse_expr(0)?)
        } else {
            None
        };

        self.expect_one(';')?;

        Ok(Item::Let {
            name: identifier,
            ty,
            expr,
        })
    }

    fn parse_ty(&mut self) -> ParseResult<Ty<'tcx>> {
        let token = self.advance();
        let ty = match token.get_type() {
            TokenType::Punct('[') => {
                let token = self.peek(0);
                let length = match token.get_type() {
                    TokenType::IntegralNumber => {
                        let length = token.get_integer().unwrap() as usize;
                        self.advance();
                        Some(length)
                    }
                    _ => None,
                };
                self.expect_one(']')?;
                let ty = self.parse_ty()?;
                match length {
                    Some(length) => Ok(TyS::Array(length, ty)),
                    None => Ok(TyS::Slice(ty)),
                }
            }
            TokenType::Punct('*') => Ok(TyS::Pointer(self.parse_ty()?)),
            TokenType::Identifier => Ok(match token.as_slice() {
                "bool" => TyS::Bool,
                "i32" => TyS::I32,
                "u32" => TyS::U32,
                ud => TyS::Other(ud.to_string()),
            }),
            TokenType::Keyword(Keyword::Fn) => {
                self.expect_one('(')?;
                let args = self.parse_ty_tuple()?;

                let ret = if self.match_many(&['-', '>']) {
                    self.parse_ty()?
                } else {
                    self.make_ty(TyS::Unit)
                };

                Ok(TyS::Function(args, ret))
            }
            TokenType::Punct('(') => {
                let types = self.parse_ty_tuple()?;
                Ok(TyS::Tuple(types))
            }
            _ => unimplemented!(),
        };
        ty.map(|ty| self.make_ty(ty))
    }

    fn make_ty(&self, ty: TyS<'tcx>) -> Ty<'tcx> {
        if let Some(ty) = self.ty.find(&ty) {
            return ty;
        }
        self.ty.alloc(ty)
    }

    fn parse_ty_tuple(&mut self) -> ParseResult<Vec<Ty<'tcx>>> {
        let mut args = vec![];
        loop {
            if self.match_one(')') {
                break;
            }
            args.push(self.parse_ty()?);
            if !self.match_one(',') {
                self.expect_one(')')?;
                break;
            }
        }
        Ok(args)
    }

    fn parse_for(&mut self) -> ParseResult<Item<'tcx>> {
        self.expect_keyword(Keyword::For)?;
        let identifier = self.expect_identifier()?.as_string();
        self.expect_keyword(Keyword::In)?;
        let expr = self.parse_expr(0)?;
        self.expect_one('{')?;
        let items = self.parse_stmts()?;
        self.expect_one('}')?;
        Ok(Item::ForIn {
            name: identifier.to_owned(),
            expr,
            body: items,
        })
    }

    fn parse_loop(&mut self) -> ParseResult<Item<'tcx>> {
        self.expect_keyword(Keyword::Loop)?;
        self.expect_one('{')?;
        let items = self.parse_stmts()?;
        self.expect_one('}')?;
        Ok(Item::Loop { body: items })
    }

    fn parse_if(&mut self) -> ParseResult<Item<'tcx>> {
        self.expect_keyword(Keyword::If)?;
        let condition = self.parse_expr(0)?;
        self.expect_one('{')?;
        let arm_true = self.parse_stmts()?;
        self.expect_one('}')?;
        let arm_false = if self.match_keyword(Keyword::Else).is_some() {
            if self.peek(0).get_type() == TokenType::Keyword(Keyword::If) {
                let item = self.parse_if()?;
                Some(vec![item])
            } else {
                self.expect_one('{')?;
                let false_arm = self.parse_stmts()?;
                self.expect_one('}')?;
                Some(false_arm)
            }
        } else {
            None
        };
        Ok(Item::If {
            condition,
            arm_true,
            arm_false,
        })
    }

    fn parse_yield(&mut self) -> ParseResult<Item<'tcx>> {
        self.expect_keyword(Keyword::Yield)?;
        let value = self.parse_expr(0)?;
        Ok(Item::Yield(Box::new(value)))
    }

    fn parse_return(&mut self) -> ParseResult<Item<'tcx>> {
        self.expect_keyword(Keyword::Return)?;
        let value = self.parse_expr(0)?;
        self.expect_one(';')?;
        Ok(Item::Return(Box::new(value)))
    }

    fn expect_keyword(&mut self, keyword: Keyword) -> ParseResult<Token<'lex>> {
        match self.match_keyword(keyword) {
            Some(token) => Ok(token),
            None => Err(self.expected(TokenType::Keyword(keyword))),
        }
    }

    fn expected(&mut self, token_type: TokenType) -> ParseError {
        let token = self.peek(0);
        ParseError::UnexpectedToken(
            token.get_type(),
            token.line(),
            token.column(),
            Some(token_type),
        )
    }

    fn match_keyword(&mut self, keyword: Keyword) -> Option<Token<'lex>> {
        match self.peek(0).get_type() {
            TokenType::Keyword(kw) if kw == keyword => Some(self.advance()),
            _ => None,
        }
    }

    fn expect_identifier(&mut self) -> ParseResult<Token<'lex>> {
        match self.match_identifier() {
            Some(token) => Ok(token),
            None => Err(self.expected(TokenType::Identifier)),
        }
    }

    fn match_identifier(&mut self) -> Option<Token<'lex>> {
        match self.peek(0).get_type() {
            TokenType::Identifier => Some(self.advance()),
            _ => None,
        }
    }

    /// Consumes and returns next token, otherwise returns an error
    fn expect_one(&mut self, ch: char) -> ParseResult<()> {
        match self.expect_punct(PunctKind::Single, ch) {
            Ok(_) => Ok(()),
            Err(_) => self.expect_punct(PunctKind::Joint, ch),
        }
    }

    /// Consumes next token and returns true only if it is a char given by argument
    fn match_one(&mut self, ch: char) -> bool {
        self.match_punct(PunctKind::Single, ch) || self.match_punct(PunctKind::Joint, ch)
    }

    #[allow(dead_code)]
    #[inline]
    fn expect_many(&mut self, chars: &[char]) -> ParseResult<()> {
        for (idx, &expected) in chars.iter().enumerate() {
            match self.peek(0).get_punct() {
                Some((actual, kind)) if actual == expected => {
                    if idx == chars.len() - 1 || kind == PunctKind::Joint {
                        self.advance();
                    }
                }
                _ => {
                    return Err(self.expected(TokenType::Punct(expected)));
                }
            }
        }
        Ok(())
    }

    #[inline]
    fn match_many(&mut self, chars: &[char]) -> bool {
        for (index, &expected) in chars.iter().enumerate() {
            if let Some((actual, kind)) = self.peek(index).get_punct() {
                if actual != expected {
                    return false;
                }

                let is_last = index == chars.len() - 1;
                match kind {
                    PunctKind::Single if !is_last => return false,
                    PunctKind::Single | PunctKind::Joint => continue,
                }
            }
        }

        for _ in chars {
            self.advance();
        }

        true
    }

    /// Consumes and returns next token, otherwise returns an error
    fn expect_punct(&mut self, kind: PunctKind, ch: char) -> ParseResult<()> {
        if self.match_punct(kind, ch) {
            Ok(())
        } else {
            Err(self.expected(TokenType::Punct(ch)))
        }
    }

    /// Consumes and returns next token only if it is a char given by argument
    fn match_punct(&mut self, kind: PunctKind, ch: char) -> bool {
        if self.peek(0).get_punct() == Some((ch, kind)) {
            self.advance();
            return true;
        }
        false
    }

    /// Returns next token without consuming it
    fn peek(&mut self, offset: usize) -> Token<'lex> {
        self.peek.peek(offset).clone()
    }

    /// Returns next token and consumes it
    fn advance(&mut self) -> Token<'lex> {
        self.peek.advance()
    }
}
