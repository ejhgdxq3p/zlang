// 文件: 前端/src/语义/类型变量表.rs
// 职责: 类型变量作用域管理——统一有名/匿名任意类型的注册与查找
// 设计: 匿名类型自动分配唯一内部标识，有名类型按名称查表；支持嵌套作用域

use super::类型信息::类型信息;
use crate::公共数据结构::源码位置;
use std::collections::HashMap;

/// 类型变量作用域表（支持嵌套作用域链）
#[derive(Debug, Clone)]
pub struct 类型变量表 {
    pub 变量列表: Vec<类型变量定义>,
    名称索引: HashMap<String, usize>,       // 当前作用域: 名称 → 索引
    pub 父作用域: Option<Box<类型变量表>>,   // 外层作用域
    匿名计数器: usize,                       // 生成唯一匿名标识
}

/// 单个类型变量的完整信息
#[derive(Debug, Clone)]
pub struct 类型变量定义 {
    pub 名称: String,              // "甲" / "键型" / "匿名_42"
    pub 约束列表: Vec<类型信息>,   // 显式约束（空=无约束）
    pub 是否匿名: bool,            // 编译器生成 vs 用户声明
    pub 定义位置: 源码位置,        // 错误定位
    pub 所属容器: Option<String>,  // None | Some("数组") | Some("映射")
}

impl 类型变量表 {
    /// 新建一个空的作用域
    pub fn 新建() -> Self {
        类型变量表 {
            变量列表: Vec::new(),
            名称索引: HashMap::new(),
            父作用域: None,
            匿名计数器: 0,
        }
    }

    /// 注册有名类型变量（任意<甲: 约束>）
    /// 返回分配的索引
    pub fn 注册有名(
        &mut self,
        名称: String,
        约束: Vec<类型信息>,
        位置: 源码位置,
        容器: Option<String>,
    ) -> usize {
        let 索引 = self.变量列表.len();
        self.变量列表.push(类型变量定义 {
            名称: 名称.clone(),
            约束列表: 约束,
            是否匿名: false,
            定义位置: 位置,
            所属容器: 容器,
        });
        self.名称索引.insert(名称, 索引);
        索引
    }

    /// 注册匿名类型变量（任意<类型|类型>）
    /// 自动生成唯一内部标识，返回分配的索引
    pub fn 注册匿名(&mut self, 约束: Vec<类型信息>, 位置: 源码位置) -> usize {
        self.匿名计数器 += 1;
        let 名称 = format!("匿名_{}", self.匿名计数器);
        let 索引 = self.变量列表.len();
        self.变量列表.push(类型变量定义 {
            名称,
            约束列表: 约束,
            是否匿名: true,
            定义位置: 位置,
            所属容器: None,
        });
        // 匿名类型不注册到名称索引
        索引
    }

    /// 按名称查找类型变量（从当前作用域向外）
    pub fn 查找(&self, 名称: &str) -> Option<&类型变量定义> {
        self.名称索引
            .get(名称)
            .and_then(|&idx| self.变量列表.get(idx))
            .or_else(|| self.父作用域.as_ref()?.查找(名称))
    }

    /// 按名称查找并返回索引
    pub fn 查找索引(&self, 名称: &str) -> Option<usize> {
        self.名称索引
            .get(名称)
            .copied()
            .or_else(|| self.父作用域.as_ref()?.查找索引(名称))
    }

    /// 按索引获取类型变量定义
    pub fn 获取(&self, 索引: usize) -> &类型变量定义 {
        &self.变量列表[索引]
    }

    /// 可选获取（索引可能越界时）
    pub fn 尝试获取(&self, 索引: usize) -> Option<&类型变量定义> {
        self.变量列表.get(索引)
    }

    /// 创建子作用域（进入函数/代码块时）
    pub fn 进入作用域(&self) -> Self {
        类型变量表 {
            变量列表: Vec::new(),
            名称索引: HashMap::new(),
            父作用域: Some(Box::new(self.clone())),
            匿名计数器: 0,
        }
    }
}
