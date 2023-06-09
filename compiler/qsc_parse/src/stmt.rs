// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#[cfg(test)]
mod tests;

use super::{
    expr::{self, expr, expr_stmt},
    keyword::Keyword,
    prim::{ident, keyword, many, opt, pat, seq, token},
    scan::Scanner,
    top, Error, Result,
};
use crate::{
    lex::{Delim, TokenKind},
    ErrorKind,
};
use qsc_ast::ast::{
    Block, Mutability, NodeId, QubitInit, QubitInitKind, QubitSource, Stmt, StmtKind,
};
use qsc_data_structures::span::Span;

pub(super) fn block(s: &mut Scanner) -> Result<Box<Block>> {
    let lo = s.peek().span.lo;
    token(s, TokenKind::Open(Delim::Brace))?;
    let stmts = many(s, stmt)?;
    check_semis(&stmts)?;
    token(s, TokenKind::Close(Delim::Brace))?;
    Ok(Box::new(Block {
        id: NodeId::default(),
        span: s.span(lo),
        stmts: stmts.into_boxed_slice(),
    }))
}

pub(super) fn stmt(s: &mut Scanner) -> Result<Box<Stmt>> {
    let lo = s.peek().span.lo;
    let kind = if token(s, TokenKind::Semi).is_ok() {
        Ok(Box::new(StmtKind::Empty))
    } else if let Some(item) = opt(s, top::item)? {
        Ok(Box::new(StmtKind::Item(item)))
    } else if let Some(var) = opt(s, var_binding)? {
        Ok(var)
    } else if let Some(qubit) = opt(s, qubit_binding)? {
        Ok(qubit)
    } else {
        let e = expr_stmt(s)?;
        if token(s, TokenKind::Semi).is_ok() {
            Ok(Box::new(StmtKind::Semi(e)))
        } else {
            Ok(Box::new(StmtKind::Expr(e)))
        }
    }?;

    Ok(Box::new(Stmt {
        id: NodeId::default(),
        span: s.span(lo),
        kind,
    }))
}

fn var_binding(s: &mut Scanner) -> Result<Box<StmtKind>> {
    let mutability = if keyword(s, Keyword::Let).is_ok() {
        Ok(Mutability::Immutable)
    } else if keyword(s, Keyword::Mutable).is_ok() {
        Ok(Mutability::Mutable)
    } else {
        let token = s.peek();
        Err(Error(ErrorKind::Rule(
            "variable binding",
            token.kind,
            token.span,
        )))
    }?;

    let lhs = pat(s)?;
    token(s, TokenKind::Eq)?;
    let rhs = expr(s)?;
    token(s, TokenKind::Semi)?;
    Ok(Box::new(StmtKind::Local(mutability, lhs, rhs)))
}

fn qubit_binding(s: &mut Scanner) -> Result<Box<StmtKind>> {
    let source = if keyword(s, Keyword::Use).is_ok() {
        Ok(QubitSource::Fresh)
    } else if keyword(s, Keyword::Borrow).is_ok() {
        Ok(QubitSource::Dirty)
    } else {
        Err(Error(ErrorKind::Rule(
            "qubit binding",
            s.peek().kind,
            s.peek().span,
        )))
    }?;

    let lhs = pat(s)?;
    token(s, TokenKind::Eq)?;
    let rhs = qubit_init(s)?;
    let scope = opt(s, block)?;
    if scope.is_none() {
        token(s, TokenKind::Semi)?;
    }

    Ok(Box::new(StmtKind::Qubit(source, lhs, rhs, scope)))
}

fn qubit_init(s: &mut Scanner) -> Result<Box<QubitInit>> {
    let lo = s.peek().span.lo;
    let kind = if let Ok(name) = ident(s) {
        if name.name.as_ref() != "Qubit" {
            Err(Error(ErrorKind::Convert(
                "qubit initializer",
                "identifier",
                name.span,
            )))
        } else if token(s, TokenKind::Open(Delim::Paren)).is_ok() {
            token(s, TokenKind::Close(Delim::Paren))?;
            Ok(QubitInitKind::Single)
        } else if token(s, TokenKind::Open(Delim::Bracket)).is_ok() {
            let size = expr(s)?;
            token(s, TokenKind::Close(Delim::Bracket))?;
            Ok(QubitInitKind::Array(size))
        } else {
            let token = s.peek();
            Err(Error(ErrorKind::Rule(
                "qubit suffix",
                token.kind,
                token.span,
            )))
        }
    } else if token(s, TokenKind::Open(Delim::Paren)).is_ok() {
        let (inits, final_sep) = seq(s, qubit_init)?;
        token(s, TokenKind::Close(Delim::Paren))?;
        Ok(final_sep.reify(inits, QubitInitKind::Paren, QubitInitKind::Tuple))
    } else {
        let token = s.peek();
        Err(Error(ErrorKind::Rule(
            "qubit initializer",
            token.kind,
            token.span,
        )))
    }?;

    Ok(Box::new(QubitInit {
        id: NodeId::default(),
        span: s.span(lo),
        kind: Box::new(kind),
    }))
}

fn check_semis(stmts: &[Box<Stmt>]) -> Result<()> {
    let leading_stmts = stmts.split_last().map_or([].as_slice(), |s| s.1);
    for stmt in leading_stmts {
        if matches!(&*stmt.kind, StmtKind::Expr(expr) if !expr::is_stmt_final(&expr.kind)) {
            let span = Span {
                lo: stmt.span.hi,
                hi: stmt.span.hi,
            };
            return Err(Error(ErrorKind::MissingSemi(span)));
        }
    }

    Ok(())
}