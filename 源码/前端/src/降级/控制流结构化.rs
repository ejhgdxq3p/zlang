use crate::底层表示::模块结构::基本块;
use crate::底层表示::类型定义::底层表示类型;
use std::collections::HashMap;

pub struct 降级上下文 {
    pub 下一个基本块标识: usize,
    pub 下一个局部变量索引: usize,
    pub 基本块列表: Vec<基本块>,
    pub 局部变量表: Vec<底层表示类型>,
    pub 变量映射: HashMap<String, usize>,
}

impl 降级上下文 {
    pub fn 新建() -> Self {
        降级上下文 {
            下一个基本块标识: 0,
            下一个局部变量索引: 1,
            基本块列表: vec![],
            局部变量表: vec![底层表示类型::整数64],
            变量映射: HashMap::new(),
        }
    }

    pub fn 分配局部变量(&mut self, 类型: 底层表示类型) -> usize {
        let 索引 = self.下一个局部变量索引;
        self.局部变量表.push(类型);
        self.下一个局部变量索引 += 1;
        索引
    }
}