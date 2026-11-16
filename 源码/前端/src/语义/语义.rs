#[path = "符号表.rs"]       pub mod 符号表;
#[path = "类型信息.rs"]     pub mod 类型信息;
#[path = "类型变量表.rs"]   pub mod 类型变量表;
#[path = "类型统一化.rs"]   pub mod 类型统一化;
#[path = "高层表示节点.rs"]  pub mod 高层表示节点;
#[path = "编译期求值.rs"]   pub mod 编译期求值;
#[path = "类型推断.rs"]     pub mod 类型推断;
#[path = "类型检查.rs"]     pub mod 类型检查;
#[path = "运算符重载.rs"]   pub mod 运算符重载;
#[path = "语义分析器.rs"]   pub mod 语义分析模块;
pub use 语义分析模块::语义分析器;
#[path = "声明分析.rs"]     pub mod 声明分析;
#[path = "类型推导.rs"]     pub mod 类型推导;
#[path = "泛型解析.rs"]     pub mod 泛型解析;
#[path = "语句分析.rs"]     pub mod 语句分析;
#[path = "表达式分析.rs"]   pub mod 表达式分析;
#[path = "类型参数作用域.rs"] pub mod 类型参数作用域; // ★§2.4.7 函数/方法级类型参数命名空间上下文（2026-10-17）