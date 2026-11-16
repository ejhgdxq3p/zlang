#![allow(non_snake_case)]
#![allow(private_interfaces)]

use base::值类型::*;

// 文件: 后端/解释器/src/虚拟机/堆与值执行器.rs
// 职责: 虚拟机 的姐妹模块，承载与 运行时值 相关的辅助方法
// 统一架构：所有操作直接在 运行时值 上完成，不再涉及 值 枚举
//
// ⚡双模式⚡ 值转整数/值转键 已统一收归 base（值类型.rs / 基础辅助.rs），
// 此处不再保留重复实现（原 VM 侧副本已被 #![allow(dead_code)] 掩盖，实为死代码）。
// 唯一真相在 base，解释/编译双模自动 parity。

impl crate::虚拟机::虚拟机 {
    /// 深度提取结构体信息（用于并发/网络资源操作）。
    /// 预留：解释器侧现有路径未调用（并发/网络资源操作统一经 base）；显式豁免 dead_code，
    /// 保留能力对称性，并避免被误判为"死代码"。
    #[allow(dead_code)]
    pub(crate) fn 运行时值到结构体(&self, v: &运行时值) -> Option<(usize, Vec<运行时值>)> {
        if v.标签 == 标签_句柄 {
            if let Some(堆数据::结构体 { 类型索引, 字段值 }) = 堆读取(v.句柄) {
                return Some((类型索引, 字段值));
            }
        }
        None
    }
}
