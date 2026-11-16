// 文件: 后端/编译器/src/调试信息.rs
// 职责: 生成 DWARF 调试信息，支持源码级调试

use std::collections::HashMap;

/// 源码位置信息
#[derive(Debug, Clone)]
pub struct 源码位置信息 {
    pub 文件路径: String,
    pub 行号: u32,
    pub 列号: u32,
}

/// DWARF调试信息生成器
pub struct 调试信息生成器 {
    pub 文件表: Vec<String>,
    pub 行号映射: HashMap<usize, 源码位置信息>,
    pub 函数调试信息: Vec<函数调试信息>,
}

#[derive(Debug, Clone)]
pub struct 函数调试信息 {
    pub 函数名: String,
    pub 起始偏移: u32,
    pub 结束偏移: u32,
    pub 指令位置列表: Vec<(u32, u32, u32)>, // (指令偏移, 行号, 列号)
}

impl 调试信息生成器 {
    pub fn 新建() -> Self {
        调试信息生成器 {
            文件表: Vec::new(),
            行号映射: HashMap::new(),
            函数调试信息: Vec::new(),
        }
    }

    /// 注册源文件
    pub fn 注册文件(&mut self, 文件路径: String) -> usize {
        if let Some(索引) = self.文件表.iter().position(|f| f == &文件路径) {
            return 索引;
        }
        let 索引 = self.文件表.len();
        self.文件表.push(文件路径);
        索引
    }

    /// 记录指令对应的源码位置
    pub fn 记录位置(&mut self, 指令偏移: usize, 文件索引: usize, 行号: u32, 列号: u32) {
        self.行号映射.insert(
            指令偏移,
            源码位置信息 {
                文件路径: self.文件表.get(文件索引).cloned().unwrap_or_default(),
                行号,
                列号,
            },
        );
    }

    /// 添加函数调试信息
    pub fn 添加函数(&mut self, 函数名: String, 起始偏移: u32, 结束偏移: u32) {
        self.函数调试信息.push(函数调试信息 {
            函数名,
            起始偏移,
            结束偏移,
            指令位置列表: Vec::new(),
        });
    }

    /// 生成DWARF字节数据（简化版本）
    pub fn 生成DWARF数据(&self) -> Vec<u8> {
        let mut 数据 = Vec::new();

        // DWARF 文件头
        数据.extend_from_slice(b"DWARF");
        数据.push(0x04); // 版本

        // 文件表
        数据.push(self.文件表.len() as u8);
        for 文件 in &self.文件表 {
            数据.push(文件.len() as u8);
            数据.extend_from_slice(文件.as_bytes());
        }

        // 函数调试信息
        数据.push(self.函数调试信息.len() as u8);
        for 函数 in &self.函数调试信息 {
            数据.push(函数.函数名.len() as u8);
            数据.extend_from_slice(函数.函数名.as_bytes());
            数据.extend_from_slice(&函数.起始偏移.to_le_bytes());
            数据.extend_from_slice(&函数.结束偏移.to_le_bytes());
        }

        数据
    }
}