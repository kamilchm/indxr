use std::fmt;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Declaration {
    pub kind: DeclKind,
    pub name: String,
    pub signature: String,
    pub visibility: Visibility,
    pub line: usize,
    pub doc_comment: Option<String>,
    pub children: Vec<Declaration>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum DeclKind {
    Function,
    Struct,
    Enum,
    Trait,
    Impl,
    Constant,
    Static,
    TypeAlias,
    Module,
    Class,
    Field,
    Variant,
    Method,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Visibility {
    Public,
    PublicCrate,
    Private,
}

impl fmt::Display for Visibility {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Visibility::Public => write!(f, "pub"),
            Visibility::PublicCrate => write!(f, "pub(crate)"),
            Visibility::Private => Ok(()),
        }
    }
}

impl fmt::Display for DeclKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DeclKind::Function | DeclKind::Method => write!(f, "fn"),
            DeclKind::Struct => write!(f, "struct"),
            DeclKind::Enum => write!(f, "enum"),
            DeclKind::Trait => write!(f, "trait"),
            DeclKind::Impl => write!(f, "impl"),
            DeclKind::Constant => write!(f, "const"),
            DeclKind::Static => write!(f, "static"),
            DeclKind::TypeAlias => write!(f, "type"),
            DeclKind::Module => write!(f, "mod"),
            DeclKind::Class => write!(f, "class"),
            DeclKind::Field => write!(f, "field"),
            DeclKind::Variant => write!(f, "variant"),
        }
    }
}
