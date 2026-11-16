#![allow(non_snake_case)]
#![allow(dead_code)]

#[path = "代码生成/代码生成核心.rs"]   pub mod 代码生成;
#[path = "代码生成/操作数执行器.rs"]    pub mod 操作数执行器;
#[path = "代码生成/算术执行器.rs"]      pub mod 算术执行器;
#[path = "代码生成/系统调用执行器.rs"]  pub mod 系统调用执行器;
#[path = "代码生成/任务执行器.rs"]      pub mod 任务执行器;
#[path = "代码生成/翻译执行器.rs"]      pub mod 翻译执行器;
#[path = "优化管线.rs"] pub mod 优化管线;
#[path = "链接器.rs"]   pub mod 链接器;
#[path = "调试信息.rs"] pub mod 调试信息;

pub use 代码生成::编译器;