//! Narrava Expression 脚本子语言：词法、语法、AST 与无上下文求值。
//!
//! 本模块不接触叙事状态；求值所需的全局值、Setup、随机源与写入能力
//! 都由调用方通过 evaluator 的 Context 接口注入，值模型见 value 模块。

mod ast;
pub mod evaluator;
mod lexer;
mod owned;
mod parser;
mod prototype;
pub mod value;

pub use ast::*;
pub use lexer::*;
pub use owned::*;
pub use parser::*;
