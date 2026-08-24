//! Twee AST 到最小 HIR 的转换测试。

use std::path::Path;

use crate::hir::{HirBodyKind, HirError, HirForKind, HirMacroArguments, HirStory};
use crate::source::Source;
use crate::twee::{BodyNode, BodyNodeKind, MacroNode, Passage, Span, Story};

// 大型测试集按用例边界拆成物理分片；门面保留共享导入与模块说明。
// include 保持原模块命名、私有辅助项可见性和测试发现路径不变。
include!("hir/part_01.rs");
include!("hir/part_02.rs");
