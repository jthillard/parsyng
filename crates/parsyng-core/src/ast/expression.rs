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
//! [`Expression`] itself is a thin wrapper over the two. A block-like
//! expression can still be an operand (`match x { .. } + 1`,
//! `unsafe { .. }.f()`): it then sits inside an [`ExpressionWithoutBlock`]
//! node. In statement and match-arm position a leading block-like
//! expression ends at its `}` unless a postfix `.`/`?` follows, as in
//! rustc.
//!
//! Coverage is the full stable grammar, let-chains, qualified paths
//! (`<T as Trait>::f`), raw borrows (`&raw const x`) and expression
//! attributes included. The remaining gaps: slice/range/box patterns (see
//! [`ast::pattern`](crate::ast::pattern)) and unstable syntax such as
//! `try`/`yeet` blocks. One token-level caveat: `x.0.1` (lexed with a
//! `0.1` float literal) is split into two [`TupleIndexExpression`]s and
//! re-emitted as `x.0 .1`, which means the same thing.
//!
//! [`ExpressionWithoutBlock::parse`] is a hand-written precedence-climbing
//! parser — see the doc comment on the `impl ExpressionWithoutBlock` block
//! further down for the precedence table and the technique used to
//! disambiguate operators that share a textual prefix with a longer one
//! (`&` vs `&&`, `=` vs `==`/`=>`, and so on).

use crate::{ToTokens, proc_macro::Delimiter};

use crate::{
    ast::{
        attributes::{Attribute, parse_outer_attributes},
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
            Percent, PercentEq, Plus, PlusEq, Pound, Question, RArrow, Raw, Return, Semicolon, Shl,
            ShlEq, Shr, ShrEq, Slash, SlashEq, Star, StarEq, Unsafe, While,
        },
        r#type::{Type, TypePath, TypeQualifiedPath},
    },
    combinator::{Punctuated, StopOnError},
    error::{Diagnostics, Result},
    parse::{Parse, ParseBuffer},
    proc_macro::{Group, Ident, Span, TokenTree},
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

/// An expression parsed with struct literals suppressed: used for
/// [`Conditions`] and [`ForExpression`]'s iterator expression, where a bare
/// `Path { ... }` would be ambiguous with the construct's own trailing
/// block.
fn parse_restricted_scrutinee(input: &mut ParseBuffer) -> Result<Expression> {
    ExpressionWithoutBlock::parse_top(input, false)
}

/// A const generic argument that can't be mistaken for a type: a
/// `{ ... }` block, a literal, or `-` followed by a literal.
///
/// Reference: <https://doc.rust-lang.org/reference/paths.html#paths-in-expressions>
pub(crate) fn parse_generic_const_arg(input: &mut ParseBuffer) -> Result<Expression> {
    if let Some(group) = input.peek_group()
        && group.delimiter() == Delimiter::Brace
    {
        return Ok(Expression::WithBlock(Box::new(ExpressionWithBlock::Block(
            input.parse()?,
        ))));
    }
    if let Ok(literal) = input.try_parse() {
        return Ok(wrap(ExpressionWithoutBlock::Literal(literal)));
    }
    let minus = input.parse()?;
    let literal = input.parse()?;
    Ok(wrap(ExpressionWithoutBlock::Unary(
        UnaryExpression::Negation(NegationExpression {
            op: NegOp::Neg(minus),
            expr: wrap(ExpressionWithoutBlock::Literal(literal)),
        }),
    )))
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
    /// A qualified path used as an expression, e.g. `<T as Trait>::f` or
    /// `<Vec<u8>>::new`.
    ///
    /// Reference: <https://doc.rust-lang.org/reference/paths.html#qualified-paths>
    QualifiedPath(TypeQualifiedPath),
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
    /// `return` or `return expr`.
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
    /// An expression preceded by outer attributes, e.g. `#[cfg(x)] a`.
    ///
    /// Reference: <https://doc.rust-lang.org/reference/expressions.html#expression-attributes>
    Attributed(AttributedExpression),
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
/// Either a plain (struct-literal-restricted) expression, `let PATTERN =
/// EXPR` (also restricted), or a let-chain mixing both with `&&` (`if let
/// Some(x) = a && x > 0 && let Ok(y) = f(x)`).
///
/// In a let-chain, each operand binds tighter than `&&`, so a top-level
/// range (`let x = a..b && ...`) in a `let` scrutinee must be parenthesized.
///
/// Reference: <https://doc.rust-lang.org/reference/expressions/if-expr.html>
#[derive(Clone, Debug)]
pub enum Conditions {
    /// `let PATTERN = EXPR`.
    Let(Let, Pattern, Eq, Box<Expression>),
    /// A plain expression.
    Expr(Expression),
    /// A let-chain: at least two `&&`-separated operands, at least one of
    /// which is a `let`.
    ///
    /// Reference: <https://doc.rust-lang.org/reference/expressions/if-expr.html#chains-of-conditions>
    Chain(Box<Condition>, Vec<(AndAnd, Condition)>),
}

/// One `&&`-separated operand of a let-chain ([`Conditions::Chain`]).
#[derive(Clone, Debug)]
pub enum Condition {
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
    attrs: Vec<Attribute>,
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
/// A tuple expression: `()`, `(a,)`, or `(a, b, c)`. Parsing rejects a
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

/// `return` or `return expr`.
///
/// Reference: <https://doc.rust-lang.org/reference/expressions/return-expr.html>
#[derive(Clone, Debug)]
pub struct ReturnExpression {
    return_token: Return,
    expr: Option<Expression>,
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

/// An expression preceded by outer attributes: `#[attr] expr`. The
/// attributes apply to the whole expression that follows.
///
/// Reference: <https://doc.rust-lang.org/reference/expressions.html#expression-attributes>
#[derive(Clone, Debug)]
pub struct AttributedExpression {
    attrs: Vec<Attribute>,
    expr: Expression,
}

/// `&expr`, `&mut expr`, the raw borrows `&raw const expr`/`&raw mut expr`,
/// or the double-reference shorthand `&&expr`/`&&mut expr`.
///
/// The double form keeps the original `&&` token intact (see [`BorrowAmp`])
/// rather than desugaring into two nested borrows, so it round-trips
/// exactly.
///
/// Reference: <https://doc.rust-lang.org/reference/expressions/operator-expr.html#borrow-operators>
#[derive(Clone, Debug)]
pub struct BorrowExpression {
    amp: BorrowAmp,
    /// `raw` plus `const` for `&raw const`; `raw` alone for `&raw mut`,
    /// whose `mut` is in `mutability`.
    raw: Option<(Raw, Option<Const>)>,
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

/// A struct expression: `Path { field: expr, field2, ..base }`, including
/// numeric field keys (`TupleStruct { 0: value }`).
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

/// One field inside a [`StructExprFields`] list, with its outer attributes:
/// `#[cfg(x)] field: expr`.
#[derive(Clone, Debug)]
pub struct StructExprField {
    attrs: Vec<Attribute>,
    kind: StructExprFieldKind,
}

/// The attribute-less part of a [`StructExprField`].
#[derive(Clone, Debug)]
pub enum StructExprFieldKind {
    /// `field: expr` or `0: expr`.
    Named(StructExprMember, Colon, Expression),
    /// `field` shorthand for `field: field`.
    Shorthand(Ident),
}

/// The key of a [`StructExprFieldKind::Named`] field.
#[derive(Clone, Debug)]
pub enum StructExprMember {
    /// A named field: `field`.
    Named(Ident),
    /// A tuple-struct field index: `0`.
    Unnamed(LiteralNumber),
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
        let span = input.span();
        unwrap_without_block(Self::parse_top(input, true)?, span)
    }
}

/// Extract the [`ExpressionWithoutBlock`] from a precedence-chain result,
/// rejecting a lone block-like expression (which only an [`Expression`] can
/// hold).
fn unwrap_without_block(expr: Expression, span: Span) -> Result<ExpressionWithoutBlock> {
    match expr {
        Expression::WithoutBlock(without_block) => Ok(*without_block),
        Expression::WithBlock(_) => Err(Diagnostics::new_error_spanned(
            "Expected an expression without block",
            span,
        )),
    }
}

/// True if the next token can start an [`ExpressionWithBlock`]: one of its
/// leading keywords, a `'label`, or a `{ ... }` group. A cheap filter so
/// that [`ExpressionWithoutBlock::parse_primary`] only attempts the full
/// [`ExpressionWithBlock`] parse when it has a chance to succeed.
fn starts_block_like(input: &mut ParseBuffer) -> bool {
    match input.peek() {
        Some(TokenTree::Ident(ident)) => matches!(
            ident.to_string().as_str(),
            "unsafe" | "if" | "loop" | "while" | "for" | "match" | "async" | "const"
        ),
        Some(TokenTree::Punct(punct)) => punct.as_char() == '\'',
        Some(TokenTree::Group(group)) => group.delimiter() == Delimiter::Brace,
        _ => false,
    }
}

/// True if the next token is a postfix `.` (not the start of `..`) or `?` —
/// the only operators that continue a block-like expression in statement
/// position.
pub(crate) fn continues_with_postfix(input: &ParseBuffer) -> bool {
    looks_like::<Question>(input) || (looks_like::<Dot>(input) && !looks_like::<DotDot>(input))
}

/// Parse an expression in statement or match-arm-body position, following
/// rustc: a leading block-like expression (`if`, `match`, `{ ... }`, ...)
/// ends the expression at its closing `}`, so `{ a } - 1` is two
/// statements — unless it's directly followed by a postfix `.`/`?`
/// (`match x { ... }.len()`), in which case the whole operator chain is
/// parsed.
pub(crate) fn parse_statement_like(input: &mut ParseBuffer) -> Result<Expression> {
    let mut fork = input.clone();
    if let Ok(block) = fork.parse::<ExpressionWithBlock>()
        && !continues_with_postfix(&fork)
    {
        *input = fork;
        return Ok(Expression::WithBlock(Box::new(block)));
    }
    ExpressionWithoutBlock::parse_top(input, true)
}

impl ExpressionWithoutBlock {
    /// The entry point of the precedence chain: leading outer attributes,
    /// then the loosest-binding level.
    fn parse_top(input: &mut ParseBuffer, allow_struct: bool) -> Result<Expression> {
        if looks_like::<Pound>(input) {
            let attrs = parse_outer_attributes(input);
            if !attrs.is_empty() {
                let expr = Self::parse_top(input, allow_struct)?;
                return Ok(wrap(Self::Attributed(AttributedExpression { attrs, expr })));
            }
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
/// Every level returns an [`Expression`]: a block-like expression
/// (`if`, `match`, `{ ... }`, ...) is a valid primary operand, and comes
/// back as [`Expression::WithBlock`] only when no operator follows it.
///
/// `return`/`break`/`continue` and closures are handled at the unary
/// level: they swallow the whole remaining expression as their operand,
/// so e.g. `a || return b` and `x = |y| y + 1` parse as in rustc.
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
/// callee. It is `false` for `if`/`while`/`match` scrutinees and `for`'s
/// iterator expression, where a bare `Path { ... }` would be ambiguous with
/// the construct's own trailing block; nested sub-expressions (inside
/// `(...)`, `[...]`, call/index arguments) go back through the
/// unrestricted [`Expression::parse`].
impl ExpressionWithoutBlock {
    fn parse_assignment(input: &mut ParseBuffer, allow_struct: bool) -> Result<Expression> {
        let lhs = Self::parse_range(input, allow_struct)?;
        macro_rules! compound {
            ($tok:ty, $variant:ident) => {
                if let Ok(op) = input.try_parse::<$tok>() {
                    let rhs = Self::parse_assignment(input, allow_struct)?;
                    return Ok(wrap(Self::CompoundAssignment(
                        CompoundAssignmentExpression {
                            lhs,
                            op: CompoundAssignOp::$variant(op),
                            rhs,
                        },
                    )));
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
            return Ok(wrap(Self::Assignment(AssignmentExpression {
                lhs,
                eq,
                rhs,
            })));
        }
        Ok(lhs)
    }

    /// The optional end of a range: absent if nothing parses, and — in a
    /// no-struct context — absent before a `{`, which is the enclosing
    /// construct's block (`for i in 0.. { ... }`).
    fn parse_range_end(input: &mut ParseBuffer, allow_struct: bool) -> Option<Expression> {
        if !allow_struct
            && let Some(group) = input.peek_group()
            && group.delimiter() == Delimiter::Brace
        {
            return None;
        }
        input
            .try_advance(|input| Self::parse_or(input, allow_struct))
            .ok()
    }

    /// `a..b`, `a..`, `..b`, `..`, `a..=b`, or `..=b` — sitting below `||`
    /// and above assignment; the end (like the start) is parsed one level
    /// up (`||`), not recursively, since ranges don't chain and need
    /// parens to combine with looser operators.
    fn parse_range(input: &mut ParseBuffer, allow_struct: bool) -> Result<Expression> {
        // the 3-char token must be tried before its 2-char prefix
        if let Ok(dot_eq) = input.try_parse::<DotDotEq>() {
            let end = Self::parse_range_end(input, allow_struct);
            return Ok(wrap(Self::Range(RangeExpression {
                start: None,
                dot: None,
                dot_eq: Some(dot_eq),
                end,
            })));
        }
        if let Ok(dot) = input.try_parse::<DotDot>() {
            let end = Self::parse_range_end(input, allow_struct);
            return Ok(wrap(Self::Range(RangeExpression {
                start: None,
                dot: Some(dot),
                dot_eq: None,
                end,
            })));
        }
        let lhs = Self::parse_or(input, allow_struct)?;
        if let Ok(dot_eq) = input.try_parse::<DotDotEq>() {
            let end = Self::parse_range_end(input, allow_struct);
            return Ok(wrap(Self::Range(RangeExpression {
                start: Some(lhs),
                dot: None,
                dot_eq: Some(dot_eq),
                end,
            })));
        }
        if let Ok(dot) = input.try_parse::<DotDot>() {
            let end = Self::parse_range_end(input, allow_struct);
            return Ok(wrap(Self::Range(RangeExpression {
                start: Some(lhs),
                dot: Some(dot),
                dot_eq: None,
                end,
            })));
        }
        Ok(lhs)
    }

    fn parse_or(input: &mut ParseBuffer, allow_struct: bool) -> Result<Expression> {
        let mut lhs = Self::parse_and(input, allow_struct)?;
        while let Ok(op) = input.try_parse::<OrOr>() {
            let rhs = Self::parse_and(input, allow_struct)?;
            lhs = wrap(Self::Binary(BinaryExpression {
                lhs,
                op: BinOp::Or(op),
                rhs,
            }));
        }
        Ok(lhs)
    }

    fn parse_and(input: &mut ParseBuffer, allow_struct: bool) -> Result<Expression> {
        let mut lhs = Self::parse_compare(input, allow_struct)?;
        while let Ok(op) = input.try_parse::<AndAnd>() {
            let rhs = Self::parse_compare(input, allow_struct)?;
            lhs = wrap(Self::Binary(BinaryExpression {
                lhs,
                op: BinOp::And(op),
                rhs,
            }));
        }
        Ok(lhs)
    }

    /// Comparisons don't chain (`a < b < c` is a hard error in Rust), so
    /// this parses at most one, not a loop.
    fn parse_compare(input: &mut ParseBuffer, allow_struct: bool) -> Result<Expression> {
        let lhs = Self::parse_bitor(input, allow_struct)?;
        macro_rules! cmp {
            ($tok:ty, $variant:ident) => {
                if let Ok(op) = input.try_parse::<$tok>() {
                    let rhs = Self::parse_bitor(input, allow_struct)?;
                    return Ok(wrap(Self::Binary(BinaryExpression {
                        lhs,
                        op: BinOp::$variant(op),
                        rhs,
                    })));
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
            return Ok(wrap(Self::Binary(BinaryExpression {
                lhs,
                op: BinOp::Lt(op),
                rhs,
            })));
        }
        if !looks_like::<Shr>(input)
            && let Ok(op) = input.try_parse::<Gt>()
        {
            let rhs = Self::parse_bitor(input, allow_struct)?;
            return Ok(wrap(Self::Binary(BinaryExpression {
                lhs,
                op: BinOp::Gt(op),
                rhs,
            })));
        }
        Ok(lhs)
    }

    fn parse_bitor(input: &mut ParseBuffer, allow_struct: bool) -> Result<Expression> {
        let mut lhs = Self::parse_bitxor(input, allow_struct)?;
        loop {
            if looks_like::<OrOr>(input) || looks_like::<OrEq>(input) {
                break;
            }
            if let Ok(op) = input.try_parse::<Or>() {
                let rhs = Self::parse_bitxor(input, allow_struct)?;
                lhs = wrap(Self::Binary(BinaryExpression {
                    lhs,
                    op: BinOp::BitOr(op),
                    rhs,
                }));
                continue;
            }
            break;
        }
        Ok(lhs)
    }

    fn parse_bitxor(input: &mut ParseBuffer, allow_struct: bool) -> Result<Expression> {
        let mut lhs = Self::parse_bitand(input, allow_struct)?;
        loop {
            if looks_like::<CaretEq>(input) {
                break;
            }
            if let Ok(op) = input.try_parse::<Caret>() {
                let rhs = Self::parse_bitand(input, allow_struct)?;
                lhs = wrap(Self::Binary(BinaryExpression {
                    lhs,
                    op: BinOp::BitXor(op),
                    rhs,
                }));
                continue;
            }
            break;
        }
        Ok(lhs)
    }

    fn parse_bitand(input: &mut ParseBuffer, allow_struct: bool) -> Result<Expression> {
        let mut lhs = Self::parse_shift(input, allow_struct)?;
        loop {
            if looks_like::<AndAnd>(input) || looks_like::<AndEq>(input) {
                break;
            }
            if let Ok(op) = input.try_parse::<And>() {
                let rhs = Self::parse_shift(input, allow_struct)?;
                lhs = wrap(Self::Binary(BinaryExpression {
                    lhs,
                    op: BinOp::BitAnd(op),
                    rhs,
                }));
                continue;
            }
            break;
        }
        Ok(lhs)
    }

    fn parse_shift(input: &mut ParseBuffer, allow_struct: bool) -> Result<Expression> {
        let mut lhs = Self::parse_additive(input, allow_struct)?;
        loop {
            macro_rules! op {
                ($guard:ty, $tok:ty, $variant:ident) => {
                    if !looks_like::<$guard>(input)
                        && let Ok(op) = input.try_parse::<$tok>()
                    {
                        let rhs = Self::parse_additive(input, allow_struct)?;
                        lhs = wrap(Self::Binary(BinaryExpression {
                            lhs,
                            op: BinOp::$variant(op),
                            rhs,
                        }));
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

    fn parse_additive(input: &mut ParseBuffer, allow_struct: bool) -> Result<Expression> {
        let mut lhs = Self::parse_multiplicative(input, allow_struct)?;
        loop {
            macro_rules! op {
                ($guard:ty, $tok:ty, $variant:ident) => {
                    if !looks_like::<$guard>(input)
                        && let Ok(op) = input.try_parse::<$tok>()
                    {
                        let rhs = Self::parse_multiplicative(input, allow_struct)?;
                        lhs = wrap(Self::Binary(BinaryExpression {
                            lhs,
                            op: BinOp::$variant(op),
                            rhs,
                        }));
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

    fn parse_multiplicative(input: &mut ParseBuffer, allow_struct: bool) -> Result<Expression> {
        let mut lhs = Self::parse_cast(input, allow_struct)?;
        loop {
            macro_rules! op {
                ($guard:ty, $tok:ty, $variant:ident) => {
                    if !looks_like::<$guard>(input)
                        && let Ok(op) = input.try_parse::<$tok>()
                    {
                        let rhs = Self::parse_cast(input, allow_struct)?;
                        lhs = wrap(Self::Binary(BinaryExpression {
                            lhs,
                            op: BinOp::$variant(op),
                            rhs,
                        }));
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

    fn parse_cast(input: &mut ParseBuffer, allow_struct: bool) -> Result<Expression> {
        let mut expr = Self::parse_unary(input, allow_struct)?;
        while let Ok(as_token) = input.try_parse::<As>() {
            let ty = input.parse()?;
            expr = wrap(Self::Cast(CastExpression { expr, as_token, ty }));
        }
        Ok(expr)
    }

    /// The borrow modifiers after `&`/`&&`: `raw const`, `raw mut`, or
    /// just `mut`. `raw` alone is a plain borrow of a variable named `raw`.
    fn parse_borrow_modifiers(
        input: &mut ParseBuffer,
    ) -> (Option<(Raw, Option<Const>)>, Option<Mut>) {
        if let Ok((raw, const_token)) = input.try_parse::<(Raw, Const)>() {
            return (Some((raw, Some(const_token))), None);
        }
        if let Ok((raw, mut_token)) = input.try_parse::<(Raw, Mut)>() {
            return (Some((raw, None)), Some(mut_token));
        }
        (None, input.try_parse().ok())
    }

    fn parse_unary(input: &mut ParseBuffer, allow_struct: bool) -> Result<Expression> {
        // `return`/`break`/`continue`/closures swallow an optional or
        // required trailing expression, so they take no postfix or binary
        // operator of their own.
        if let Some(ident) = input.peek_ident() {
            match ident.to_string().as_str() {
                "return" => return Ok(wrap(Self::Return(input.parse()?))),
                "break" => return Ok(wrap(Self::Break(input.parse()?))),
                "continue" => return Ok(wrap(Self::Continue(input.parse()?))),
                "move" => return Ok(wrap(Self::Closure(input.parse()?))),
                // `async { ... }`/`async move { ... }` is a block, parsed
                // as a primary below; anything else is an async closure.
                "async" => {
                    if let Ok(closure) = input.try_parse() {
                        return Ok(wrap(Self::Closure(closure)));
                    }
                }
                _ => {}
            }
        }
        if looks_like::<OrOr>(input) || looks_like::<Or>(input) {
            return Ok(wrap(Self::Closure(input.parse()?)));
        }
        // `&&expr` — a single `&&` token means two levels of borrow; kept
        // as one `BorrowAmp::Double` node (not two nested `Borrow`s) so it
        // round-trips as `&&`, not a synthesized `& &`.
        if let Ok(and_and) = input.try_parse::<AndAnd>() {
            let (raw, mutability) = Self::parse_borrow_modifiers(input);
            let expr = Self::parse_unary(input, allow_struct)?;
            return Ok(wrap(Self::Unary(UnaryExpression::Borrow(
                BorrowExpression {
                    amp: BorrowAmp::Double(and_and),
                    raw,
                    mutability,
                    expr,
                },
            ))));
        }
        if let Ok(and_token) = input.try_parse::<And>() {
            let (raw, mutability) = Self::parse_borrow_modifiers(input);
            let expr = Self::parse_unary(input, allow_struct)?;
            return Ok(wrap(Self::Unary(UnaryExpression::Borrow(
                BorrowExpression {
                    amp: BorrowAmp::Single(and_token),
                    raw,
                    mutability,
                    expr,
                },
            ))));
        }
        if let Ok(star) = input.try_parse::<Star>() {
            let expr = Self::parse_unary(input, allow_struct)?;
            return Ok(wrap(Self::Unary(UnaryExpression::Deref(
                DereferenceExpression { star, expr },
            ))));
        }
        if let Ok(minus) = input.try_parse::<Minus>() {
            let expr = Self::parse_unary(input, allow_struct)?;
            return Ok(wrap(Self::Unary(UnaryExpression::Negation(
                NegationExpression {
                    op: NegOp::Neg(minus),
                    expr,
                },
            ))));
        }
        if let Ok(not) = input.try_parse::<Not>() {
            let expr = Self::parse_unary(input, allow_struct)?;
            return Ok(wrap(Self::Unary(UnaryExpression::Negation(
                NegationExpression {
                    op: NegOp::Not(not),
                    expr,
                },
            ))));
        }
        Self::parse_postfix(input, allow_struct)
    }

    /// `x.0.1` lexes as `x`, `.`, `0.1` (a float literal): if the next
    /// token is such a `<digits>.<digits>` float, consume it and split it
    /// into its two tuple indices plus a synthesized `.`, all spanned at
    /// the original literal.
    fn parse_nested_tuple_index(
        input: &mut ParseBuffer,
    ) -> Option<(LiteralNumber, Dot, LiteralNumber)> {
        let literal = input.peek_literal()?;
        let text = literal.to_string();
        let span = literal.span();
        let (first, second) = text.split_once('.')?;
        let is_index = |part: &str| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit());
        if !is_index(first) || !is_index(second) {
            return None;
        }
        let first = LiteralNumber::from_digits(first, span);
        let second = LiteralNumber::from_digits(second, span);
        input.literal();
        Some((first, Dot::new(span), second))
    }

    fn parse_postfix(input: &mut ParseBuffer, allow_struct: bool) -> Result<Expression> {
        let mut wrapped = Self::parse_primary(input, allow_struct)?;
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
                if let Some((first, inner_dot, second)) = Self::parse_nested_tuple_index(input) {
                    let inner = wrap(Self::TupleIndex(TupleIndexExpression {
                        expr: wrapped,
                        dot,
                        index: first,
                    }));
                    wrapped = wrap(Self::TupleIndex(TupleIndexExpression {
                        expr: inner,
                        dot: inner_dot,
                        index: second,
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
        Ok(wrapped)
    }

    fn parse_primary(input: &mut ParseBuffer, allow_struct: bool) -> Result<Expression> {
        if starts_block_like(input)
            && let Ok(block) = input.try_parse::<ExpressionWithBlock>()
        {
            return Ok(Expression::WithBlock(Box::new(block)));
        }
        if let Some(ident) = input.peek_ident() {
            #[allow(clippy::cmp_owned)]
            if ident.to_string() == "_" {
                return Ok(wrap(Self::Underscore(input.parse()?)));
            }
        }
        if let Some(group) = input.peek_group() {
            return if group.delimiter() == Delimiter::Parenthesis {
                if let Ok(tuple) = input.try_parse() {
                    Ok(wrap(Self::Tuple(tuple)))
                } else {
                    Ok(wrap(Self::Grouped(input.parse()?)))
                }
            } else if group.delimiter() == Delimiter::Bracket {
                Ok(wrap(Self::Array(input.parse()?)))
            } else {
                Err(Diagnostics::new_error_spanned(
                    "Expected an expression",
                    input.span(),
                ))
            };
        }
        if let Ok(literal) = input.try_parse() {
            return Ok(wrap(Self::Literal(literal)));
        }
        if let Ok(macro_call) = input.try_parse() {
            return Ok(wrap(Self::MacroCall(macro_call)));
        }
        if looks_like::<Lt>(input) {
            return Ok(wrap(Self::QualifiedPath(
                TypeQualifiedPath::parse_expression(input)?,
            )));
        }
        if let Ok(path) = input.try_advance(TypePath::parse_expression) {
            if allow_struct
                && let Some(group) = input.peek_group()
                && group.delimiter() == Delimiter::Brace
            {
                return Ok(wrap(Self::Struct(StructExpression {
                    fields: input.parse()?,
                    path,
                })));
            }
            return Ok(wrap(Self::Path(path)));
        }
        Err(Diagnostics::new_error_spanned(
            "Expected an expression",
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

/// Parses the whole precedence chain: a lone block-like expression comes
/// back as [`Expression::WithBlock`], anything else (including a
/// block-like expression used as an operand, e.g. `match x { .. } + 1`) as
/// [`Expression::WithoutBlock`].
impl Parse for Expression {
    fn parse(input: &mut crate::parse::ParseBuffer) -> crate::error::Result<Self> {
        ExpressionWithoutBlock::parse_top(input, true)
    }
}
impl ToTokens for ExpressionWithoutBlock {
    fn to_tokens(&self, tokens: &mut crate::proc_macro::TokenStream) {
        match self {
            Self::Literal(literal) => literal.to_tokens(tokens),
            Self::Path(path) => path.to_tokens(tokens),
            Self::QualifiedPath(path) => path.to_tokens(tokens),
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
            Self::Attributed(attributed_expression) => attributed_expression.to_tokens(tokens),
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
            expr: input.try_parse().ok(),
        })
    }
}

impl Parse for RangeExpression {
    fn parse(input: &mut crate::parse::ParseBuffer) -> crate::error::Result<Self> {
        if let Expression::WithoutBlock(expr) = ExpressionWithoutBlock::parse_range(input, true)?
            && let ExpressionWithoutBlock::Range(range) = *expr
        {
            return Ok(range);
        }
        Err(Diagnostics::new_error_spanned(
            "Expected a range expression",
            input.span(),
        ))
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
                if let Expression::WithoutBlock(expr) =
                    ExpressionWithoutBlock::parse_assignment(input, true)?
                    && let ExpressionWithoutBlock::$variant(value) = *expr
                {
                    return Ok(value);
                }
                Err(Diagnostics::new_error_spanned($msg, input.span()))
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
delegate_to_precedence_chain!(AwaitExpression, Await, "Expected an `.await` expression");
delegate_to_precedence_chain!(IndexExpression, Index, "Expected an index expression");
delegate_to_precedence_chain!(
    TupleIndexExpression,
    TupleIndex,
    "Expected a tuple index expression"
);
delegate_to_precedence_chain!(FieldExpression, Field, "Expected a field expression");
delegate_to_precedence_chain!(CallExpression, Call, "Expected a call expression");

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
        Ok(Self {
            attrs: parse_outer_attributes(input),
            kind: input.parse()?,
        })
    }
}

impl Parse for StructExprFieldKind {
    fn parse(input: &mut ParseBuffer) -> Result<Self> {
        if let Ok((member, colon, expr)) = input.try_parse() {
            return Ok(Self::Named(member, colon, expr));
        }
        Ok(Self::Shorthand(input.parse()?))
    }
}

impl Parse for StructExprMember {
    fn parse(input: &mut ParseBuffer) -> Result<Self> {
        if let Ok(ident) = input.try_parse() {
            return Ok(Self::Named(ident));
        }
        Ok(Self::Unnamed(input.parse()?))
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
        self.attrs.to_tokens(tokens);
        self.kind.to_tokens(tokens);
    }
}
impl ToTokens for StructExprFieldKind {
    fn to_tokens(&self, tokens: &mut crate::proc_macro::TokenStream) {
        match self {
            Self::Named(member, colon, expr) => {
                member.to_tokens(tokens);
                colon.to_tokens(tokens);
                expr.to_tokens(tokens);
            }
            Self::Shorthand(ident) => ident.to_tokens(tokens),
        }
    }
}
impl ToTokens for StructExprMember {
    fn to_tokens(&self, tokens: &mut crate::proc_macro::TokenStream) {
        match self {
            Self::Named(ident) => ident.to_tokens(tokens),
            Self::Unnamed(index) => index.to_tokens(tokens),
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
        self.raw.to_tokens(tokens);
        self.mutability.to_tokens(tokens);
        self.expr.to_tokens(tokens);
    }
}
impl ToTokens for AttributedExpression {
    fn to_tokens(&self, tokens: &mut crate::proc_macro::TokenStream) {
        self.attrs.to_tokens(tokens);
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

        // a single element without a trailing comma is a grouped
        // expression, not a 1-tuple; `()` is the (empty) unit tuple
        if exprs.len() == 1 && exprs.trailing().is_some() {
            return Err(Diagnostics::new_error_spanned(
                "Expected a tuple expression",
                exprs.span(),
            ));
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
        if has_top_level_let(input)
            && let Ok(chain) = input.try_advance(parse_let_chain)
        {
            return Ok(chain);
        }
        Ok(Self::Expr(parse_restricted_scrutinee(input)?))
    }
}

/// Whether a `let` keyword appears in the condition starting at `input`,
/// i.e. before the first `{ ... }` group (the `if`/`while` body).
#[allow(clippy::cmp_owned)]
fn has_top_level_let(input: &ParseBuffer) -> bool {
    for token in input.clone() {
        match token {
            TokenTree::Ident(ident) if ident.to_string() == "let" => return true,
            TokenTree::Group(group) if group.delimiter() == Delimiter::Brace => return false,
            _ => {}
        }
    }
    false
}

/// Parse one operand of a let-chain, binding tighter than `&&`.
fn parse_condition(input: &mut ParseBuffer) -> Result<Condition> {
    if let Ok(let_token) = input.try_parse::<Let>() {
        let pat = input.parse()?;
        let eq = input.parse()?;
        let scrutinee = ExpressionWithoutBlock::parse_compare(input, false)?;
        return Ok(Condition::Let(let_token, pat, eq, Box::new(scrutinee)));
    }
    Ok(Condition::Expr(ExpressionWithoutBlock::parse_compare(
        input, false,
    )?))
}

/// Parse a condition containing at least one `let`, failing otherwise so
/// that plain expressions go through the full expression parser.
fn parse_let_chain(input: &mut ParseBuffer) -> Result<Conditions> {
    let first = parse_condition(input)?;
    let mut rest = vec![];
    while let Ok(and) = input.try_parse::<AndAnd>() {
        rest.push((and, parse_condition(input)?));
    }
    let is_let = |condition: &Condition| matches!(condition, Condition::Let(..));
    match first {
        Condition::Let(let_token, pat, eq, scrutinee) if rest.is_empty() => {
            Ok(Conditions::Let(let_token, pat, eq, scrutinee))
        }
        first if is_let(&first) || rest.iter().any(|(_, condition)| is_let(condition)) => {
            Ok(Conditions::Chain(Box::new(first), rest))
        }
        _ => Err(Diagnostics::new_error_spanned(
            "Expected a `let` condition",
            input.span(),
        )),
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
            Self::Chain(first, rest) => {
                first.to_tokens(tokens);
                for (and, condition) in rest {
                    and.to_tokens(tokens);
                    condition.to_tokens(tokens);
                }
            }
        }
    }
}

impl ToTokens for Condition {
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
        let attrs = parse_outer_attributes(input);
        let pat = input.parse()?;
        let guard = if let Ok(if_token) = input.try_parse::<If>() {
            Some((if_token, input.parse()?))
        } else {
            None
        };
        let fat_arrow = input.parse()?;
        let body = parse_statement_like(input)?;
        let comma = input.try_parse().ok();
        Ok(Self {
            attrs,
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
        self.attrs.to_tokens(tokens);
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
    use crate::ast::statements::Statement;
    use crate::ast::tests::check;

    fn parse_check(s: &str) -> ExpressionWithoutBlock {
        check::<ExpressionWithoutBlock>(s.parse().unwrap())
    }

    #[test]
    fn mul_binds_tighter_than_add() {
        let e = parse_check("a + b * c");
        match e {
            ExpressionWithoutBlock::Binary(BinaryExpression {
                op: BinOp::Add(_),
                rhs,
                ..
            }) => match *rhs_inner(&rhs) {
                ExpressionWithoutBlock::Binary(BinaryExpression {
                    op: BinOp::Mul(_), ..
                }) => {}
                _ => panic!("expected mul on rhs of add"),
            },
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
            ExpressionWithoutBlock::Binary(BinaryExpression {
                op: BinOp::BitAnd(_),
                ..
            })
        ));
        let e2 = parse_check("a && b");
        assert!(matches!(
            e2,
            ExpressionWithoutBlock::Binary(BinaryExpression {
                op: BinOp::And(_),
                ..
            })
        ));
        // space-separated: must NOT be parsed as `&&`
        let e3 = parse_check("a & &b");
        assert!(matches!(
            e3,
            ExpressionWithoutBlock::Binary(BinaryExpression {
                op: BinOp::BitAnd(_),
                ..
            })
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
            "a >> b", "a == b", "a != b", "a < b", "a > b", "a <= b", "a >= b", "a && b", "a || b",
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
            ExpressionWithoutBlock::Range(RangeExpression {
                dot_eq: Some(_),
                ..
            })
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
            ExpressionWithoutBlock::Binary(BinaryExpression {
                op: BinOp::BitOr(_),
                ..
            })
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
                    Expression::WithoutBlock(b) => {
                        assert!(matches!(*b, ExpressionWithoutBlock::Path(_)));
                    }
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
    fn let_chains() {
        let e = parse_check_block("if let Some(x) = a && x > 0 && let Ok(y) = f(x) { }");
        assert!(matches!(
            e,
            ExpressionWithBlock::If(IfExpression {
                condition: Conditions::Chain(..),
                ..
            })
        ));
        check::<IfExpression>("if a && let Some(x) = b { }".parse().unwrap());
        check::<WhileExpression>(
            "while let Some(x) = it.next() && x != 0 { }"
                .parse()
                .unwrap(),
        );
        // No `let`: an ordinary expression, `||` included.
        let e = parse_check_block("if a && b || c { }");
        assert!(matches!(
            e,
            ExpressionWithBlock::If(IfExpression {
                condition: Conditions::Expr(_),
                ..
            })
        ));
        // A `let` inside the body doesn't make the condition a chain.
        check::<IfExpression>("if a && b { let x = 1; }".parse().unwrap());
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
        check::<MatchExpression>(
            "match x { 1 | 2 => a, n if n > 2 => b, _ => c }"
                .parse()
                .unwrap(),
        );
    }

    #[test]
    fn async_and_const_blocks() {
        check::<AsyncBlockExpression>("async { x }".parse().unwrap());
        check::<AsyncBlockExpression>("async move { x }".parse().unwrap());
        check::<ConstBlockExpression>("const { x }".parse().unwrap());
    }

    #[test]
    fn realistic_function_body_round_trips() {
        check::<Braced<Vec<Statement>>>(
            r#"{
                let result = match items.iter().find(|x| x.id == target) {
                    Some(item) if item.active => Ok(item.value.clone()),
                    Some(_) => Err("inactive"),
                    None => Err(r"not found"),
                };
                for entry in &mut list {
                    entry.count += 1;
                }
                while let Some(next) = queue.pop() {
                    process(next)?;
                }
                let byte = if c == 'a' { b'a' } else { b"xyz"[0] };
                Point { x: 1, y: 2, ..default }
            }"#
            .parse()
            .unwrap(),
        );
    }

    fn expr_check(s: &str) -> Expression {
        check::<Expression>(s.parse().unwrap())
    }

    fn without_block(e: &Expression) -> &ExpressionWithoutBlock {
        rhs_inner(e)
    }

    #[test]
    fn block_like_expressions_as_operands() {
        for src in [
            "if a { 1 } else { 2 } + 3",
            "a + if c { 1 } else { 2 }",
            "x = match y { _ => 1 }",
            "x = loop { break 1 }",
            "! unsafe { f () }",
            "& { x }",
            "- { x }",
            "unsafe { f () }.bar ()",
            "match x { _ => v }.len ()",
            "{ x }.len ()",
            "async { 1 }.await",
            "const { 1 } + 1",
            "foo (match x { _ => 1 })",
        ] {
            assert!(
                matches!(expr_check(src), Expression::WithoutBlock(_)),
                "{src}"
            );
        }
        assert!(matches!(
            expr_check("if a { 1 } else { 2 }"),
            Expression::WithBlock(_)
        ));
        let e = expr_check("if a { 1 } else { 2 } + 3");
        assert!(matches!(
            without_block(&e),
            ExpressionWithoutBlock::Binary(BinaryExpression {
                lhs: Expression::WithBlock(_),
                ..
            })
        ));
        // a lone block-like expression is not an `ExpressionWithoutBlock`
        assert!(
            ParseBuffer::new("{ x }".parse().unwrap())
                .parse::<ExpressionWithoutBlock>()
                .is_err()
        );
    }

    #[test]
    fn no_struct_context_stops_before_body() {
        check::<ExpressionWithBlock>("for i in 0 .. { }".parse().unwrap());
        check::<ExpressionWithBlock>("for i in .. { }".parse().unwrap());
        check::<ExpressionWithBlock>("if x == y { }".parse().unwrap());
        check::<ExpressionWithBlock>("while a < b { }".parse().unwrap());
        check::<ExpressionWithBlock>("match x { }".parse().unwrap());
    }

    fn statements(s: &str) -> Vec<Statement> {
        crate::ast::tests::parse_exact::<Vec<Statement>>(s.parse().unwrap())
    }

    #[test]
    fn block_like_statement_ends_at_its_block() {
        let stmts = statements("{ 1 } - 1");
        assert_eq!(stmts.len(), 2);
        assert!(matches!(stmts[0], Statement::ExpressionWithBlock(_, None)));
        let stmts = statements("if a { } else { } * x");
        assert_eq!(stmts.len(), 2);
        let stmts = statements("match x { }.len () ;");
        assert_eq!(stmts.len(), 1);
        assert!(matches!(
            stmts[0],
            Statement::ExpressionWithoutBlock(ExpressionWithoutBlock::MethodCall(_), Some(_))
        ));
        let stmts = statements("unsafe { f () } ? ;");
        assert!(matches!(
            stmts[..],
            [Statement::ExpressionWithoutBlock(
                ExpressionWithoutBlock::Try(_),
                Some(_)
            )]
        ));
    }

    #[test]
    fn match_arm_block_bodies() {
        let e = parse_check_block(
            "match x { A => { } B => { } - 1 => 2 , _ => if a { 1 } else { 2 } }",
        );
        let ExpressionWithBlock::Match(m) = e else {
            panic!("expected match");
        };
        assert_eq!(m.arms.len(), 4);
        parse_check_block("match x { _ => match y { } . len () , }");
        parse_check_block("match x { # [cfg (a)] A => 1 , _ => 2 }");
    }

    #[test]
    fn tuples_unit_and_single() {
        assert!(matches!(
            parse_check("()"),
            ExpressionWithoutBlock::Tuple(_)
        ));
        assert!(matches!(
            parse_check("(1 ,)"),
            ExpressionWithoutBlock::Tuple(_)
        ));
        assert!(matches!(
            parse_check("(1)"),
            ExpressionWithoutBlock::Grouped(_)
        ));
    }

    #[test]
    fn bare_return() {
        assert!(matches!(
            parse_check("return"),
            ExpressionWithoutBlock::Return(ReturnExpression { expr: None, .. })
        ));
        parse_check("a || return b");
        parse_check("x = | y | y + 1");
    }

    #[test]
    fn expression_paths_need_turbofish() {
        let ExpressionWithoutBlock::Tuple(tuple) = parse_check("(a < b , c > d)") else {
            panic!("expected tuple");
        };
        assert_eq!(tuple.exprs.len(), 2);
        assert!(matches!(
            parse_check("f (a < b , c > d)"),
            ExpressionWithoutBlock::Call(_)
        ));
        parse_check("Vec :: < u8 > :: new ()");
        parse_check("Foo :: < T > { a : 1 }");
        parse_check("f :: < { N + 1 } > ()");
        parse_check("f :: < 3 , - 1 , 'a , T > ()");
    }

    #[test]
    fn qualified_path_expressions() {
        assert!(matches!(
            parse_check("< T as Trait > :: f"),
            ExpressionWithoutBlock::QualifiedPath(_)
        ));
        parse_check("< T > :: f");
        parse_check("< Vec < u8 > as Default > :: default ()");
        parse_check("< T as Trait > :: f :: < U > (x) < y");
    }

    #[test]
    fn raw_borrows() {
        let ExpressionWithoutBlock::Unary(UnaryExpression::Borrow(b)) =
            parse_check("& raw const x")
        else {
            panic!("expected borrow");
        };
        assert!(matches!(b.raw, Some((_, Some(_)))));
        let ExpressionWithoutBlock::Unary(UnaryExpression::Borrow(b)) = parse_check("& raw mut x")
        else {
            panic!("expected borrow");
        };
        assert!(matches!(b.raw, Some((_, None))) && b.mutability.is_some());
        // `raw` on its own is just a variable
        let ExpressionWithoutBlock::Unary(UnaryExpression::Borrow(b)) = parse_check("& raw") else {
            panic!("expected borrow");
        };
        assert!(b.raw.is_none());
        parse_check("& raw . len ()");
    }

    #[test]
    fn nested_tuple_index() {
        let mut input = ParseBuffer::new("x.0.1".parse().unwrap());
        let e = input.parse::<ExpressionWithoutBlock>().unwrap();
        assert!(input.is_empty());
        let ExpressionWithoutBlock::TupleIndex(outer) = e else {
            panic!("expected tuple index");
        };
        assert_eq!(outer.index.content(), "1");
        let ExpressionWithoutBlock::TupleIndex(inner) = without_block(&outer.expr) else {
            panic!("expected nested tuple index");
        };
        assert_eq!(inner.index.content(), "0");
        assert_eq!(e_to_string(&outer), "x . 0 . 1");
    }

    fn e_to_string(e: &impl ToTokens) -> String {
        let mut out = crate::proc_macro::TokenStream::new();
        e.to_tokens(&mut out);
        out.to_string()
    }

    #[test]
    fn attributes_on_expressions() {
        assert!(matches!(
            parse_check("# [attr] x"),
            ExpressionWithoutBlock::Attributed(_)
        ));
        parse_check("[# [a] 1 , 2]");
        parse_check("f (# [a] x)");
        parse_check("(# [a] x ,)");
        parse_check("Foo { # [cfg (x)] a : 1 , # [cfg (y)] b }");
        let stmts = statements("# [cfg (x)] { } # [allow (y)] f () ;");
        assert_eq!(stmts.len(), 2);
    }

    #[test]
    fn numeric_struct_fields() {
        parse_check("Foo { 0 : a , 1 : b }");
    }
}
