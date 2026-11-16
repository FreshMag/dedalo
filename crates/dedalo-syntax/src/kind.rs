/// Every token and node kind of the syntax tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u16)]
pub enum SyntaxKind {
    // Tokens.
    Whitespace,
    Newline,
    /// `# ...`, `// ...` or `/* ... */`.
    Comment,
    Ident,
    Number,
    Str,
    Heredoc,
    LBrace,
    RBrace,
    LBrack,
    RBrack,
    LParen,
    RParen,
    Eq,
    Comma,
    Dot,
    /// A character outside the version 1 subset, such as an operator.
    Unknown,
    /// End of input. Never appears in a tree.
    Eof,
    // Nodes.
    Root,
    Block,
    Label,
    Body,
    Attribute,
    /// Number, string, heredoc, `true`, `false` or `null`.
    Literal,
    /// A bare identifier used as a value, dotted when it comes from a plugin.
    Symbol,
    List,
    Object,
    Entry,
    Call,
    Error,
}

const ALL: [SyntaxKind; 30] = {
    use SyntaxKind::*;
    [
        Whitespace, Newline, Comment, Ident, Number, Str, Heredoc, LBrace, RBrace, LBrack, RBrack,
        LParen, RParen, Eq, Comma, Dot, Unknown, Eof, Root, Block, Label, Body, Attribute, Literal,
        Symbol, List, Object, Entry, Call, Error,
    ]
};

impl SyntaxKind {
    pub fn is_trivia(self) -> bool {
        matches!(
            self,
            SyntaxKind::Whitespace | SyntaxKind::Newline | SyntaxKind::Comment
        )
    }

    /// True for the node kinds that can stand as an attribute value.
    pub fn is_expr(self) -> bool {
        matches!(
            self,
            SyntaxKind::Literal
                | SyntaxKind::Symbol
                | SyntaxKind::List
                | SyntaxKind::Object
                | SyntaxKind::Call
        )
    }
}

impl From<SyntaxKind> for rowan::SyntaxKind {
    fn from(kind: SyntaxKind) -> Self {
        Self(kind as u16)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Lang {}

impl rowan::Language for Lang {
    type Kind = SyntaxKind;

    fn kind_from_raw(raw: rowan::SyntaxKind) -> SyntaxKind {
        ALL[raw.0 as usize]
    }

    fn kind_to_raw(kind: SyntaxKind) -> rowan::SyntaxKind {
        kind.into()
    }
}

pub type SyntaxNode = rowan::SyntaxNode<Lang>;
pub type SyntaxToken = rowan::SyntaxToken<Lang>;
pub type SyntaxElement = rowan::SyntaxElement<Lang>;
