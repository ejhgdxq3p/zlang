// 文件: 前端/src/库.rs
// 职责: 前端模块入口，声明所有子模块

#![allow(non_snake_case)]
#![allow(dead_code)]

#[path = "公共数据结构.rs"]     pub mod 公共数据结构;
#[path = "诊断收集器.rs"]       pub mod 诊断收集器;
#[path = "诊断全局.rs"]         pub mod 诊断全局;
#[path = "词法分析.rs"]         pub mod 词法分析;
#[path = "语法分析/语法分析器.rs"]   pub mod 语法分析;
#[path = "语法分析/语句语法分析.rs"]  pub mod 语句语法分析;
#[path = "语法分析/表达式语法分析.rs"]pub mod 表达式语法分析;
#[path = "语法分析/类型语法分析.rs"]  pub mod 类型语法分析;
#[path = "语义/语义.rs"]         pub mod 语义;
#[path = "底层表示/底层表示.rs"] pub mod 底层表示;
#[path = "标准库元数据.rs"]      pub mod 标准库元数据;  // ★标准库元数据单一真相（2026-08-30 彻底单源重构）
#[path = "名称身份.rs"]          pub mod 名称身份;       // ★四类受限名称统一身份层（§2.2.3 关键字/保留关键字/上下文关键字/预定义标识符）
#[path = "标准库门面生成.rs"]     pub mod 标准库门面生成; // ★门面机械生成器（元数据→门面 唯一投影通道）
#[path = "标准库真源校验.rs"]     pub mod 标准库真源校验; // ★标准库真源自洽门禁（防双源漂移）
#[path = "规范签名.rs"]           pub mod 规范签名;       // ★规范签名权威表（规范 §5 的可执行转录，供第④层对账）
#[path = "降级/降级.rs"]         pub mod 降级;
#[path = "模块加载器.rs"]       pub mod 模块加载器;
#[path = "格式化器.rs"]         pub mod 格式化器;
#[path = "文档生成器.rs"]       pub mod 文档生成器;
#[path = "流水线.rs"]           pub mod 流水线;