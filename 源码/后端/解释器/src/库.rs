#![allow(non_snake_case)]
#![allow(dead_code)]
#![allow(private_interfaces)]

// 统一架构：值表示.rs 已删除，所有值操作通过运行时 crate
#[path = "字节码格式.rs"]     pub mod 字节码格式;
#[path = "字节码生成.rs"]     pub mod 字节码生成;
#[path = "值转换.rs"]         pub mod 值转换;
// 2026-08-03 删除网络资源.rs（VM 侧未使用死代码，实际网络走 base 网络表）
#[path = "虚拟机/虚拟机核心.rs"]      pub mod 虚拟机;
#[path = "虚拟机/并发执行器.rs"]        pub mod 并发执行器;
#[path = "虚拟机/系统调用执行器.rs"]    pub mod 系统调用执行器;
#[path = "虚拟机/堆与值执行器.rs"]      pub mod 堆与值执行器;
#[path = "调试器.rs"]         pub mod 调试器;
#[path = "缓存管理.rs"]       pub mod 缓存管理;
