use crate::公共数据结构::*;

pub struct 诊断收集器 {
    诊断列表: Vec<诊断信息>,
    错误计数: usize,
    警告计数: usize,
    最大错误数: usize,
    最大警告数: usize,
    截断错误数: usize,
    截断警告数: usize,
    每行最大诊断数: usize,       // 行级别截断阈值
    源映射: 源文件映射,           // 源代码缓存
    格式化器: Option<Box<dyn 格式化器 + Send>>, // 可插拔格式化器
    简洁模式: bool,               // --简洁 简版格式
    颜色模式: bool,               // --颜色 ANSI 颜色
    结构化模式: bool,             // --结构化 JSON 输出
}

impl 诊断收集器 {
    pub fn 新建() -> Self {
        诊断收集器 {
            诊断列表: Vec::new(),
            错误计数: 0,
            警告计数: 0,
            最大错误数: 100,
            最大警告数: 50,
            截断错误数: 0,
            截断警告数: 0,
            每行最大诊断数: 3,
            源映射: 源文件映射::新建(),
            格式化器: None,
            简洁模式: false,
            颜色模式: false,
            结构化模式: false,
        }
    }

    pub fn 报告(&mut self, 诊断: 诊断信息) {
        match 诊断.级别 {
            诊断级别::错误 => {
                if self.错误计数 < self.最大错误数 {
                    self.诊断列表.push(诊断);
                } else {
                    self.截断错误数 += 1;
                }
                self.错误计数 += 1;
            }
            诊断级别::警告 => {
                if self.警告计数 < self.最大警告数 {
                    self.诊断列表.push(诊断);
                } else {
                    self.截断警告数 += 1;
                }
                self.警告计数 += 1;
            }
        }
    }

    /// 报告后端/运行时错误（字符串形式）
    pub fn 报告字符串错误(&mut self, 描述: String, 文件路径: String, 行号: usize, 列号: usize) {
        let 诊断 = 诊断信息::新(
            诊断级别::错误,
            错误类型::运行时错误,
            描述,
            源码位置::新(文件路径, 行号, 列号),
            None,
        );
        self.报告(诊断);
    }

    pub fn 有错误(&self) -> bool { self.错误计数 > 0 }

    /// 获取诊断列表的引用（结构体API，供编辑器服务使用）
    pub fn 获取诊断列表(&self) -> &[诊断信息] { &self.诊断列表 }

    /// 获取源文件映射的引用
    pub fn 获取源文件映射(&self) -> &源文件映射 { &self.源映射 }

    /// 设置格式化器
    pub fn 设置格式化器(&mut self, 格式化器: Box<dyn 格式化器 + Send>) {
        self.格式化器 = Some(格式化器);
    }

    /// 设置行级别截断阈值
    pub fn 设置每行最大诊断数(&mut self, 最大数: usize) {
        self.每行最大诊断数 = 最大数;
    }

    /// 设置简洁模式（--简洁），无源码上下文
    pub fn 设置简洁模式(&mut self, 启用: bool) {
        self.简洁模式 = 启用;
    }

    /// 设置颜色模式（--颜色），ANSI 颜色输出
    pub fn 设置颜色模式(&mut self, 启用: bool) {
        self.颜色模式 = 启用;
    }

    /// 设置结构化模式（--结构化），JSON 输出供工具解析
    pub fn 设置结构化模式(&mut self, 启用: bool) {
        self.结构化模式 = 启用;
    }

    /// 获取结构化模式状态
    pub fn 结构化模式(&self) -> bool {
        self.结构化模式
    }

    /// 注册源文件源码（供代码上下文显示使用）
    pub fn 注册源文件(&mut self, 文件路径: &str, 源码: &str) {
        self.源映射.注册文件(文件路径, 源码);
    }

    /// 输出所有诊断
    ///
    /// 简洁模式（简洁模式=true）时输出无源码上下文的两行简版。
    /// 完整模式时输出含源码行 + 波浪线标记的完整格式。
    ///
    /// 输出格式（完整模式）：
    ///   第3行1列 | 错误: 语法错误: 【描述】
    ///   3 │ 源码行内容
    ///     │ ～～～ 位置标记
    ///     │
    ///   帮助: 修复建议
    ///
    /// 输出格式（简洁模式）：
    ///   第3行1列 | 错误: 语法错误: 【描述】
    ///   帮助: 修复建议
    pub fn 输出所有诊断(&self) -> Vec<String> {
        // 先排序（规则：错误优先于警告，按行号从小到大）
        let mut 排序后列表 = self.诊断列表.clone();
        排序后列表.sort_by(|甲, 乙| {
            let 甲键 = 诊断排序键(甲);
            let 乙键 = 诊断排序键(乙);
            甲键.cmp(&乙键)
        });

        // 行级别截断：同文件同行的错误/警告各最多保留 N 条
        let mut 截断后列表 = 排序后列表.clone();
        if self.每行最大诊断数 > 0 {
            应用行截断(&mut 截断后列表, self.每行最大诊断数);
        }

        // 结构化模式走 JSON 格式化器（输出到 stdout）
        if self.结构化模式 {
            let 格式化器 = JSON格式化器;
            let mut 输出 = Vec::new();
            for 诊断 in &截断后列表 {
                输出.push(格式化器.格式化(诊断, &self.源映射));
            }
            // 断尾提示同样 JSON 格式
            if self.截断错误数 > 0 {
                输出.push(format!("{{\"级别\":\"错误\",\"截断\":{}}}", self.截断错误数));
            }
            if self.截断警告数 > 0 {
                输出.push(format!("{{\"级别\":\"警告\",\"截断\":{}}}", self.截断警告数));
            }
            return 输出;
        }

        // 根据模式选择格式化器
        let 格式化器: Box<dyn 格式化器> = if self.简洁模式 {
            if self.颜色模式 {
                Box::new(智语言简洁格式化器::新建_颜色())
            } else {
                Box::new(智语言简洁格式化器::新建())
            }
        } else {
            if self.颜色模式 {
                Box::new(智语言格式化器::新建_颜色())
            } else {
                Box::new(智语言格式化器::新建())
            }
        };

        let mut 输出 = Vec::new();
        for (i, 诊断) in 截断后列表.iter().enumerate() {
            // 每个错误之间用空行分隔（第一个之前不空行）
            if i > 0 {
                输出.push(String::new());
            }

            输出.push(格式化器.格式化(诊断, &self.源映射));
        }

        // 断尾提示
        if self.截断错误数 > 0 {
            输出.push(format!("... 以及其他 {} 条错误", self.截断错误数));
        }
        if self.截断警告数 > 0 {
            输出.push(format!("... 以及其他 {} 条警告", self.截断警告数));
        }

        输出
    }

    /// 输出所有诊断简洁（测试格式化器，无空行分隔，用于测试断定）
    pub fn 输出所有诊断简洁(&self) -> Vec<String> {
        let mut 排序后列表 = self.诊断列表.clone();
        排序后列表.sort_by(|甲, 乙| {
            let 甲键 = 诊断排序键(甲);
            let 乙键 = 诊断排序键(乙);
            甲键.cmp(&乙键)
        });

        let 格式化器 = 测试格式化器;
        let mut 输出 = Vec::new();
        for 诊断 in &排序后列表 {
            输出.push(格式化器.格式化(诊断, &self.源映射));
        }

        if self.截断错误数 > 0 {
            输出.push(format!("... 以及其他 {} 条错误", self.截断错误数));
        }
        if self.截断警告数 > 0 {
            输出.push(format!("... 以及其他 {} 条警告", self.截断警告数));
        }

        输出
    }

    /// 获取错误计数
    pub fn 错误计数(&self) -> usize { self.错误计数 }

    /// 获取警告计数
    pub fn 警告计数(&self) -> usize { self.警告计数 }
}
