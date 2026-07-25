//! Expressions.
//!
//! Following the Rust reference, the grammar is split into expressions that
//! end in a `{ ... }` block ([`ExpressionWithBlock`] — `if`, `match`,
//! `loop`/`while`/`for`, `unsafe`/`async`/`const { }`, bare blocks) and
//! those that don't ([`ExpressionWithoutBlock`] — everything else,
//! including operators, closures, and struct/macro-call expressions). The
//! distinction matters for parsing statements: an [`ExpressionWithBlock`]
//! can appear as a statement without a trailing `;`, while an
//! [`ExpressionWithoutBlock`] needs one (except in tail position).
//! [`Expression`] itself is a thin wrapper over the two.
//!
//! Coverage is close to the full stable grammar. The remaining gaps: string/
//! char/byte literals (a pre-existing limitation of
//! [`ast::literal::Literal`](crate::ast::literal::Literal)), slice/range/box
//! patterns (see [`ast::pattern`](crate::ast::pattern)), unstable
//! multi-condition let-chains (`if let ... && let ...`), `try`/`yeet`
//! blocks, and inline `asm!`.
//!
//! [`ExpressionWithoutBlock::parse`] is a hand-written precedence-climbing
//! parser — see the doc comment on the `impl ExpressionWithoutBlock` block
//! further down for the precedence table and the technique used to
//! disambiguate operators that share a textual prefix with a longer one
//! (`&` vs `&&`, `=` vs `==`/`=>`, and so on).

use crate::{ToTokens, proc_macro::Delimiter};

use crate::{
    ast::{
        delimiter::{Braced, Bracketed, Parenthesized},
        item::Lifetime,
        literal::{Literal, LiteralNumber},
        path::{GenericArgs, SimplePath},
        pattern::Pattern,
        statements::Statement,
        tokens::{
            And, AndAnd, AndEq, As, Async, Await, Break, Caret, CaretEq, Colon, Comma, Const,
            Continue, Dot, DotDot, DotDotEq, Else, Eq, EqEq, FatArrow, For, Ge, Gt, If, In, Le,
            Let, Loop, Lt, Match, Minus, MinusEq, Move, Mut, Ne, Not, Or, OrEq, OrOr, PathSep,
            Percent, PercentEq, Plus, PlusEq, Question, RArrow, Return, Semicolon, Shl, ShlEq,
            Shr, ShrEq, Slash, SlashEq, Star, StarEq, Unsafe, While,
        },
        r#type::{Type, TypePath},
    },
    combinator::{Punctuated, StopOnError},
    error::{Diagnostics, Result},
    parse::{Parse, ParseBuffer},
    proc_macro::{Group, Ident},
};

/// Wrap an [`ExpressionWithoutBlock`] in the [`Expression`] enum — used
/// throughout the precedence-climbing parser below, since every operand
/// field is typed [`Expression`] (never [`ExpressionWithoutBlock`]
/// directly), matching the rest of this module.
fn wrap(expr: ExpressionWithoutBlock) -> Expression {
    Expression::WithoutBlock(Box::new(expr))
}

/// True if `T` would parse successfully at the current position, without
/// consuming any input either way — a genuine negative-lookahead check.
/// Unlike [`crate::parse::Peekable`] (which commits the match on success),
/// this never mutates `input`.
fn looks_like<T: Parse>(input: &ParseBuffer) -> bool {
    input.clone().try_parse::<T>().is_ok()
}

/// An expression parsed with struct literals suppressed, matching the
/// original `Expression::parse`'s WithBlock-first ordering: used for
/// [`Conditions`] and [`ForExpression`]'s iterator expression, where a bare
/// `Path { ... }` would be ambiguous with the construct's own trailing
/// block.
fn parse_restricted_scrutinee(input: &mut ParseBuffer) -> Result<Expression> {
    if let Ok(block) = input.try_parse() {
        Ok(Expression::WithBlock(Box::new(block)))
    } else {
        Ok(wrap(ExpressionWithoutBlock::parse_no_struct_literal(
            input,
        )?))
    }
}

/// Any expression: either [`WithBlock`](Self::WithBlock) or
/// [`WithoutBlock`](Self::WithoutBlock) — see the [module docs](self).
///
/// Reference: <https://doc.rust-lang.org/reference/expressions.html>
#[derive(Clone, Debug)]
pub enum Expression {
    /// An expression without a trailing `{ ... }` block.
    WithoutBlock(Box<ExpressionWithoutBlock>),
    /// An expression ending in a `{ ... }` block.
    WithBlock(Box<ExpressionWithBlock>),
}

/// An expression that doesn't end in a `{ ... }` block. See the
/// [module docs](self) for what's covered.
///
/// Parsing handles the trailing/postfix operators (`.await`, `.0`/`.field`,
/// `[index]`, `(call)`, `..`/`..=` ranges) itself, in a loop, after parsing
/// the leading primary expression — so e.g. `foo.bar().baz` parses as nested
/// [`FieldExpression`]/[`CallExpression`] values.
///
/// Reference: <https://doc.rust-lang.org/reference/expressions.html>
#[derive(Clone, Debug)]
pub enum ExpressionWithoutBlock {
    /// A literal, e.g. `1`, `1.5`.
    ///
    /// Reference: <https://doc.rust-lang.org/reference/expressions/literal-expr.html>
    Literal(Literal),
    /// A path used as an expression, e.g. `foo::bar`.
    ///
    /// Reference: <https://doc.rust-lang.org/reference/expressions/path-expr.html>
    Path(TypePath),
    /// `expr.await`.
    ///
    /// Reference: <https://doc.rust-lang.org/reference/expressions/await-expr.html>
    Await(AwaitExpression),
    /// `expr[index]`.
    ///
    /// Reference: <https://doc.rust-lang.org/reference/expressions/array-expr.html#array-and-slice-indexing-expressions>
    Index(IndexExpression),
    /// `[a, b, c]` or `[x; n]`.
    ///
    /// Reference: <https://doc.rust-lang.org/reference/expressions/array-expr.html#array-expressions>
    Array(ArrayExpression),
    /// `(a, b, c)`.
    ///
    /// Reference: <https://doc.rust-lang.org/reference/expressions/tuple-expr.html#tuple-expressions>
    Tuple(TupleExpression),
    /// `expr.0`.
    ///
    /// Reference: <https://doc.rust-lang.org/reference/expressions/tuple-expr.html#tuple-indexing-expressions>
    TupleIndex(TupleIndexExpression),
    /// `expr.field`.
    ///
    /// Reference: <https://doc.rust-lang.org/reference/expressions/field-expr.html>
    Field(FieldExpression),
    /// `return expr`.
    ///
    /// Reference: <https://doc.rust-lang.org/reference/expressions/return-expr.html>
    Return(ReturnExpression),
    /// `continue` / `continue 'label`.
    ///
    /// Reference: <https://doc.rust-lang.org/reference/expressions/loop-expr.html#continue-expressions>
    Continue(ContinueExpression),
    /// `break`, `break 'label`, or `break expr`.
    ///
    /// Reference: <https://doc.rust-lang.org/reference/expressions/loop-expr.html#break-expressions>
    Break(BreakExpression),
    /// `_`.
    ///
    /// Reference: <https://doc.rust-lang.org/reference/expressions/underscore-expr.html>
    Underscore(UnderscoreExpression),
    /// `(expr)`.
    ///
    /// Reference: <https://doc.rust-lang.org/reference/expressions/grouped-expr.html>
    Grouped(GroupedExpression),
    /// `expr(a, b)`.
    ///
    /// Reference: <https://doc.rust-lang.org/reference/expressions/call-expr.html>
    Call(CallExpression),
    /// `a..b`, `a..`, `..b`, `..`, `a..=b`, or `..=b`.
    ///
    /// Reference: <https://doc.rust-lang.org/reference/expressions/range-expr.html>
    Range(RangeExpression),
    /// `&expr`, `&mut expr`, `-expr`, or `!expr`.
    ///
    /// Reference: <https://doc.rust-lang.org/reference/expressions/operator-expr.html#borrow-operators>,
    /// <https://doc.rust-lang.org/reference/expressions/operator-expr.html#the-dereference-operator>,
    /// <https://doc.rust-lang.org/reference/expressions/operator-expr.html#negation-operators>
    Unary(UnaryExpression),
    /// `a + b`, `a == b`, `a && b`, and so on.
    ///
    /// Reference: <https://doc.rust-lang.org/reference/expressions/operator-expr.html#arithmetic-and-logical-binary-operators>,
    /// <https://doc.rust-lang.org/reference/expressions/operator-expr.html#comparison-operators>,
    /// <https://doc.rust-lang.org/reference/expressions/operator-expr.html#lazy-boolean-operators>
    Binary(BinaryExpression),
    /// `expr as Type`.
    ///
    /// Reference: <https://doc.rust-lang.org/reference/expressions/operator-expr.html#type-cast-expressions>
    Cast(CastExpression),
    /// `place = value`.
    ///
    /// Reference: <https://doc.rust-lang.org/reference/expressions/operator-expr.html#assignment-expressions>
    Assignment(AssignmentExpression),
    /// `place += value`, and the other compound assignments.
    ///
    /// Reference: <https://doc.rust-lang.org/reference/expressions/operator-expr.html#compound-assignment-expressions>
    CompoundAssignment(CompoundAssignmentExpression),
    /// `expr?`.
    ///
    /// Reference: <https://doc.rust-lang.org/reference/expressions/operator-expr.html#the-question-mark-operator>
    Try(TryExpression),
    /// `expr.method::<T>(a, b)`.
    ///
    /// Reference: <https://doc.rust-lang.org/reference/expressions/method-call-expr.html>
    MethodCall(MethodCallExpression),
    /// `move? |params| -> Ret? body`, including `async move? |params| body`.
    ///
    /// Reference: <https://doc.rust-lang.org/reference/expressions/closure-expr.html>
    Closure(ClosureExpression),
    /// `Path { field: expr, ..base }`.
    ///
    /// Reference: <https://doc.rust-lang.org/reference/expressions/struct-expr.html>
    Struct(StructExpression),
    /// `path!(...)`, `path![...]`, or `path!{...}` used as an expression.
    ///
    /// Reference: <https://doc.rust-lang.org/reference/macros.html#macro-invocation>
    MacroCall(MacroCallExpression),
}

/// An expression that ends in a `{ ... }` block: a bare block, `unsafe`
/// block, `loop`, or `if`. See the [module docs](self).
///
/// Reference: <https://doc.rust-lang.org/reference/expressions.html>
#[derive(Clone, Debug)]
pub enum ExpressionWithBlock {
    /// A bare (possibly labeled) block: `{ ... }`.
    ///
    /// Reference: <https://doc.rust-lang.org/reference/expressions/block-expr.html>
    Block(BlockExpression),
    /// An `unsafe { ... }` block.
    ///
    /// Reference: <https://doc.rust-lang.org/reference/expressions/block-expr.html#unsafe-blocks>
    Unsafe(UnsafeBlockExpression),
    /// A `loop { ... }` expression.
    ///
    /// Reference: <https://doc.rust-lang.org/reference/expressions/loop-expr.html#infinite-loops>
    Loop(LoopExpression),
    /// An `if condition { ... } else ...` expression.
    ///
    /// Reference: <https://doc.rust-lang.org/reference/expressions/if-expr.html>
    If(IfExpression),
    /// A `while condition { ... }` expression, including `while let`.
    ///
    /// Reference: <https://doc.rust-lang.org/reference/expressions/loop-expr.html#predicate-loops>
    While(WhileExpression),
    /// A `for pat in expr { ... }` expression.
    ///
    /// Reference: <https://doc.rust-lang.org/reference/expressions/loop-expr.html#iterator-loops>
    For(ForExpression),
    /// A `match scrutinee { arm* }` expression.
    ///
    /// Reference: <https://doc.rust-lang.org/reference/expressions/match-expr.html>
    Match(MatchExpression),
    /// An `async { ... }` / `async move { ... }` block.
    ///
    /// Reference: <https://doc.rust-lang.org/reference/expressions/block-expr.html#async-blocks>
    Async(AsyncBlockExpression),
    /// A `const { ... }` block.
    ///
    /// Reference: <https://doc.rust-lang.org/reference/expressions/block-expr.html#const-blocks>
    ConstBlock(ConstBlockExpression),
}

/// A bare block, optionally labeled: `'label: { ... }`.
///
/// Reference: <https://doc.rust-lang.org/reference/expressions/block-expr.html>
#[derive(Clone, Debug)]
pub struct BlockExpression {
    label: Option<(Lifetime, Colon)>,
    block: Braced<Vec<Statement>>,
}

/// An `unsafe { ... }` block.
///
/// Reference: <https://doc.rust-lang.org/reference/expressions/block-expr.html#unsafe-blocks>
#[derive(Clone, Debug)]
pub struct UnsafeBlockExpression {
    unsafe_token: Unsafe,
    block: Braced<Vec<Statement>>,
}

/// A `loop { ... }` expression, optionally labeled.
///
/// Reference: <https://doc.rust-lang.org/reference/expressions/loop-expr.html#infinite-loops>
#[derive(Clone, Debug)]
pub struct LoopExpression {
    label: Option<(Lifetime, Colon)>,
    loop_token: Loop,
    block: Braced<Vec<Statement>>,
}

/// An `if condition { ... } else ...` expression.
///
/// Reference: <https://doc.rust-lang.org/reference/expressions/if-expr.html>
#[derive(Clone, Debug)]
pub struct IfExpression {
    if_token: If,
    condition: Conditions,
    block: Braced<Vec<Statement>>,
    else_branch: Option<(Else, ElseExpression)>,
}

/// The `else` branch of an [`IfExpression`]: another `if` (for `else if`
/// chains) or a plain block.
///
/// Reference: <https://doc.rust-lang.org/reference/expressions/if-expr.html>
#[derive(Clone, Debug)]
pub enum ElseExpression {
    /// `else if ...` (chaining into another `if`).
    If(Box<IfExpression>),
    /// `else { ... }`.
    Block(Braced<Vec<Statement>>),
}

/// The condition of an [`IfExpression`]/[`WhileExpression`].
///
/// Either a plain (struct-literal-restricted) expression, or `let PATTERN =
/// EXPR` (also restricted). Only the single-condition stable form is
/// supported — not the unstable multi-condition let-chains (`if let ... &&
/// let ...`).
///
/// Reference: <https://doc.rust-lang.org/reference/expressions/if-expr.html#if-let-expressions>
#[derive(Clone, Debug)]
pub enum Conditions {
    /// `let PATTERN = EXPR`.
    Let(Let, Pattern, Eq, Box<Expression>),
    /// A plain expression.
    Expr(Expression),
}

/// A `while condition { ... }` expression, optionally labeled, including
/// `while let`.
///
/// Reference: <https://doc.rust-lang.org/reference/expressions/loop-expr.html#predicate-loops>
#[derive(Clone, Debug)]
pub struct WhileExpression {
    label: Option<(Lifetime, Colon)>,
    while_token: While,
    condition: Conditions,
    block: Braced<Vec<Statement>>,
}

/// A `for pat in expr { ... }` expression, optionally labeled.
///
/// Reference: <https://doc.rust-lang.org/reference/expressions/loop-expr.html#iterator-loops>
#[derive(Clone, Debug)]
pub struct ForExpression {
    label: Option<(Lifetime, Colon)>,
    for_token: For,
    pat: Pattern,
    in_token: In,
    expr: Expression,
    block: Braced<Vec<Statement>>,
}

/// A `match scrutinee { arm* }` expression.
///
/// Reference: <https://doc.rust-lang.org/reference/expressions/match-expr.html>
#[derive(Clone, Debug)]
pub struct MatchExpression {
    match_token: Match,
    scrutinee: Expression,
    arms: Braced<Vec<MatchArm>>,
}

/// One arm of a [`MatchExpression`]: `pattern (if guard)? => body ,?`. The
/// pattern already covers `|` alternation (see [`Pattern::Or`]).
///
/// Reference: <https://doc.rust-lang.org/reference/expressions/match-expr.html>
#[derive(Clone, Debug)]
pub struct MatchArm {
    pat: Pattern,
    guard: Option<(If, Expression)>,
    fat_arrow: FatArrow,
    body: Expression,
    comma: Option<Comma>,
}

/// An `async { ... }` / `async move { ... }` block.
///
/// Reference: <https://doc.rust-lang.org/reference/expressions/block-expr.html#async-blocks>
#[derive(Clone, Debug)]
pub struct AsyncBlockExpression {
    async_token: Async,
    move_token: Option<Move>,
    block: Braced<Vec<Statement>>,
}

/// A `const { ... }` block.
///
/// Reference: <https://doc.rust-lang.org/reference/expressions/block-expr.html#const-blocks>
#[derive(Clone, Debug)]
pub struct ConstBlockExpression {
    const_token: Const,
    block: Braced<Vec<Statement>>,
}

/// `expr.await`.
///
/// Reference: <https://doc.rust-lang.org/reference/expressions/await-expr.html>
#[derive(Clone, Debug)]
pub struct AwaitExpression {
    expr: Expression,
    dot: Dot,
    await_token: Await,
}
/// `expr[index]`.
///
/// Reference: <https://doc.rust-lang.org/reference/expressions/array-expr.html#array-and-slice-indexing-expressions>
#[derive(Clone, Debug)]
pub struct IndexExpression {
    expr: Expression,
    index: Bracketed<Expression>,
}
/// A tuple expression: `(a, b, c)`. Parsing rejects zero elements and a
/// single element without a trailing comma, to disambiguate from
/// [`GroupedExpression`] (`(expr)`).
///
/// Reference: <https://doc.rust-lang.org/reference/expressions/tuple-expr.html#tuple-expressions>
#[derive(Clone, Debug)]
pub struct TupleExpression {
    exprs: Parenthesized<Punctuated<Expression, Comma>>,
}

/// An array expression: `[a, b, c]` or `[x; n]`.
///
/// Reference: <https://doc.rust-lang.org/reference/expressions/array-expr.html#array-expressions>
#[derive(Clone, Debug)]
pub struct ArrayExpression {
    exprs: Bracketed<ArrayElements>,
}

/// The inside of an [`ArrayExpression`]'s brackets: a repeat expression
/// (`x; n`) or an element list (`a, b, c`).
///
/// Reference: <https://doc.rust-lang.org/reference/expressions/array-expr.html#array-expressions>
#[derive(Clone, Debug)]
pub enum ArrayElements {
    /// `x; n` — repeat `x` `n` times.
    Repetition(Expression, Semicolon, Expression),
    /// `a, b, c` — a literal element list.
    List(Punctuated<Expression, Comma>),
}
/// `expr.0` (numeric tuple-field access).
///
/// Reference: <https://doc.rust-lang.org/reference/expressions/tuple-expr.html#tuple-indexing-expressions>
#[derive(Clone, Debug)]
pub struct TupleIndexExpression {
    expr: Expression,
    dot: Dot,
    index: LiteralNumber,
}

/// `expr.field` (named field access).
///
/// Reference: <https://doc.rust-lang.org/reference/expressions/field-expr.html>
#[derive(Clone, Debug)]
pub struct FieldExpression {
    expr: Expression,
    dot: Dot,
    field: Ident,
}

/// `return expr`.
///
/// Reference: <https://doc.rust-lang.org/reference/expressions/return-expr.html>
#[derive(Clone, Debug)]
pub struct ReturnExpression {
    return_token: Return,
    expr: Expression,
}

/// `continue` / `continue 'label`.
///
/// Reference: <https://doc.rust-lang.org/reference/expressions/loop-expr.html#continue-expressions>
#[derive(Clone, Debug)]
pub struct ContinueExpression {
    continue_token: Continue,
    label: Option<Lifetime>,
}
/// `break`, `break 'label`, or `break expr`.
///
/// Reference: <https://doc.rust-lang.org/reference/expressions/loop-expr.html#break-expressions>
#[derive(Clone, Debug)]
pub struct BreakExpression {
    break_token: Break,
    label: Option<Lifetime>,
    expr: Option<Expression>,
}

/// `expr(a, b)` — a function call.
///
/// Reference: <https://doc.rust-lang.org/reference/expressions/call-expr.html>
#[derive(Clone, Debug)]
pub struct CallExpression {
    expr: Expression,
    params: Parenthesized<Punctuated<Expression, Comma>>,
}
/// A range expression: `a..b`, `a..`, `..b`, `..`, `a..=b`, or `..=b`.
///
/// Reference: <https://doc.rust-lang.org/reference/expressions/range-expr.html>
#[derive(Clone, Debug)]
pub struct RangeExpression {
    start: Option<Expression>,
    dot: Option<DotDot>,
    dot_eq: Option<DotDotEq>,
    end: Option<Expression>,
}
/// The wildcard/placeholder expression `_`.
///
/// Reference: <https://doc.rust-lang.org/reference/expressions/underscore-expr.html>
#[derive(Clone, Debug)]
pub struct UnderscoreExpression {
    underscore: Ident,
}
/// A parenthesized expression: `(expr)`.
///
/// Reference: <https://doc.rust-lang.org/reference/expressions/grouped-expr.html>
#[derive(Clone, Debug)]
pub struct GroupedExpression {
    group: Parenthesized<Expression>,
}

/// `&expr`, `&mut expr`, or the double-reference shorthand `&&expr`/`&&mut expr`.
///
/// The double form keeps the original `&&` token intact (see [`BorrowAmp`])
/// rather than desugaring into two nested borrows, so it round-trips
/// exactly.
///
/// Reference: <https://doc.rust-lang.org/reference/expressions/operator-expr.html#borrow-operators>
#[derive(Clone, Debug)]
pub struct BorrowExpression {
    amp: BorrowAmp,
    mutability: Option<Mut>,
    expr: Expression,
}

/// The `&`/`&&` token of a [`BorrowExpression`].
#[derive(Clone, Debug)]
pub enum BorrowAmp {
    /// A single `&`.
    Single(And),
    /// `&&`, i.e. two borrows in one token — `mut`, if present, binds to
    /// the inner one only, matching rustc.
    Double(AndAnd),
}

/// `*expr`.
///
/// Reference: <https://doc.rust-lang.org/reference/expressions/operator-expr.html#the-dereference-operator>
#[derive(Clone, Debug)]
pub struct DereferenceExpression {
    star: Star,
    expr: Expression,
}

/// `-expr` or `!expr`.
///
/// Reference: <https://doc.rust-lang.org/reference/expressions/operator-expr.html#negation-operators>
#[derive(Clone, Debug)]
pub struct NegationExpression {
    op: NegOp,
    expr: Expression,
}

/// The operator of a [`NegationExpression`].
#[derive(Clone, Debug)]
pub enum NegOp {
    /// `-`.
    Neg(Minus),
    /// `!`.
    Not(Not),
}

/// A prefix operator expression: [`BorrowExpression`],
/// [`DereferenceExpression`], or [`NegationExpression`].
#[derive(Clone, Debug)]
pub enum UnaryExpression {
    /// `&expr` / `&mut expr` / `&&expr`.
    Borrow(BorrowExpression),
    /// `*expr`.
    Deref(DereferenceExpression),
    /// `-expr` / `!expr`.
    Negation(NegationExpression),
}

/// `lhs OP rhs`, for every arithmetic, bitwise, comparison, and lazy
/// boolean binary operator — see [`BinOp`].
///
/// Reference: <https://doc.rust-lang.org/reference/expressions/operator-expr.html#arithmetic-and-logical-binary-operators>
#[derive(Clone, Debug)]
pub struct BinaryExpression {
    lhs: Expression,
    op: BinOp,
    rhs: Expression,
}

/// The operator of a [`BinaryExpression`].
#[derive(Clone, Debug)]
pub enum BinOp {
    /// `+`.
    Add(Plus),
    /// `-`.
    Sub(Minus),
    /// `*`.
    Mul(Star),
    /// `/`.
    Div(Slash),
    /// `%`.
    Rem(Percent),
    /// `&`.
    BitAnd(And),
    /// `|`.
    BitOr(Or),
    /// `^`.
    BitXor(Caret),
    /// `<<`.
    Shl(Shl),
    /// `>>`.
    Shr(Shr),
    /// `==`.
    Eq(EqEq),
    /// `!=`.
    Ne(Ne),
    /// `<`.
    Lt(Lt),
    /// `>`.
    Gt(Gt),
    /// `<=`.
    Le(Le),
    /// `>=`.
    Ge(Ge),
    /// `&&`.
    And(AndAnd),
    /// `||`.
    Or(OrOr),
}

/// `expr as Type`.
///
/// Reference: <https://doc.rust-lang.org/reference/expressions/operator-expr.html#type-cast-expressions>
#[derive(Clone, Debug)]
pub struct CastExpression {
    expr: Expression,
    as_token: As,
    ty: Type,
}

/// `place = value`.
///
/// Reference: <https://doc.rust-lang.org/reference/expressions/operator-expr.html#assignment-expressions>
#[derive(Clone, Debug)]
pub struct AssignmentExpression {
    lhs: Expression,
    eq: Eq,
    rhs: Expression,
}

/// `place OP= value`, for every compound assignment operator — see
/// [`CompoundAssignOp`].
///
/// Reference: <https://doc.rust-lang.org/reference/expressions/operator-expr.html#compound-assignment-expressions>
#[derive(Clone, Debug)]
pub struct CompoundAssignmentExpression {
    lhs: Expression,
    op: CompoundAssignOp,
    rhs: Expression,
}

/// The operator of a [`CompoundAssignmentExpression`].
#[derive(Clone, Debug)]
pub enum CompoundAssignOp {
    /// `+=`.
    Add(PlusEq),
    /// `-=`.
    Sub(MinusEq),
    /// `*=`.
    Mul(StarEq),
    /// `/=`.
    Div(SlashEq),
    /// `%=`.
    Rem(PercentEq),
    /// `&=`.
    BitAnd(AndEq),
    /// `|=`.
    BitOr(OrEq),
    /// `^=`.
    BitXor(CaretEq),
    /// `<<=`.
    Shl(ShlEq),
    /// `>>=`.
    Shr(ShrEq),
}

/// `expr?`.
///
/// Reference: <https://doc.rust-lang.org/reference/expressions/operator-expr.html#the-question-mark-operator>
#[derive(Clone, Debug)]
pub struct TryExpression {
    expr: Expression,
    question: Question,
}

/// `expr.method::<T>(a, b)` — distinguished from [`FieldExpression`] by the
/// trailing `(...)`, and from [`CallExpression`] by the leading `.method`.
///
/// Reference: <https://doc.rust-lang.org/reference/expressions/method-call-expr.html>
#[derive(Clone, Debug)]
pub struct MethodCallExpression {
    expr: Expression,
    dot: Dot,
    method: Ident,
    turbofish: Option<(PathSep, GenericArgs)>,
    args: Parenthesized<Punctuated<Expression, Comma>>,
}

/// `move? |params| -> Ret? body`, including `async move? |params| body`.
///
/// When a return type is given, `body` must be a [`BlockExpression`]
/// (enforced by [`Parse`]); otherwise it's any [`Expression`].
///
/// Reference: <https://doc.rust-lang.org/reference/expressions/closure-expr.html>
#[derive(Clone, Debug)]
pub struct ClosureExpression {
    async_token: Option<Async>,
    move_token: Option<Move>,
    params: ClosureParams,
    return_type: Option<(RArrow, Type)>,
    body: Expression,
}

/// A [`ClosureExpression`]'s parameter list: `||` (no params) or
/// `|a, b: T|`.
#[derive(Clone, Debug)]
pub enum ClosureParams {
    /// `||`.
    Empty(OrOr),
    /// `|a, b: T|`.
    List(Or, Box<Punctuated<ClosureParam, Comma, StopOnError>>, Or),
}

/// One parameter in a [`ClosureExpression`]'s parameter list: `pat` or
/// `pat: Type`.
#[derive(Clone, Debug)]
pub struct ClosureParam {
    pat: Pattern,
    ty: Option<(Colon, Type)>,
}

/// A struct expression: `Path { field: expr, field2, ..base }`. Numeric
/// tuple-index field keys (`TupleStruct { 0: value }`) are not supported —
/// only identifier-keyed fields.
///
/// Reference: <https://doc.rust-lang.org/reference/expressions/struct-expr.html>
#[derive(Clone, Debug)]
pub struct StructExpression {
    path: TypePath,
    fields: Braced<StructExprFields>,
}

/// The inside of a [`StructExpression`]'s braces.
#[derive(Clone, Debug)]
pub struct StructExprFields {
    fields: Punctuated<StructExprField, Comma, StopOnError>,
    rest: Option<(DotDot, Box<Expression>)>,
}

/// One field inside a [`StructExprFields`] list.
#[derive(Clone, Debug)]
pub enum StructExprField {
    /// `field: expr`.
    Named(Ident, Colon, Expression),
    /// `field` shorthand for `field: field`.
    Shorthand(Ident),
}

/// A macro invocation used as an expression, e.g. `foo!(a, b)`.
///
/// Shares its shape with
/// [`MacroInvocationItem`](crate::ast::item::macro_item::MacroInvocationItem)
/// minus the (never-present, in expression position) trailing `;`.
///
/// Reference: <https://doc.rust-lang.org/reference/macros.html#macro-invocation>
#[derive(Clone, Debug)]
pub struct MacroCallExpression {
    path: SimplePath,
    bang: Not,
    body: Group,
}

impl Parse for ExpressionWithoutBlock {
    fn parse(input: &mut ParseBuffer) -> Result<Self> {
        Self::parse_top(input, true)
    }
}

impl ExpressionWithoutBlock {
    /// Parse with struct-expression literals suppressed at every level of
    /// the precedence chain below (not just the very first token) — used
    /// by `if`/`while`/`match` scrutinees and `for`'s iterator expression,
    /// where a bare `Path { ... }` would be ambiguous with the construct's
    /// own trailing block. Nested sub-expressions (inside `(...)`,
    /// `[...]`, call/index arguments) go back through the normal
    /// unrestricted [`Parse`] impl, since those recurse via a fresh
    /// [`Expression::parse`]/[`ExpressionWithoutBlock::parse`] call rather
    /// than through `no_struct`-threaded helpers here.
    pub(crate) fn parse_no_struct_literal(input: &mut ParseBuffer) -> Result<Self> {
        Self::parse_top(input, false)
    }

    fn parse_top(input: &mut ParseBuffer, allow_struct: bool) -> Result<Self> {
        // `return`/`break`/`continue`/closures swallow an optional or
        // required trailing expression and can't themselves be used as an
        // operand without parens (matching rustc) — so they're handled
        // once, here, at the very top, rather than as part of the
        // precedence chain below.
        if let Some(ident) = input.peek_ident() {
            #[allow(clippy::cmp_owned)]
            let text = ident.to_string();
            if text == "return" {
                return Ok(Self::Return(input.parse()?));
            }
            if text == "break" {
                return Ok(Self::Break(input.parse()?));
            }
            if text == "continue" {
                return Ok(Self::Continue(input.parse()?));
            }
            // `async {}`/`async move {}` (a block, not a closure) is
            // handled by `ExpressionWithBlock`, tried before this type
            // everywhere `ExpressionWithoutBlock` is reachable — so
            // "async" reaching here can only be an async closure.
            if text == "move" || text == "async" {
                return Ok(Self::Closure(input.parse()?));
            }
        }
        if looks_like::<OrOr>(input) || looks_like::<Or>(input) {
            return Ok(Self::Closure(input.parse()?));
        }
        Self::parse_assignment(input, allow_struct)
    }
}

/// Precedence-climbing operator parsing, from loosest to tightest binding:
/// assignment → range → `||` → `&&` → comparisons (non-chaining) → `|` →
/// `^` → `&` → shift → additive → multiplicative → `as` → unary prefix →
/// postfix chain → primary — see
/// <https://doc.rust-lang.org/reference/expressions.html#expression-precedence>.
///
/// Several single-char operators are textual prefixes of a longer operator
/// used at a *different*, looser level (`&` vs `&&`/`&=`, `|` vs `||`/`|=`,
/// `<`/`>` vs `<<`/`>>`/`<=`/`>=`, every `OP` vs `OP=`). Before accepting
/// the short token at its level, these negative-lookahead for the longer
/// one first (via [`looks_like`]) and defer to the looser level instead —
/// correct only because [`RustPunct2`](crate::ast::tokens::RustPunct2)/
/// [`RustPunct3`](crate::ast::tokens::RustPunct3) require `Spacing::Joint`
/// between their constituent `Punct`s, so e.g. `&&` never matches a
/// space-separated `& &`.
///
/// Every level threads `allow_struct` straight through to its tighter
/// callee — see [`parse_no_struct_literal`](Self::parse_no_struct_literal).
impl ExpressionWithoutBlock {
    fn parse_assignment(input: &mut ParseBuffer, allow_struct: bool) -> Result<Self> {
        let lhs = Self::parse_range(input, allow_struct)?;
        macro_rules! compound {
            ($tok:ty, $variant:ident) => {
                if let Ok(op) = input.try_parse::<$tok>() {
                    let rhs = Self::parse_assignment(input, allow_struct)?;
                    return Ok(Self::CompoundAssignment(CompoundAssignmentExpression {
                        lhs: wrap(lhs),
                        op: CompoundAssignOp::$variant(op),
                        rhs: wrap(rhs),
                    }));
                }
            };
        }
        compound!(PlusEq, Add);
        compound!(MinusEq, Sub);
        compound!(StarEq, Mul);
        compound!(SlashEq, Div);
        compound!(PercentEq, Rem);
        compound!(CaretEq, BitXor);
        compound!(AndEq, BitAnd);
        compound!(OrEq, BitOr);
        compound!(ShlEq, Shl);
        compound!(ShrEq, Shr);
        // `=` must not swallow the first char of `=>` (a match arm's fat
        // arrow, reached when this is a match guard expression).
        if !looks_like::<FatArrow>(input)
            && let Ok(eq) = input.try_parse::<Eq>()
        {
            let rhs = Self::parse_assignment(input, allow_struct)?;
            return Ok(Self::Assignment(AssignmentExpression {
                lhs: wrap(lhs),
                eq,
                rhs: wrap(rhs),
            }));
        }
        Ok(lhs)
    }

    /// `a..b`, `a..`, `..b`, `..`, `a..=b`, or `..=b` — sitting below `||`
    /// and above assignment; the end (like the start) is parsed one level
    /// up (`||`), not recursively, since ranges don't chain and need
    /// parens to combine with looser operators.
    fn parse_range(input: &mut ParseBuffer, allow_struct: bool) -> Result<Self> {
        // the 3-char token must be tried before its 2-char prefix
        if let Ok(dot_eq) = input.try_parse::<DotDotEq>() {
            let end = Self::parse_or(input, allow_struct).ok().map(wrap);
            return Ok(Self::Range(RangeExpression {
                start: None,
                dot: None,
                dot_eq: Some(dot_eq),
                end,
            }));
        }
        if let Ok(dot) = input.try_parse::<DotDot>() {
            let end = Self::parse_or(input, allow_struct).ok().map(wrap);
            return Ok(Self::Range(RangeExpression {
                start: None,
                dot: Some(dot),
                dot_eq: None,
                end,
            }));
        }
        let lhs = Self::parse_or(input, allow_struct)?;
        if let Ok(dot_eq) = input.try_parse::<DotDotEq>() {
            let end = Self::parse_or(input, allow_struct).ok().map(wrap);
            return Ok(Self::Range(RangeExpression {
                start: Some(wrap(lhs)),
                dot: None,
                dot_eq: Some(dot_eq),
                end,
            }));
        }
        if let Ok(dot) = input.try_parse::<DotDot>() {
            let end = Self::parse_or(input, allow_struct).ok().map(wrap);
            return Ok(Self::Range(RangeExpression {
                start: Some(wrap(lhs)),
                dot: Some(dot),
                dot_eq: None,
                end,
            }));
        }
        Ok(lhs)
    }

    fn parse_or(input: &mut ParseBuffer, allow_struct: bool) -> Result<Self> {
        let mut lhs = Self::parse_and(input, allow_struct)?;
        while let Ok(op) = input.try_parse::<OrOr>() {
            let rhs = Self::parse_and(input, allow_struct)?;
            lhs = Self::Binary(BinaryExpression {
                lhs: wrap(lhs),
                op: BinOp::Or(op),
                rhs: wrap(rhs),
            });
        }
        Ok(lhs)
    }

    fn parse_and(input: &mut ParseBuffer, allow_struct: bool) -> Result<Self> {
        let mut lhs = Self::parse_compare(input, allow_struct)?;
        while let Ok(op) = input.try_parse::<AndAnd>() {
            let rhs = Self::parse_compare(input, allow_struct)?;
            lhs = Self::Binary(BinaryExpression {
                lhs: wrap(lhs),
                op: BinOp::And(op),
                rhs: wrap(rhs),
            });
        }
        Ok(lhs)
    }

    /// Comparisons don't chain (`a < b < c` is a hard error in Rust), so
    /// this parses at most one, not a loop.
    fn parse_compare(input: &mut ParseBuffer, allow_struct: bool) -> Result<Self> {
        let lhs = Self::parse_bitor(input, allow_struct)?;
        macro_rules! cmp {
            ($tok:ty, $variant:ident) => {
                if let Ok(op) = input.try_parse::<$tok>() {
                    let rhs = Self::parse_bitor(input, allow_struct)?;
                    return Ok(Self::Binary(BinaryExpression {
                        lhs: wrap(lhs),
                        op: BinOp::$variant(op),
                        rhs: wrap(rhs),
                    }));
                }
            };
        }
        cmp!(Le, Le);
        cmp!(Ge, Ge);
        cmp!(EqEq, Eq);
        cmp!(Ne, Ne);
        // `<`/`>` must not swallow the first char of a leftover `<<`/`>>`
        // (only possible here as `<<=`/`>>=`, since plain `<<`/`>>` would
        // already have been consumed by `parse_shift` below).
        if !looks_like::<Shl>(input)
            && let Ok(op) = input.try_parse::<Lt>()
        {
            let rhs = Self::parse_bitor(input, allow_struct)?;
            return Ok(Self::Binary(BinaryExpression {
                lhs: wrap(lhs),
                op: BinOp::Lt(op),
                rhs: wrap(rhs),
            }));
        }
        if !looks_like::<Shr>(input)
            && let Ok(op) = input.try_parse::<Gt>()
        {
            let rhs = Self::parse_bitor(input, allow_struct)?;
            return Ok(Self::Binary(BinaryExpression {
                lhs: wrap(lhs),
                op: BinOp::Gt(op),
                rhs: wrap(rhs),
            }));
        }
        Ok(lhs)
    }

    fn parse_bitor(input: &mut ParseBuffer, allow_struct: bool) -> Result<Self> {
        let mut lhs = Self::parse_bitxor(input, allow_struct)?;
        loop {
            if looks_like::<OrOr>(input) || looks_like::<OrEq>(input) {
                break;
            }
            if let Ok(op) = input.try_parse::<Or>() {
                let rhs = Self::parse_bitxor(input, allow_struct)?;
                lhs = Self::Binary(BinaryExpression {
                    lhs: wrap(lhs),
                    op: BinOp::BitOr(op),
                    rhs: wrap(rhs),
                });
                continue;
            }
            break;
        }
        Ok(lhs)
    }

    fn parse_bitxor(input: &mut ParseBuffer, allow_struct: bool) -> Result<Self> {
        let mut lhs = Self::parse_bitand(input, allow_struct)?;
        loop {
            if looks_like::<CaretEq>(input) {
                break;
            }
            if let Ok(op) = input.try_parse::<Caret>() {
                let rhs = Self::parse_bitand(input, allow_struct)?;
                lhs = Self::Binary(BinaryExpression {
                    lhs: wrap(lhs),
                    op: BinOp::BitXor(op),
                    rhs: wrap(rhs),
                });
                continue;
            }
            break;
        }
        Ok(lhs)
    }

    fn parse_bitand(input: &mut ParseBuffer, allow_struct: bool) -> Result<Self> {
        let mut lhs = Self::parse_shift(input, allow_struct)?;
        loop {
            if looks_like::<AndAnd>(input) || looks_like::<AndEq>(input) {
                break;
            }
            if let Ok(op) = input.try_parse::<And>() {
                let rhs = Self::parse_shift(input, allow_struct)?;
                lhs = Self::Binary(BinaryExpression {
                    lhs: wrap(lhs),
                    op: BinOp::BitAnd(op),
                    rhs: wrap(rhs),
                });
                continue;
            }
            break;
        }
        Ok(lhs)
    }

    fn parse_shift(input: &mut ParseBuffer, allow_struct: bool) -> Result<Self> {
        let mut lhs = Self::parse_additive(input, allow_struct)?;
        loop {
            macro_rules! op {
                ($guard:ty, $tok:ty, $variant:ident) => {
                    if !looks_like::<$guard>(input)
                        && let Ok(op) = input.try_parse::<$tok>()
                    {
                        let rhs = Self::parse_additive(input, allow_struct)?;
                        lhs = Self::Binary(BinaryExpression {
                            lhs: wrap(lhs),
                            op: BinOp::$variant(op),
                            rhs: wrap(rhs),
                        });
                        continue;
                    }
                };
            }
            op!(ShlEq, Shl, Shl);
            op!(ShrEq, Shr, Shr);
            break;
        }
        Ok(lhs)
    }

    fn parse_additive(input: &mut ParseBuffer, allow_struct: bool) -> Result<Self> {
        let mut lhs = Self::parse_multiplicative(input, allow_struct)?;
        loop {
            macro_rules! op {
                ($guard:ty, $tok:ty, $variant:ident) => {
                    if !looks_like::<$guard>(input)
                        && let Ok(op) = input.try_parse::<$tok>()
                    {
                        let rhs = Self::parse_multiplicative(input, allow_struct)?;
                        lhs = Self::Binary(BinaryExpression {
                            lhs: wrap(lhs),
                            op: BinOp::$variant(op),
                            rhs: wrap(rhs),
                        });
                        continue;
                    }
                };
            }
            op!(PlusEq, Plus, Add);
            op!(MinusEq, Minus, Sub);
            break;
        }
        Ok(lhs)
    }

    fn parse_multiplicative(input: &mut ParseBuffer, allow_struct: bool) -> Result<Self> {
        let mut lhs = Self::parse_cast(input, allow_struct)?;
        loop {
            macro_rules! op {
                ($guard:ty, $tok:ty, $variant:ident) => {
                    if !looks_like::<$guard>(input)
                        && let Ok(op) = input.try_parse::<$tok>()
                    {
                        let rhs = Self::parse_cast(input, allow_struct)?;
                        lhs = Self::Binary(BinaryExpression {
                            lhs: wrap(lhs),
                            op: BinOp::$variant(op),
                            rhs: wrap(rhs),
                        });
                        continue;
                    }
                };
            }
            op!(StarEq, Star, Mul);
            op!(SlashEq, Slash, Div);
            op!(PercentEq, Percent, Rem);
            break;
        }
        Ok(lhs)
    }

    fn parse_cast(input: &mut ParseBuffer, allow_struct: bool) -> Result<Self> {
        let mut expr = Self::parse_unary(input, allow_struct)?;
        while let Ok(as_token) = input.try_parse::<As>() {
            let ty = input.parse()?;
            expr = Self::Cast(CastExpression {
                expr: wrap(expr),
                as_token,
                ty,
            });
        }
        Ok(expr)
    }

    fn parse_unary(input: &mut ParseBuffer, allow_struct: bool) -> Result<Self> {
        // `&&expr` — a single `&&` token means two levels of borrow; kept
        // as one `BorrowAmp::Double` node (not two nested `Borrow`s) so it
        // round-trips as `&&`, not a synthesized `& &`.
        if let Ok(and_and) = input.try_parse::<AndAnd>() {
            let mutability = input.try_parse().ok();
            let expr = wrap(Self::parse_unary(input, allow_struct)?);
            return Ok(Self::Unary(UnaryExpression::Borrow(BorrowExpression {
                amp: BorrowAmp::Double(and_and),
                mutability,
                expr,
            })));
        }
        if let Ok(and_token) = input.try_parse::<And>() {
            let mutability = input.try_parse().ok();
            let expr = wrap(Self::parse_unary(input, allow_struct)?);
            return Ok(Self::Unary(UnaryExpression::Borrow(BorrowExpression {
                amp: BorrowAmp::Single(and_token),
                mutability,
                expr,
            })));
        }
        if let Ok(star) = input.try_parse::<Star>() {
            let expr = wrap(Self::parse_unary(input, allow_struct)?);
            return Ok(Self::Unary(UnaryExpression::Deref(DereferenceExpression {
                star,
                expr,
            })));
        }
        if let Ok(minus) = input.try_parse::<Minus>() {
            let expr = wrap(Self::parse_unary(input, allow_struct)?);
            return Ok(Self::Unary(UnaryExpression::Negation(NegationExpression {
                op: NegOp::Neg(minus),
                expr,
            })));
        }
        if let Ok(not) = input.try_parse::<Not>() {
            let expr = wrap(Self::parse_unary(input, allow_struct)?);
            return Ok(Self::Unary(UnaryExpression::Negation(NegationExpression {
                op: NegOp::Not(not),
                expr,
            })));
        }
        Self::parse_postfix(input, allow_struct)
    }

    fn parse_postfix(input: &mut ParseBuffer, allow_struct: bool) -> Result<Self> {
        let primary = Self::parse_primary(input, allow_struct)?;
        let mut wrapped = wrap(primary);
        loop {
            if let Ok(question) = input.try_parse::<Question>() {
                wrapped = wrap(Self::Try(TryExpression {
                    expr: wrapped,
                    question,
                }));
                continue;
            }
            // a lone `.` must not swallow the first char of `..`/`..=`
            // (range, a looser level handled outside the postfix chain)
            if !looks_like::<DotDot>(input)
                && let Ok(dot) = input.try_parse::<Dot>()
            {
                if let Ok(await_token) = input.try_parse::<Await>() {
                    wrapped = wrap(Self::Await(AwaitExpression {
                        expr: wrapped,
                        dot,
                        await_token,
                    }));
                    continue;
                }
                if let Ok(index) = input.try_parse::<LiteralNumber>() {
                    wrapped = wrap(Self::TupleIndex(TupleIndexExpression {
                        expr: wrapped,
                        dot,
                        index,
                    }));
                    continue;
                }
                if let Ok(field) = input.try_parse::<Ident>() {
                    let turbofish = input.try_parse::<(PathSep, GenericArgs)>().ok();
                    if let Ok(args) =
                        input.try_parse::<Parenthesized<Punctuated<Expression, Comma>>>()
                    {
                        wrapped = wrap(Self::MethodCall(MethodCallExpression {
                            expr: wrapped,
                            dot,
                            method: field,
                            turbofish,
                            args,
                        }));
                        continue;
                    }
                    if turbofish.is_some() {
                        return Err(Diagnostics::new_error_spanned(
                            "Expected `(...)` after turbofish in method call",
                            input.span(),
                        ));
                    }
                    wrapped = wrap(Self::Field(FieldExpression {
                        expr: wrapped,
                        dot,
                        field,
                    }));
                    continue;
                }
                return Err(Diagnostics::new_error_spanned(
                    "Expected `await`, a tuple index, or a field after `.`",
                    input.span(),
                ));
            }
            if let Ok(index) = input.try_parse::<Bracketed<Expression>>() {
                wrapped = wrap(Self::Index(IndexExpression {
                    expr: wrapped,
                    index,
                }));
                continue;
            }
            if let Ok(params) = input.try_parse::<Parenthesized<Punctuated<Expression, Comma>>>() {
                wrapped = wrap(Self::Call(CallExpression {
                    expr: wrapped,
                    params,
                }));
                continue;
            }
            break;
        }
        match wrapped {
            Expression::WithoutBlock(without_block) => Ok(*without_block),
            Expression::WithBlock(_) => {
                unreachable!("postfix parsing must keep non-block expression")
            }
        }
    }

    fn parse_primary(input: &mut ParseBuffer, allow_struct: bool) -> Result<Self> {
        if let Some(ident) = input.peek_ident() {
            #[allow(clippy::cmp_owned)]
            if ident.to_string() == "_" {
                return Ok(Self::Underscore(input.parse()?));
            }
        }
        if let Some(group) = input.peek_group() {
            return if group.delimiter() == Delimiter::Parenthesis {
                if let Ok(tuple) = input.try_parse() {
                    Ok(Self::Tuple(tuple))
                } else {
                    Ok(Self::Grouped(input.parse()?))
                }
            } else if group.delimiter() == Delimiter::Bracket {
                Ok(Self::Array(input.parse()?))
            } else {
                Err(Diagnostics::new_error_spanned(
                    "Expected an expression without block",
                    input.span(),
                ))
            };
        }
        if let Ok(literal) = input.try_parse() {
            return Ok(Self::Literal(literal));
        }
        if let Ok(macro_call) = input.try_parse() {
            return Ok(Self::MacroCall(macro_call));
        }
        if let Ok(path) = input.try_parse::<TypePath>() {
            if allow_struct
                && let Some(group) = input.peek_group()
                && group.delimiter() == Delimiter::Brace
            {
                return Ok(Self::Struct(StructExpression {
                    fields: input.parse()?,
                    path,
                }));
            }
            return Ok(Self::Path(path));
        }
        Err(Diagnostics::new_error_spanned(
            "Expected an expression without block",
            input.span(),
        ))
    }
}

impl Parse for ExpressionWithBlock {
    fn parse(input: &mut crate::parse::ParseBuffer) -> crate::error::Result<Self> {
        if let Ok(unsafe_block) = input.try_parse() {
            Ok(Self::Unsafe(unsafe_block))
        } else if let Ok(if_expr) = input.try_parse() {
            Ok(Self::If(if_expr))
        } else if let Ok(loop_expr) = input.try_parse() {
            Ok(Self::Loop(loop_expr))
        } else if let Ok(while_expr) = input.try_parse() {
            Ok(Self::While(while_expr))
        } else if let Ok(for_expr) = input.try_parse() {
            Ok(Self::For(for_expr))
        } else if let Ok(match_expr) = input.try_parse() {
            Ok(Self::Match(match_expr))
        } else if let Ok(async_block) = input.try_parse() {
            Ok(Self::Async(async_block))
        } else if let Ok(const_block) = input.try_parse() {
            Ok(Self::ConstBlock(const_block))
        } else if let Ok(block_expr) = input.try_parse() {
            Ok(Self::Block(block_expr))
        } else {
            Err(Diagnostics::new_error_spanned(
                "Expected an expression with block",
                input.span(),
            ))
        }
    }
}

impl Parse for Expression {
    fn parse(input: &mut crate::parse::ParseBuffer) -> crate::error::Result<Self> {
        if let Ok(block) = input.try_parse() {
            Ok(Self::WithBlock(Box::new(block)))
        } else if let Ok(without_block) = input.try_parse() {
            Ok(Self::WithoutBlock(without_block))
        } else {
            Err(Diagnostics::new_error_spanned(
                "Expected an expression",
                input.span(),
            ))
        }
    }
}
impl ToTokens for ExpressionWithoutBlock {
    fn to_tokens(&self, tokens: &mut crate::proc_macro::TokenStream) {
        match self {
            Self::Literal(literal) => literal.to_tokens(tokens),
            Self::Path(path) => path.to_tokens(tokens),
            Self::Await(await_expression) => await_expression.to_tokens(tokens),
            Self::Index(index_expression) => index_expression.to_tokens(tokens),
            Self::Tuple(tuple_expression) => tuple_expression.to_tokens(tokens),
            Self::TupleIndex(tuple_index_expression) => {
                tuple_index_expression.to_tokens(tokens);
            }
            Self::Field(field_expression) => field_expression.to_tokens(tokens),
            Self::Return(return_expression) => {
                return_expression.to_tokens(tokens);
            }
            Self::Continue(continue_expression) => {
                continue_expression.to_tokens(tokens);
            }
            Self::Break(break_expression) => break_expression.to_tokens(tokens),
            Self::Underscore(underscore_expression) => {
                underscore_expression.to_tokens(tokens);
            }
            Self::Grouped(grouped_expression) => {
                grouped_expression.to_tokens(tokens);
            }
            Self::Call(call_expression) => call_expression.to_tokens(tokens),
            Self::Range(range_expression) => range_expression.to_tokens(tokens),
            Self::Array(array_expression) => array_expression.to_tokens(tokens),
            Self::Unary(unary_expression) => unary_expression.to_tokens(tokens),
            Self::Binary(binary_expression) => binary_expression.to_tokens(tokens),
            Self::Cast(cast_expression) => cast_expression.to_tokens(tokens),
            Self::Assignment(assignment_expression) => assignment_expression.to_tokens(tokens),
            Self::CompoundAssignment(compound_assignment_expression) => {
                compound_assignment_expression.to_tokens(tokens);
            }
            Self::Try(try_expression) => try_expression.to_tokens(tokens),
            Self::MethodCall(method_call_expression) => method_call_expression.to_tokens(tokens),
            Self::Closure(closure_expression) => closure_expression.to_tokens(tokens),
            Self::Struct(struct_expression) => struct_expression.to_tokens(tokens),
            Self::MacroCall(macro_call_expression) => macro_call_expression.to_tokens(tokens),
        }
    }
}

impl ToTokens for ExpressionWithBlock {
    fn to_tokens(&self, tokens: &mut crate::proc_macro::TokenStream) {
        match self {
            Self::Block(block_expression) => block_expression.to_tokens(tokens),
            Self::Unsafe(unsafe_block_expression) => unsafe_block_expression.to_tokens(tokens),
            Self::Loop(loop_expression) => loop_expression.to_tokens(tokens),
            Self::If(if_expression) => if_expression.to_tokens(tokens),
            Self::While(while_expression) => while_expression.to_tokens(tokens),
            Self::For(for_expression) => for_expression.to_tokens(tokens),
            Self::Match(match_expression) => match_expression.to_tokens(tokens),
            Self::Async(async_block_expression) => async_block_expression.to_tokens(tokens),
            Self::ConstBlock(const_block_expression) => const_block_expression.to_tokens(tokens),
        }
    }
}

impl ToTokens for Expression {
    fn to_tokens(&self, tokens: &mut crate::proc_macro::TokenStream) {
        match self {
            Self::WithoutBlock(expression_without_block) => {
                expression_without_block.to_tokens(tokens);
            }
            Self::WithBlock(expression_with_block) => expression_with_block.to_tokens(tokens),
        }
    }
}

impl ToTokens for AwaitExpression {
    fn to_tokens(&self, tokens: &mut crate::proc_macro::TokenStream) {
        self.expr.to_tokens(tokens);
        self.dot.to_tokens(tokens);
        self.await_token.to_tokens(tokens);
    }
}
impl ToTokens for FieldExpression {
    fn to_tokens(&self, tokens: &mut crate::proc_macro::TokenStream) {
        self.expr.to_tokens(tokens);
        self.dot.to_tokens(tokens);
        self.field.to_tokens(tokens);
    }
}
impl ToTokens for TupleExpression {
    fn to_tokens(&self, tokens: &mut crate::proc_macro::TokenStream) {
        self.exprs.to_tokens(tokens);
    }
}
impl ToTokens for IndexExpression {
    fn to_tokens(&self, tokens: &mut crate::proc_macro::TokenStream) {
        self.expr.to_tokens(tokens);
        self.index.to_tokens(tokens);
    }
}
impl ToTokens for TupleIndexExpression {
    fn to_tokens(&self, tokens: &mut crate::proc_macro::TokenStream) {
        self.expr.to_tokens(tokens);
        self.dot.to_tokens(tokens);
        self.index.to_tokens(tokens);
    }
}
impl ToTokens for ReturnExpression {
    fn to_tokens(&self, tokens: &mut crate::proc_macro::TokenStream) {
        self.return_token.to_tokens(tokens);
        self.expr.to_tokens(tokens);
    }
}
impl ToTokens for ContinueExpression {
    fn to_tokens(&self, tokens: &mut crate::proc_macro::TokenStream) {
        self.continue_token.to_tokens(tokens);
        self.label.to_tokens(tokens);
    }
}
impl ToTokens for BreakExpression {
    fn to_tokens(&self, tokens: &mut crate::proc_macro::TokenStream) {
        self.break_token.to_tokens(tokens);
        self.label.to_tokens(tokens);
        self.expr.to_tokens(tokens);
    }
}
impl ToTokens for CallExpression {
    fn to_tokens(&self, tokens: &mut crate::proc_macro::TokenStream) {
        self.expr.to_tokens(tokens);
        self.params.to_tokens(tokens);
    }
}
impl ToTokens for RangeExpression {
    fn to_tokens(&self, tokens: &mut crate::proc_macro::TokenStream) {
        self.start.to_tokens(tokens);
        self.dot.to_tokens(tokens);
        self.dot_eq.to_tokens(tokens);
        self.end.to_tokens(tokens);
    }
}
impl ToTokens for UnderscoreExpression {
    fn to_tokens(&self, tokens: &mut crate::proc_macro::TokenStream) {
        self.underscore.to_tokens(tokens);
    }
}
impl ToTokens for GroupedExpression {
    fn to_tokens(&self, tokens: &mut crate::proc_macro::TokenStream) {
        self.group.to_tokens(tokens);
    }
}

impl Parse for ReturnExpression {
    fn parse(input: &mut crate::parse::ParseBuffer) -> crate::error::Result<Self> {
        Ok(Self {
            return_token: input.parse()?,
            expr: input.parse()?,
        })
    }
}

impl Parse for AwaitExpression {
    fn parse(input: &mut crate::parse::ParseBuffer) -> crate::error::Result<Self> {
        Ok(Self {
            expr: input.parse()?,
            dot: input.parse()?,
            await_token: input.parse()?,
        })
    }
}

impl Parse for IndexExpression {
    fn parse(input: &mut crate::parse::ParseBuffer) -> crate::error::Result<Self> {
        Ok(Self {
            expr: input.parse()?,
            index: input.parse()?,
        })
    }
}

impl Parse for TupleIndexExpression {
    fn parse(input: &mut crate::parse::ParseBuffer) -> crate::error::Result<Self> {
        Ok(Self {
            expr: input.parse()?,
            dot: input.parse()?,
            index: input.parse()?,
        })
    }
}

impl Parse for FieldExpression {
    fn parse(input: &mut crate::parse::ParseBuffer) -> crate::error::Result<Self> {
        Ok(Self {
            expr: input.parse()?,
            dot: input.parse()?,
            field: input.parse()?,
        })
    }
}

impl Parse for CallExpression {
    fn parse(input: &mut crate::parse::ParseBuffer) -> crate::error::Result<Self> {
        Ok(Self {
            expr: input.parse()?,
            params: input.parse()?,
        })
    }
}

impl Parse for RangeExpression {
    fn parse(input: &mut crate::parse::ParseBuffer) -> crate::error::Result<Self> {
        match ExpressionWithoutBlock::parse_range(input, true)? {
            ExpressionWithoutBlock::Range(range) => Ok(range),
            _ => Err(Diagnostics::new_error_spanned(
                "Expected a range expression",
                input.span(),
            )),
        }
    }
}

/// Delegates to [`ExpressionWithoutBlock::parse_assignment`] (the top of
/// the precedence chain) and unwraps the matching variant — this type is
/// only ever *constructed* by the climbing parser, but keeping a real
/// `Parse` impl makes it directly testable, matching [`RangeExpression`].
macro_rules! delegate_to_precedence_chain {
    ($ty:ty, $variant:ident, $msg:literal) => {
        impl Parse for $ty {
            fn parse(input: &mut crate::parse::ParseBuffer) -> crate::error::Result<Self> {
                match ExpressionWithoutBlock::parse_assignment(input, true)? {
                    ExpressionWithoutBlock::$variant(value) => Ok(value),
                    _ => Err(Diagnostics::new_error_spanned($msg, input.span())),
                }
            }
        }
    };
}

delegate_to_precedence_chain!(UnaryExpression, Unary, "Expected a unary expression");
delegate_to_precedence_chain!(BinaryExpression, Binary, "Expected a binary expression");
delegate_to_precedence_chain!(CastExpression, Cast, "Expected a cast expression");
delegate_to_precedence_chain!(
    AssignmentExpression,
    Assignment,
    "Expected an assignment expression"
);
delegate_to_precedence_chain!(
    CompoundAssignmentExpression,
    CompoundAssignment,
    "Expected a compound assignment expression"
);
delegate_to_precedence_chain!(TryExpression, Try, "Expected a `?` expression");
delegate_to_precedence_chain!(
    MethodCallExpression,
    MethodCall,
    "Expected a method call expression"
);

impl Parse for ClosureExpression {
    fn parse(input: &mut ParseBuffer) -> Result<Self> {
        let async_token = input.try_parse().ok();
        let move_token = input.try_parse().ok();
        let params = input.parse()?;
        let return_type: Option<(RArrow, Type)> = input.try_parse().ok();
        let body = if return_type.is_some() {
            Expression::WithBlock(Box::new(ExpressionWithBlock::Block(input.parse()?)))
        } else {
            input.parse()?
        };
        Ok(Self {
            async_token,
            move_token,
            params,
            return_type,
            body,
        })
    }
}

impl Parse for ClosureParams {
    fn parse(input: &mut ParseBuffer) -> Result<Self> {
        if let Ok(or_or) = input.try_parse::<OrOr>() {
            return Ok(Self::Empty(or_or));
        }
        Ok(Self::List(input.parse()?, input.parse()?, input.parse()?))
    }
}

impl Parse for ClosureParam {
    fn parse(input: &mut ParseBuffer) -> Result<Self> {
        Ok(Self {
            // not `Pattern::parse` — a trailing `|` here must be able to
            // mean the closure's own closing delimiter, not another
            // or-pattern alternative (see `Pattern::parse_no_top_alt`).
            pat: Pattern::parse_no_top_alt(input)?,
            ty: input.try_parse().ok(),
        })
    }
}

impl Parse for StructExprField {
    fn parse(input: &mut ParseBuffer) -> Result<Self> {
        if let Ok((ident, colon, expr)) = input.try_parse::<(Ident, Colon, Expression)>() {
            return Ok(Self::Named(ident, colon, expr));
        }
        Ok(Self::Shorthand(input.parse()?))
    }
}

impl Parse for StructExprFields {
    fn parse(input: &mut ParseBuffer) -> Result<Self> {
        let fields = input.parse()?;
        let rest = if let Ok(dot_dot) = input.try_parse::<DotDot>() {
            Some((dot_dot, Box::new(input.parse()?)))
        } else {
            None
        };
        Ok(Self { fields, rest })
    }
}

impl Parse for StructExpression {
    fn parse(input: &mut ParseBuffer) -> Result<Self> {
        Ok(Self {
            path: input.parse()?,
            fields: input.parse()?,
        })
    }
}

impl Parse for MacroCallExpression {
    fn parse(input: &mut ParseBuffer) -> Result<Self> {
        Ok(Self {
            path: input.parse()?,
            bang: input.parse()?,
            body: input.parse()?,
        })
    }
}

impl ToTokens for ClosureExpression {
    fn to_tokens(&self, tokens: &mut crate::proc_macro::TokenStream) {
        self.async_token.to_tokens(tokens);
        self.move_token.to_tokens(tokens);
        self.params.to_tokens(tokens);
        self.return_type.to_tokens(tokens);
        self.body.to_tokens(tokens);
    }
}
impl ToTokens for ClosureParams {
    fn to_tokens(&self, tokens: &mut crate::proc_macro::TokenStream) {
        match self {
            Self::Empty(or_or) => or_or.to_tokens(tokens),
            Self::List(open, params, close) => {
                open.to_tokens(tokens);
                params.to_tokens(tokens);
                close.to_tokens(tokens);
            }
        }
    }
}
impl ToTokens for ClosureParam {
    fn to_tokens(&self, tokens: &mut crate::proc_macro::TokenStream) {
        self.pat.to_tokens(tokens);
        self.ty.to_tokens(tokens);
    }
}
impl ToTokens for StructExprField {
    fn to_tokens(&self, tokens: &mut crate::proc_macro::TokenStream) {
        match self {
            Self::Named(ident, colon, expr) => {
                ident.to_tokens(tokens);
                colon.to_tokens(tokens);
                expr.to_tokens(tokens);
            }
            Self::Shorthand(ident) => ident.to_tokens(tokens),
        }
    }
}
impl ToTokens for StructExprFields {
    fn to_tokens(&self, tokens: &mut crate::proc_macro::TokenStream) {
        self.fields.to_tokens(tokens);
        self.rest.to_tokens(tokens);
    }
}
impl ToTokens for StructExpression {
    fn to_tokens(&self, tokens: &mut crate::proc_macro::TokenStream) {
        self.path.to_tokens(tokens);
        self.fields.to_tokens(tokens);
    }
}
impl ToTokens for MacroCallExpression {
    fn to_tokens(&self, tokens: &mut crate::proc_macro::TokenStream) {
        self.path.to_tokens(tokens);
        self.bang.to_tokens(tokens);
        tokens.extend(Some(self.body.clone()));
    }
}

impl ToTokens for BorrowAmp {
    fn to_tokens(&self, tokens: &mut crate::proc_macro::TokenStream) {
        match self {
            Self::Single(and) => and.to_tokens(tokens),
            Self::Double(and_and) => and_and.to_tokens(tokens),
        }
    }
}
impl ToTokens for BorrowExpression {
    fn to_tokens(&self, tokens: &mut crate::proc_macro::TokenStream) {
        self.amp.to_tokens(tokens);
        self.mutability.to_tokens(tokens);
        self.expr.to_tokens(tokens);
    }
}
impl ToTokens for DereferenceExpression {
    fn to_tokens(&self, tokens: &mut crate::proc_macro::TokenStream) {
        self.star.to_tokens(tokens);
        self.expr.to_tokens(tokens);
    }
}
impl ToTokens for NegOp {
    fn to_tokens(&self, tokens: &mut crate::proc_macro::TokenStream) {
        match self {
            Self::Neg(minus) => minus.to_tokens(tokens),
            Self::Not(not) => not.to_tokens(tokens),
        }
    }
}
impl ToTokens for NegationExpression {
    fn to_tokens(&self, tokens: &mut crate::proc_macro::TokenStream) {
        self.op.to_tokens(tokens);
        self.expr.to_tokens(tokens);
    }
}
impl ToTokens for UnaryExpression {
    fn to_tokens(&self, tokens: &mut crate::proc_macro::TokenStream) {
        match self {
            Self::Borrow(borrow) => borrow.to_tokens(tokens),
            Self::Deref(deref) => deref.to_tokens(tokens),
            Self::Negation(negation) => negation.to_tokens(tokens),
        }
    }
}
impl ToTokens for BinOp {
    fn to_tokens(&self, tokens: &mut crate::proc_macro::TokenStream) {
        match self {
            Self::Add(t) => t.to_tokens(tokens),
            Self::Sub(t) => t.to_tokens(tokens),
            Self::Mul(t) => t.to_tokens(tokens),
            Self::Div(t) => t.to_tokens(tokens),
            Self::Rem(t) => t.to_tokens(tokens),
            Self::BitAnd(t) => t.to_tokens(tokens),
            Self::BitOr(t) => t.to_tokens(tokens),
            Self::BitXor(t) => t.to_tokens(tokens),
            Self::Shl(t) => t.to_tokens(tokens),
            Self::Shr(t) => t.to_tokens(tokens),
            Self::Eq(t) => t.to_tokens(tokens),
            Self::Ne(t) => t.to_tokens(tokens),
            Self::Lt(t) => t.to_tokens(tokens),
            Self::Gt(t) => t.to_tokens(tokens),
            Self::Le(t) => t.to_tokens(tokens),
            Self::Ge(t) => t.to_tokens(tokens),
            Self::And(t) => t.to_tokens(tokens),
            Self::Or(t) => t.to_tokens(tokens),
        }
    }
}
impl ToTokens for BinaryExpression {
    fn to_tokens(&self, tokens: &mut crate::proc_macro::TokenStream) {
        self.lhs.to_tokens(tokens);
        self.op.to_tokens(tokens);
        self.rhs.to_tokens(tokens);
    }
}
impl ToTokens for CastExpression {
    fn to_tokens(&self, tokens: &mut crate::proc_macro::TokenStream) {
        self.expr.to_tokens(tokens);
        self.as_token.to_tokens(tokens);
        self.ty.to_tokens(tokens);
    }
}
impl ToTokens for AssignmentExpression {
    fn to_tokens(&self, tokens: &mut crate::proc_macro::TokenStream) {
        self.lhs.to_tokens(tokens);
        self.eq.to_tokens(tokens);
        self.rhs.to_tokens(tokens);
    }
}
impl ToTokens for CompoundAssignOp {
    fn to_tokens(&self, tokens: &mut crate::proc_macro::TokenStream) {
        match self {
            Self::Add(t) => t.to_tokens(tokens),
            Self::Sub(t) => t.to_tokens(tokens),
            Self::Mul(t) => t.to_tokens(tokens),
            Self::Div(t) => t.to_tokens(tokens),
            Self::Rem(t) => t.to_tokens(tokens),
            Self::BitAnd(t) => t.to_tokens(tokens),
            Self::BitOr(t) => t.to_tokens(tokens),
            Self::BitXor(t) => t.to_tokens(tokens),
            Self::Shl(t) => t.to_tokens(tokens),
            Self::Shr(t) => t.to_tokens(tokens),
        }
    }
}
impl ToTokens for CompoundAssignmentExpression {
    fn to_tokens(&self, tokens: &mut crate::proc_macro::TokenStream) {
        self.lhs.to_tokens(tokens);
        self.op.to_tokens(tokens);
        self.rhs.to_tokens(tokens);
    }
}
impl ToTokens for TryExpression {
    fn to_tokens(&self, tokens: &mut crate::proc_macro::TokenStream) {
        self.expr.to_tokens(tokens);
        self.question.to_tokens(tokens);
    }
}
impl ToTokens for MethodCallExpression {
    fn to_tokens(&self, tokens: &mut crate::proc_macro::TokenStream) {
        self.expr.to_tokens(tokens);
        self.dot.to_tokens(tokens);
        self.method.to_tokens(tokens);
        self.turbofish.to_tokens(tokens);
        self.args.to_tokens(tokens);
    }
}

impl Parse for UnderscoreExpression {
    fn parse(input: &mut crate::parse::ParseBuffer) -> crate::error::Result<Self> {
        let underscore: Ident = input.parse()?;
        #[allow(clippy::cmp_owned)]
        if underscore.to_string() == "_" {
            Ok(Self { underscore })
        } else {
            Err(Diagnostics::new_error_spanned(
                "Expected `_`",
                underscore.span(),
            ))
        }
    }
}

impl Parse for BreakExpression {
    fn parse(input: &mut crate::parse::ParseBuffer) -> crate::error::Result<Self> {
        Ok(Self {
            break_token: input.parse()?,
            label: input.try_parse().ok(),
            expr: input.try_parse().ok(),
        })
    }
}

impl Parse for ContinueExpression {
    fn parse(input: &mut crate::parse::ParseBuffer) -> crate::error::Result<Self> {
        Ok(Self {
            continue_token: input.parse()?,
            label: input.try_parse().ok(),
        })
    }
}
impl Parse for GroupedExpression {
    fn parse(input: &mut crate::parse::ParseBuffer) -> crate::error::Result<Self> {
        Ok(Self {
            group: input.parse()?,
        })
    }
}
impl Parse for TupleExpression {
    fn parse(input: &mut crate::parse::ParseBuffer) -> crate::error::Result<Self> {
        let exprs: Parenthesized<Punctuated<Expression, _>> = input.parse()?;

        if exprs.is_empty() || (exprs.len() == 1 && exprs.trailing().is_none()) {
            return Err(Diagnostics::new_error_spanned("", exprs.span()));
        }
        Ok(Self { exprs })
    }
}
impl ToTokens for ArrayElements {
    fn to_tokens(&self, tokens: &mut crate::proc_macro::TokenStream) {
        match self {
            Self::Repetition(expression, semicolon, expression1) => {
                expression.to_tokens(tokens);
                semicolon.to_tokens(tokens);
                expression1.to_tokens(tokens);
            }
            Self::List(punctuated) => {
                punctuated.to_tokens(tokens);
            }
        }
    }
}
impl ToTokens for ArrayExpression {
    fn to_tokens(&self, tokens: &mut crate::proc_macro::TokenStream) {
        self.exprs.to_tokens(tokens);
    }
}
impl Parse for ArrayElements {
    fn parse(input: &mut crate::parse::ParseBuffer) -> crate::error::Result<Self> {
        if input.is_empty() {
            return Ok(Self::List(Punctuated::new()));
        }
        let first = input.parse()?;
        if let Ok(semicolon) = input.peek_parse() {
            return Ok(Self::Repetition(first, semicolon, input.parse()?));
        }
        if input.is_empty() {
            Ok(Self::List(Punctuated::one(first)))
        } else {
            let comma = input.parse()?;
            let mut list: Punctuated<Expression, _> = input.parse()?;
            list.push_back((first, comma));
            Ok(Self::List(list))
        }
    }
}
impl Parse for ArrayExpression {
    fn parse(input: &mut crate::parse::ParseBuffer) -> crate::error::Result<Self> {
        Ok(Self {
            exprs: input.parse()?,
        })
    }
}

impl Parse for BlockExpression {
    fn parse(input: &mut crate::parse::ParseBuffer) -> crate::error::Result<Self> {
        Ok(Self {
            label: input.try_parse().ok(),
            block: input.parse()?,
        })
    }
}

impl Parse for UnsafeBlockExpression {
    fn parse(input: &mut crate::parse::ParseBuffer) -> crate::error::Result<Self> {
        Ok(Self {
            unsafe_token: input.parse()?,
            block: input.parse()?,
        })
    }
}

impl Parse for LoopExpression {
    fn parse(input: &mut crate::parse::ParseBuffer) -> crate::error::Result<Self> {
        Ok(Self {
            label: input.try_parse().ok(),
            loop_token: input.parse()?,
            block: input.parse()?,
        })
    }
}

impl Parse for ElseExpression {
    fn parse(input: &mut crate::parse::ParseBuffer) -> crate::error::Result<Self> {
        if let Ok(if_expression) = input.try_parse() {
            Ok(Self::If(Box::new(if_expression)))
        } else if let Ok(block) = input.try_parse() {
            Ok(Self::Block(block))
        } else {
            Err(Diagnostics::new_error_spanned(
                "Expected block expression or `if` after `else`",
                input.span(),
            ))
        }
    }
}

impl Parse for IfExpression {
    fn parse(input: &mut crate::parse::ParseBuffer) -> crate::error::Result<Self> {
        let if_token = input.parse()?;
        let condition = input.parse()?;
        let block = input.parse()?;

        let else_branch = if let Ok(else_token) = input.peek_parse::<Else>() {
            Some((else_token, input.parse()?))
        } else {
            None
        };

        Ok(Self {
            if_token,
            condition,
            block,
            else_branch,
        })
    }
}

impl ToTokens for BlockExpression {
    fn to_tokens(&self, tokens: &mut crate::proc_macro::TokenStream) {
        self.label.to_tokens(tokens);
        self.block.to_tokens(tokens);
    }
}

impl ToTokens for UnsafeBlockExpression {
    fn to_tokens(&self, tokens: &mut crate::proc_macro::TokenStream) {
        self.unsafe_token.to_tokens(tokens);
        self.block.to_tokens(tokens);
    }
}

impl ToTokens for LoopExpression {
    fn to_tokens(&self, tokens: &mut crate::proc_macro::TokenStream) {
        self.label.to_tokens(tokens);
        self.loop_token.to_tokens(tokens);
        self.block.to_tokens(tokens);
    }
}

impl ToTokens for ElseExpression {
    fn to_tokens(&self, tokens: &mut crate::proc_macro::TokenStream) {
        match self {
            Self::If(if_expression) => if_expression.to_tokens(tokens),
            Self::Block(block) => block.to_tokens(tokens),
        }
    }
}

impl ToTokens for IfExpression {
    fn to_tokens(&self, tokens: &mut crate::proc_macro::TokenStream) {
        self.if_token.to_tokens(tokens);
        self.condition.to_tokens(tokens);
        self.block.to_tokens(tokens);
        self.else_branch.to_tokens(tokens);
    }
}

impl Parse for Conditions {
    fn parse(input: &mut ParseBuffer) -> Result<Self> {
        if let Ok(let_token) = input.try_parse::<Let>() {
            let pat = input.parse()?;
            let eq = input.parse()?;
            let scrutinee = parse_restricted_scrutinee(input)?;
            return Ok(Self::Let(let_token, pat, eq, Box::new(scrutinee)));
        }
        Ok(Self::Expr(parse_restricted_scrutinee(input)?))
    }
}

impl ToTokens for Conditions {
    fn to_tokens(&self, tokens: &mut crate::proc_macro::TokenStream) {
        match self {
            Self::Let(let_token, pat, eq, expr) => {
                let_token.to_tokens(tokens);
                pat.to_tokens(tokens);
                eq.to_tokens(tokens);
                expr.to_tokens(tokens);
            }
            Self::Expr(expr) => expr.to_tokens(tokens),
        }
    }
}

impl Parse for WhileExpression {
    fn parse(input: &mut ParseBuffer) -> Result<Self> {
        Ok(Self {
            label: input.try_parse().ok(),
            while_token: input.parse()?,
            condition: input.parse()?,
            block: input.parse()?,
        })
    }
}

impl ToTokens for WhileExpression {
    fn to_tokens(&self, tokens: &mut crate::proc_macro::TokenStream) {
        self.label.to_tokens(tokens);
        self.while_token.to_tokens(tokens);
        self.condition.to_tokens(tokens);
        self.block.to_tokens(tokens);
    }
}

impl Parse for ForExpression {
    fn parse(input: &mut ParseBuffer) -> Result<Self> {
        Ok(Self {
            label: input.try_parse().ok(),
            for_token: input.parse()?,
            pat: input.parse()?,
            in_token: input.parse()?,
            expr: parse_restricted_scrutinee(input)?,
            block: input.parse()?,
        })
    }
}

impl ToTokens for ForExpression {
    fn to_tokens(&self, tokens: &mut crate::proc_macro::TokenStream) {
        self.label.to_tokens(tokens);
        self.for_token.to_tokens(tokens);
        self.pat.to_tokens(tokens);
        self.in_token.to_tokens(tokens);
        self.expr.to_tokens(tokens);
        self.block.to_tokens(tokens);
    }
}

impl Parse for MatchArm {
    fn parse(input: &mut ParseBuffer) -> Result<Self> {
        let pat = input.parse()?;
        let guard = if let Ok(if_token) = input.try_parse::<If>() {
            Some((if_token, input.parse()?))
        } else {
            None
        };
        let fat_arrow = input.parse()?;
        let body = input.parse()?;
        let comma = input.try_parse().ok();
        Ok(Self {
            pat,
            guard,
            fat_arrow,
            body,
            comma,
        })
    }
}

impl ToTokens for MatchArm {
    fn to_tokens(&self, tokens: &mut crate::proc_macro::TokenStream) {
        self.pat.to_tokens(tokens);
        self.guard.to_tokens(tokens);
        self.fat_arrow.to_tokens(tokens);
        self.body.to_tokens(tokens);
        self.comma.to_tokens(tokens);
    }
}

impl Parse for MatchExpression {
    fn parse(input: &mut ParseBuffer) -> Result<Self> {
        Ok(Self {
            match_token: input.parse()?,
            scrutinee: parse_restricted_scrutinee(input)?,
            arms: input.parse()?,
        })
    }
}

impl ToTokens for MatchExpression {
    fn to_tokens(&self, tokens: &mut crate::proc_macro::TokenStream) {
        self.match_token.to_tokens(tokens);
        self.scrutinee.to_tokens(tokens);
        self.arms.to_tokens(tokens);
    }
}

impl Parse for AsyncBlockExpression {
    fn parse(input: &mut ParseBuffer) -> Result<Self> {
        Ok(Self {
            async_token: input.parse()?,
            move_token: input.try_parse().ok(),
            block: input.parse()?,
        })
    }
}

impl ToTokens for AsyncBlockExpression {
    fn to_tokens(&self, tokens: &mut crate::proc_macro::TokenStream) {
        self.async_token.to_tokens(tokens);
        self.move_token.to_tokens(tokens);
        self.block.to_tokens(tokens);
    }
}

impl Parse for ConstBlockExpression {
    fn parse(input: &mut ParseBuffer) -> Result<Self> {
        Ok(Self {
            const_token: input.parse()?,
            block: input.parse()?,
        })
    }
}

impl ToTokens for ConstBlockExpression {
    fn to_tokens(&self, tokens: &mut crate::proc_macro::TokenStream) {
        self.const_token.to_tokens(tokens);
        self.block.to_tokens(tokens);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::tests::check;

    fn parse_check(s: &str) -> ExpressionWithoutBlock {
        check::<ExpressionWithoutBlock>(s.parse().unwrap())
    }

    #[test]
    fn mul_binds_tighter_than_add() {
        let e = parse_check("a + b * c");
        match e {
            ExpressionWithoutBlock::Binary(BinaryExpression { op: BinOp::Add(_), rhs, .. }) => {
                match *rhs_inner(&rhs) {
                    ExpressionWithoutBlock::Binary(BinaryExpression { op: BinOp::Mul(_), .. }) => {}
                    _ => panic!("expected mul on rhs of add"),
                }
            }
            other => panic!("expected top-level add, got {other:?}"),
        }
    }

    fn rhs_inner(e: &Expression) -> &ExpressionWithoutBlock {
        match e {
            Expression::WithoutBlock(b) => b,
            Expression::WithBlock(_) => panic!("expected WithoutBlock"),
        }
    }

    #[test]
    fn assignment_is_loosest_binds_range_first() {
        let e = parse_check("a = b..c");
        match e {
            ExpressionWithoutBlock::Assignment(AssignmentExpression { rhs, .. }) => {
                match rhs_inner(&rhs) {
                    ExpressionWithoutBlock::Range(_) => {}
                    other => panic!("expected range on rhs of assignment, got {other:?}"),
                }
            }
            other => panic!("expected top-level assignment, got {other:?}"),
        }
    }

    #[test]
    fn cast_is_left_associative() {
        let e = parse_check("x as i32 as i64");
        match e {
            ExpressionWithoutBlock::Cast(CastExpression { expr, .. }) => match rhs_inner(&expr) {
                ExpressionWithoutBlock::Cast(_) => {}
                other => panic!("expected nested cast, got {other:?}"),
            },
            other => panic!("expected top-level cast, got {other:?}"),
        }
    }

    #[test]
    fn double_borrow_round_trips() {
        let e = parse_check("&&x");
        match e {
            ExpressionWithoutBlock::Unary(UnaryExpression::Borrow(BorrowExpression {
                amp: BorrowAmp::Double(_),
                ..
            })) => {}
            other => panic!("expected double borrow, got {other:?}"),
        }
    }

    #[test]
    fn bitand_vs_logical_and() {
        let e = parse_check("a & b");
        assert!(matches!(
            e,
            ExpressionWithoutBlock::Binary(BinaryExpression { op: BinOp::BitAnd(_), .. })
        ));
        let e2 = parse_check("a && b");
        assert!(matches!(
            e2,
            ExpressionWithoutBlock::Binary(BinaryExpression { op: BinOp::And(_), .. })
        ));
        // space-separated: must NOT be parsed as `&&`
        let e3 = parse_check("a & &b");
        assert!(matches!(
            e3,
            ExpressionWithoutBlock::Binary(BinaryExpression { op: BinOp::BitAnd(_), .. })
        ));
    }

    #[test]
    fn compound_assign_shift() {
        let e = parse_check("a <<= b");
        assert!(matches!(
            e,
            ExpressionWithoutBlock::CompoundAssignment(CompoundAssignmentExpression {
                op: CompoundAssignOp::Shl(_),
                ..
            })
        ));
    }

    #[test]
    fn every_binary_operator_round_trips() {
        for src in [
            "a + b", "a - b", "a * b", "a / b", "a % b", "a & b", "a | b", "a ^ b", "a << b",
            "a >> b", "a == b", "a != b", "a < b", "a > b", "a <= b", "a >= b", "a && b",
            "a || b",
        ] {
            parse_check(src);
        }
    }

    #[test]
    fn every_compound_assign_operator_round_trips() {
        for src in [
            "a += b", "a -= b", "a *= b", "a /= b", "a %= b", "a &= b", "a |= b", "a ^= b",
            "a <<= b", "a >>= b", "a = b",
        ] {
            parse_check(src);
        }
    }

    #[test]
    fn every_unary_operator_round_trips() {
        for src in ["-a", "!a", "*a", "&a", "&mut a", "&&a", "&&mut a"] {
            parse_check(src);
        }
    }

    #[test]
    fn method_call_vs_field_vs_call() {
        let m = parse_check("x.foo(1)");
        assert!(matches!(m, ExpressionWithoutBlock::MethodCall(_)));
        let f = parse_check("x.foo");
        assert!(matches!(f, ExpressionWithoutBlock::Field(_)));
        let c = parse_check("x(1)");
        assert!(matches!(c, ExpressionWithoutBlock::Call(_)));
    }

    #[test]
    fn try_operator() {
        let e = parse_check("x?");
        assert!(matches!(e, ExpressionWithoutBlock::Try(_)));
    }

    #[test]
    fn range_prefix_vs_binary_precedence() {
        // `..=` must not be mis-split into `..` + `=`
        let e = parse_check("..=5");
        assert!(matches!(
            e,
            ExpressionWithoutBlock::Range(RangeExpression { dot_eq: Some(_), .. })
        ));
    }

    fn parse_check_block(s: &str) -> ExpressionWithBlock {
        check::<ExpressionWithBlock>(s.parse().unwrap())
    }

    #[test]
    fn closure_no_params() {
        let e = parse_check("|| 1");
        assert!(matches!(e, ExpressionWithoutBlock::Closure(_)));
    }

    #[test]
    fn closure_param_does_not_swallow_closing_bar_as_or_pattern() {
        // `x`'s pattern parsing must not treat the closure's closing `|`
        // as an or-pattern separator and eat into the body.
        let e = check::<ClosureExpression>("|x| x.id".parse().unwrap());
        match e.body {
            Expression::WithoutBlock(b) => assert!(matches!(*b, ExpressionWithoutBlock::Field(_))),
            Expression::WithBlock(_) => panic!("expected WithoutBlock body"),
        }
        check::<Expression>("items.find(|x| x.id == target)".parse().unwrap());
    }

    #[test]
    fn closure_with_params_and_move() {
        let e = parse_check("move |x, y: i32| x + y");
        assert!(matches!(e, ExpressionWithoutBlock::Closure(_)));
    }

    #[test]
    fn closure_with_return_type_requires_block_body() {
        let e = parse_check("|x: i32| -> i32 { x }");
        match e {
            ExpressionWithoutBlock::Closure(ClosureExpression {
                return_type: Some(_),
                body,
                ..
            }) => {
                assert!(matches!(body, Expression::WithBlock(_)));
            }
            other => panic!("expected closure with return type, got {other:?}"),
        }
    }

    #[test]
    fn async_closure() {
        let e = parse_check("async move || 1");
        assert!(matches!(e, ExpressionWithoutBlock::Closure(_)));
    }

    #[test]
    fn bitor_vs_closure_disambiguation() {
        let e = parse_check("a | b");
        assert!(matches!(
            e,
            ExpressionWithoutBlock::Binary(BinaryExpression { op: BinOp::BitOr(_), .. })
        ));
    }

    #[test]
    fn struct_expression_with_shorthand_and_rest() {
        let e = parse_check("Foo { a, b: 1, ..base }");
        assert!(matches!(e, ExpressionWithoutBlock::Struct(_)));
    }

    #[test]
    fn macro_call_expression() {
        let e = parse_check("foo!(1, 2)");
        assert!(matches!(e, ExpressionWithoutBlock::MacroCall(_)));
    }

    #[test]
    fn if_struct_literal_disambiguation() {
        // `if Foo { }` must parse `Foo` as a bare path condition, not a
        // struct literal — this is the entire reason `no_struct_literal`
        // exists.
        let e = parse_check_block("if Foo { }");
        match e {
            ExpressionWithBlock::If(IfExpression {
                condition: Conditions::Expr(cond),
                ..
            }) => {
                assert!(matches!(cond, Expression::WithoutBlock(_)));
                match cond {
                    Expression::WithoutBlock(b) => assert!(matches!(*b, ExpressionWithoutBlock::Path(_))),
                    Expression::WithBlock(_) => panic!("expected WithoutBlock"),
                }
            }
            other => panic!("expected if-expression, got {other:?}"),
        }
    }

    #[test]
    fn if_condition_allows_struct_literal_in_parens() {
        check::<IfExpression>("if (Foo { a: 1 }.a == 1) { }".parse().unwrap());
    }

    #[test]
    fn if_let_expression() {
        let e = parse_check_block("if let Some(x) = y { }");
        assert!(matches!(
            e,
            ExpressionWithBlock::If(IfExpression {
                condition: Conditions::Let(..),
                ..
            })
        ));
    }

    #[test]
    fn while_let_expression() {
        check::<WhileExpression>("while let Some(x) = y { }".parse().unwrap());
        check::<WhileExpression>("'lbl: while x < 10 { }".parse().unwrap());
    }

    #[test]
    fn for_expression() {
        check::<ForExpression>("for x in 0..10 { }".parse().unwrap());
    }

    #[test]
    fn match_expression_with_guard_and_or_pattern() {
        check::<MatchExpression>("match x { 1 | 2 => a, n if n > 2 => b, _ => c }".parse().unwrap());
    }

    #[test]
    fn async_and_const_blocks() {
        check::<AsyncBlockExpression>("async { x }".parse().unwrap());
        check::<AsyncBlockExpression>("async move { x }".parse().unwrap());
        check::<ConstBlockExpression>("const { x }".parse().unwrap());
    }

    #[test]
    fn realistic_function_body_round_trips() {
        // note: no string literals — only numeric literals are supported
        // (a pre-existing, documented gap in `ast::literal::Literal`,
        // unrelated to this expression work).
        check::<Braced<Vec<Statement>>>(
            "{
                let result = match items.iter().find(|x| x.id == target) {
                    Some(item) if item.active => Ok(item.value.clone()),
                    Some(_) => Err(1),
                    None => Err(0),
                };
                for entry in &mut list {
                    entry.count += 1;
                }
                while let Some(next) = queue.pop() {
                    process(next)?;
                }
                Point { x: 1, y: 2, ..default }
            }"
            .parse()
            .unwrap(),
        );
    }
}



