use super::*;

// 大型测试集按用例边界拆成物理分片；门面保留共享导入与模块说明。
// include 保持原模块命名、私有辅助项可见性和测试发现路径不变。
include!("widgets_and_runtime/part_01.rs");
include!("widgets_and_runtime/part_02.rs");
include!("widgets_and_runtime/part_03.rs");
