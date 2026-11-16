// 文件: 后端/解释器/src/调试器.rs
// 职责: 命令行交互式调试器，支持断点、单步、变量查看、调用栈回溯

use super::虚拟机::虚拟机;
use std::collections::HashMap;

pub struct 调试器 {
    pub 断点集合: HashMap<usize, bool>,
    pub 单步模式: bool,
    pub 源码映射: Vec<(usize, usize)>,
    pub 已附着: bool,
    pub 当前函数名: Option<String>,
    pub 断点计数器: usize,
}

impl 调试器 {
    pub fn 新建() -> Self {
        调试器 {
            断点集合: HashMap::new(),
            单步模式: false,
            源码映射: vec![],
            已附着: false,
            当前函数名: None,
            断点计数器: 0,
        }
    }

    pub fn 附着(&mut self, 源码映射: Vec<(usize, usize)>) {
        self.源码映射 = 源码映射;
        self.已附着 = true;
        println!("调试器已附着，源码映射已加载（{}条）", self.源码映射.len());
    }

    pub fn 分离(&mut self) {
        self.已附着 = false;
        self.单步模式 = false;
        self.断点集合.clear();
        println!("调试器已分离");
    }

    pub fn 添加断点(&mut self, 行号: usize) -> usize {
        self.断点计数器 += 1;
        self.断点集合.insert(行号, true);
        let 编号 = self.断点计数器;
        println!("断点 #{} 已设置在第{}行", 编号, 行号);
        编号
    }

    pub fn 按函数名添加断点(&mut self, 函数名: &str, 虚拟机: &虚拟机) -> Option<usize> {
        for 函数 in &虚拟机.函数表 {
            if 函数.名称.contains(函数名) {
                let 行号 = self.偏移转行号(函数.代码偏移 as usize);
                if let Some(行) = 行号 {
                    return Some(self.添加断点(行));
                }
            }
        }
        println!("函数【{}】未找到", 函数名);
        None
    }

    pub fn 删除断点(&mut self, 行号: usize) -> bool {
        let 存在 = self.断点集合.remove(&行号).is_some();
        if 存在 {
            println!("已删除第{}行的断点", 行号);
        } else {
            println!("第{}行没有断点", 行号);
        }
        存在
    }

    pub fn 列出断点(&self) {
        if self.断点集合.is_empty() {
            println!("当前没有断点");
            return;
        }
        println!("当前断点（共{}个）:", self.断点集合.len());
        let mut 断点列表: Vec<_> = self.断点集合.iter().collect();
        断点列表.sort_by_key(|(行号, _)| *行号);
        for (行号, 启用) in &断点列表 {
            let 状态 = if **启用 { "启用" } else { "禁用" };
            println!("  #{}  第{}行 [{}]", 行号, 行号, 状态);
        }
    }

    pub fn 启用断点(&mut self, 行号: usize) {
        if let Some(启用) = self.断点集合.get_mut(&行号) {
            *启用 = true;
            println!("第{}行断点已启用", 行号);
        } else {
            println!("第{}行没有断点", 行号);
        }
    }

    pub fn 禁用断点(&mut self, 行号: usize) {
        if let Some(启用) = self.断点集合.get_mut(&行号) {
            *启用 = false;
            println!("第{}行断点已禁用", 行号);
        } else {
            println!("第{}行没有断点", 行号);
        }
    }

    pub fn 是否需要中断(&self, 指令偏移: usize) -> bool {
        if !self.已附着 {
            return false;
        }
        if self.单步模式 {
            return true;
        }
        if let Some(行号) = self.偏移转行号(指令偏移) {
            if let Some(启用) = self.断点集合.get(&行号) {
                return *启用;
            }
        }
        false
    }

    fn 偏移转行号(&self, 偏移: usize) -> Option<usize> {
        let mut 最接近行号 = None;
        for (指令偏移, 行号) in &self.源码映射 {
            if *指令偏移 <= 偏移 {
                最接近行号 = Some(*行号);
            } else {
                break;
            }
        }
        最接近行号
    }

    pub fn 进入单步模式(&mut self) {
        self.单步模式 = true;
        println!("单步模式已开启");
    }

    pub fn 退出单步模式(&mut self) {
        self.单步模式 = false;
        println!("单步模式已关闭");
    }
}