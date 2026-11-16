#[path = "控制流结构化.rs"] pub mod 控制流结构化;
#[path = "类型擦除.rs"]     pub mod 类型擦除;
// 2026-08-03 删除 索引调整.rs（空桩：LIR 保持 1-based，索引转换在 base 后端完成）
#[path = "公共降级.rs"]          pub mod 公共降级;
#[path = "类型转换.rs"]   pub mod 类型转换;
#[path = "函数转换.rs"]   pub mod 函数转换;
#[path = "语句转换.rs"]   pub mod 语句转换;

pub use 公共降级::{降级转换, 内置返回类型};
pub use 类型转换::转换类型;