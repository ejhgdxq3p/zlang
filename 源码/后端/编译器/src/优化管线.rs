// 文件: 后端/编译器/src/优化管线.rs
// 职责: 分级优化策略，对接 Cranelift 优化 Pass

use cranelift_codegen::settings::{self, Configurable};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum 优化级别 {
    无优化,
    速度优化,
    速度与体积优化,
}

impl 优化级别 {
    pub fn 从命令行解析(标志: &str) -> Self {
        match 标志 {
            "--调试" => 优化级别::无优化,
            "--精简" => 优化级别::速度与体积优化,
            _ => 优化级别::速度优化,
        }
    }

    pub fn 描述(&self) -> &str {
        match self {
            优化级别::无优化 => "无优化（调试模式）",
            优化级别::速度优化 => "速度优化（默认）",
            优化级别::速度与体积优化 => "速度与体积优化",
        }
    }

    /// 返回优化级别的数字编号，用于构建签名
    pub fn 编号(&self) -> &str {
        match self {
            优化级别::无优化 => "0",
            优化级别::速度优化 => "1",
            优化级别::速度与体积优化 => "2",
        }
    }

    pub fn 转cranelift级别(&self) -> settings::OptLevel {
        match self {
            优化级别::无优化 => settings::OptLevel::None,
            优化级别::速度优化 => settings::OptLevel::Speed,
            优化级别::速度与体积优化 => settings::OptLevel::SpeedAndSize,
        }
    }
}

pub struct 优化管线 {
    pub 级别: 优化级别,
}

impl 优化管线 {
    pub fn 新建(级别: 优化级别) -> Self {
        优化管线 { 级别 }
    }

    pub fn 默认() -> Self {
        优化管线 {
            级别: 优化级别::速度优化,
        }
    }

    pub fn 创建设置(&self) -> settings::Flags {
        let mut 构建器 = settings::builder();
        构建器
            .set("opt_level", &self.级别.转cranelift级别().to_string())
            .expect("设置优化级别失败");
        if matches!(self.级别, 优化级别::速度优化 | 优化级别::速度与体积优化) {
            构建器
                .set("enable_verifier", "false")
                .expect("设置验证器失败");
        }
        settings::Flags::new(构建器)
    }
}