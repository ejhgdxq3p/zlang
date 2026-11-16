// 文件: 前端/src/语义/类型统一化.rs
// 职责: 泛型结构体实例的类型变量绑定与检查
// 设计: 首次写入绑定 + 后续等值检查 —— 覆盖通道/任务/用户自定义泛型结构体

use super::类型信息::类型信息;
use std::collections::HashMap;

/// 单个泛型结构体实例的类型参数绑定表
/// 索引: 结构体形参列表中类型参数的位置
/// 值: None=未绑定, Some(T)=已绑定为具体类型
#[derive(Debug, Clone)]
pub struct 类型绑定表 {
    绑定: Vec<Option<类型信息>>,
}

impl 类型绑定表 {
    pub fn 新建(参数数量: usize) -> Self {
        类型绑定表 { 绑定: vec![None; 参数数量] }
    }

    /// 绑定或检查类型参数
    /// 首次调用（未绑定）→ 绑定该类型
    /// 后续调用（已绑定）→ 检查是否与已绑定类型匹配，不匹配则返回错误
    pub fn 绑定或检查(&mut self, 参数索引: usize, 新类型: &类型信息) -> Result<(), String> {
        if 参数索引 >= self.绑定.len() {
            return Err(format!("泛型类型参数索引越界：期望 < {}，实际 {}", self.绑定.len(), 参数索引));
        }
        if let Some(已绑定) = &self.绑定[参数索引] {
            if 已绑定 != 新类型 {
                return Err(format!(
                    "类型不匹配：泛型参数期望【{}】，但传入【{}】",
                    已绑定.类型名称(),
                    新类型.类型名称()
                ));
            }
        } else {
            self.绑定[参数索引] = Some(新类型.clone());
        }
        Ok(())
    }

    /// 获取指定位置的已绑定类型（未绑定返回 None）
    pub fn 获取类型(&self, 参数索引: usize) -> Option<&类型信息> {
        self.绑定.get(参数索引).and_then(|o| o.as_ref())
    }
}

/// 全局实例绑定表：变量名 → 该变量的类型参数绑定表
/// 每个被赋值为泛型结构体的变量在此注册一个条目
#[derive(Debug, Clone)]
pub struct 实例绑定表 {
    表: HashMap<String, 类型绑定表>,
}

impl 实例绑定表 {
    pub fn 新建() -> Self {
        实例绑定表 { 表: HashMap::new() }
    }

    /// 注册一个泛型结构体实例（仅在首次声明/赋值时调用）
    /// 若变量已注册，则重置其绑定表（支持重新赋值）
    /// 写入真实类型参数到绑定槽位
    pub fn 注册或重置实例(&mut self, 变量名: &str, 类型参数: &[类型信息]) {
        let 绑定: Vec<Option<类型信息>> = 类型参数.iter().map(|t| Some(t.clone())).collect();
        self.表.insert(变量名.to_string(), 类型绑定表 { 绑定 });
    }

    /// 注册实例并只保留槽位数（用于类型参数尚未确定时）
    pub fn 注册或重置实例占位(&mut self, 变量名: &str, 参数数量: usize) {
        self.表.insert(变量名.to_string(), 类型绑定表::新建(参数数量));
    }

    /// 绑定方法实参类型到指定参数位置
    /// 例如：管道.发送(42) → 绑定索引0为整数
    pub fn 绑定方法实参(
        &mut self,
        变量名: &str,
        参数索引: usize,
        实参类型: &类型信息,
    ) -> Result<(), String> {
        if let Some(表) = self.表.get_mut(变量名) {
            表.绑定或检查(参数索引, 实参类型)
        } else {
            // 未注册的变量跳过（非泛型结构体/未跟踪的变量）
            Ok(())
        }
    }

    /// 获取方法返回类型（将类型变量替换为已绑定的具体类型）
    /// 例如：接收() → 任意<甲>，若甲已绑定为整数，则返回整数
    pub fn 获取方法返回类型(
        &self,
        变量名: &str,
        方法声明返回类型: &类型信息,
    ) -> 类型信息 {
        if let 类型信息::任意类型 { 变量索引, .. } = 方法声明返回类型 {
            if let Some(表) = self.表.get(变量名) {
                if let Some(已绑定) = 表.获取类型(*变量索引) {
                    return 已绑定.clone();
                }
            }
        }
        // 不是类型变量引用，或未绑定：返回原类型
        方法声明返回类型.clone()
    }

    /// 检查变量是否已注册（泛型结构体实例）
    pub fn 是否已注册(&self, 变量名: &str) -> bool {
        self.表.contains_key(变量名)
    }

    /// 读取已注册实例的第 索引 个类型参数（用于通道元素类型一致性检查）；未注册/越界返回 None。
    pub fn 读取类型参数(&self, 变量名: &str, 索引: usize) -> Option<类型信息> {
        self.表
            .get(变量名)
            .and_then(|t| t.绑定.get(索引).cloned())
            .flatten()
    }
}
