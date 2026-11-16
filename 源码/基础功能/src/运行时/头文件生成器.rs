// 文件: 基础功能/src/运行时/头文件生成器.rs
// 职责: 为智语言导出 DLL 生成 C 兼容的头文件 (.h)
//
// 设计: 读取导出函数元数据，自动生成:
//   - 类型映射 #define (整数→int64_t 等)
//   - 函数声明 (extern "C" 或 __declspec(dllexport))
//   - 结构体布局 #[repr(C)]
//   - 回调类型定义
//
// 统一架构: 双模式共用，解释/编译 DLL 产出均生成同一份 .h

use super::FFI代码生成器::{C类型, 导出函数元数据};
use std::collections::HashSet;

/// .h 头文件生成器
pub struct 头文件生成器;

impl 头文件生成器 {
    /// 生成 .h 文件内容
    pub fn 生成头文件(
        导出函数列表: &[导出函数元数据],
        库名: &str,
    ) -> String {
        let mut 文件 = String::new();

        // ── 文件头 ──
        文件.push_str("// ===========================================\n");
        文件.push_str(&format!("//  自动生成: {} — 智语言导出 DLL 头文件\n", 库名));
        文件.push_str("//  请勿手动编辑\n");
        文件.push_str("// ===========================================\n\n");
        文件.push_str("#ifndef Z_EXPORT_H\n");
        文件.push_str("#define Z_EXPORT_H\n\n");
        文件.push_str("#include <stdint.h>\n");
        文件.push_str("#include <stdbool.h>\n");
        文件.push_str("#include <stddef.h>\n\n");

        // ── 平台导出宏 ──
        文件.push_str("// 平台导出声明\n");
        文件.push_str("#if defined(_WIN32) || defined(_WIN64)\n");
        文件.push_str("    #define 智语言_API __declspec(dllexport)\n");
        文件.push_str("    #define 智语言_CALL __stdcall\n");
        文件.push_str("#else\n");
        文件.push_str("    #define 智语言_API __attribute__((visibility(\"default\")))\n");
        文件.push_str("    #define 智语言_CALL\n");
        文件.push_str("#endif\n\n");

        // ── 基础类型映射 ──
        文件.push_str("// 类型映射\n");
        文件.push_str("typedef int64_t  智整数;\n");
        文件.push_str("typedef double   智小数;\n");
        文件.push_str("typedef bool    智逻辑;\n");
        文件.push_str("typedef const char* 智文本;\n");
        文件.push_str("typedef void*    智句柄;\n\n");

        // ── 收集外部结构体 ──
        let mut 结构体名集合 = HashSet::new();
        for f in 导出函数列表 {
            Self::收集结构体名(&f.形参类型, &mut 结构体名集合);
            Self::收集结构体名(&[f.返回类型.clone()], &mut 结构体名集合);
        }

        if !结构体名集合.is_empty() {
            文件.push_str("// 结构体布局\n");
            for f in 导出函数列表 {
                for 类型 in &f.形参类型 {
                    Self::生成结构体定义(类型, &mut 文件);
                }
            }
            文件.push_str("\n");
        }

        // ── 导出函数声明 ──
        文件.push_str("// 导出函数声明\n");
        for f in 导出函数列表 {
            let 返回类型名 = match &f.返回类型 {
                C类型::空值 => "void".to_string(),
                C类型::整数8 => "int8_t".to_string(),
                C类型::整数16 => "int16_t".to_string(),
                C类型::整数32 => "int32_t".to_string(),
                C类型::整数64 => "int64_t".to_string(),
                C类型::无符号8 => "uint8_t".to_string(),
                C类型::无符号16 => "uint16_t".to_string(),
                C类型::无符号32 => "uint32_t".to_string(),
                C类型::无符号64 => "uint64_t".to_string(),
                C类型::小数32 => "float".to_string(),
                C类型::小数64 => "double".to_string(),
                C类型::逻辑 => "bool".to_string(),
                C类型::指针 => "void*".to_string(),
                C类型::字符串 => "const char*".to_string(),
                // ★平台相关：直接写 C 平台类型名（long/size_t 由宿主平台决定）
                C类型::C长整数 => "long".to_string(),
                C类型::C无符号长整数 => "unsigned long".to_string(),
                C类型::C字节数量 => "size_t".to_string(),
                C类型::C有符号字节数量 => "ssize_t".to_string(),
                C类型::C字符 => "char".to_string(),
                C类型::C无符号字符 => "unsigned char".to_string(),
                C类型::结构体(名, _) => format!("{}(*)", 名),
                C类型::函数指针(_) => "void*".to_string(),
                // 🆕 数组字段：C 侧为指针（规范数组映射表）
                C类型::数组(_) => "void*".to_string(),
            };
            let 形参声明: Vec<String> = f.形参类型.iter().enumerate()
                .map(|(i, t)| Self::类型到C声明(t, &format!("p{}", i)))
                .collect();
            // 导出符号名用真实 c符号（如 math_add），与编译模式 Cranelift 实际导出的符号名一致。
            // 【2026-10-21 定稿】不再生成 智语言_导出_N 扁平契约别名：一个符号只允许一套 ABI，
            // 扁平契约（运行时值缓冲）属运行时内部约定，规范 §6.5.3 只规定真实 C 签名。
            let 函数名 = f.c符号.clone();
            if 返回类型名 == "void" {
                文件.push_str(&format!("智语言_API void 智语言_CALL {}({});\n", 函数名, 形参声明.join(", ")));
            } else {
                文件.push_str(&format!("智语言_API {} 智语言_CALL {}({});\n", 返回类型名, 函数名, 形参声明.join(", ")));
            }
        }

        // ── 文本返回值回收 ──
        // 导出函数返回 `字符串` 时，库内会给出一个 NUL 结尾的 UTF-8 副本，
        // 所有权交给 C 侧，用完必须调用本函数回收（否则泄漏）。
        if 导出函数列表.iter().any(|f| f.返回类型 == C类型::字符串) {
            文件.push_str("\n// 文本返回值回收（导出函数返回 const char* 时调用）\n");
            文件.push_str("智语言_API void 智语言_CALL 智语言_释放C字符串(const char* 文本);\n");
        }

        文件.push_str("\n#endif /* Z_EXPORT_H */\n");
        文件
    }

    fn 类型到C声明(类型: &C类型, 名: &str) -> String {
        match 类型 {
            C类型::空值 => format!("void {}", 名),
            C类型::整数8 => format!("int8_t {}", 名),
            C类型::整数16 => format!("int16_t {}", 名),
            C类型::整数32 => format!("int32_t {}", 名),
            C类型::整数64 => format!("int64_t {}", 名),
            C类型::无符号8 => format!("uint8_t {}", 名),
            C类型::无符号16 => format!("uint16_t {}", 名),
            C类型::无符号32 => format!("uint32_t {}", 名),
            C类型::无符号64 => format!("uint64_t {}", 名),
            C类型::小数32 => format!("float {}", 名),
            C类型::小数64 => format!("double {}", 名),
            C类型::逻辑 => format!("bool {}", 名),
            C类型::指针 => format!("void* {}", 名),
            C类型::字符串 => format!("const char* {}", 名),
            C类型::C长整数 => format!("long {}", 名),
            C类型::C无符号长整数 => format!("unsigned long {}", 名),
            C类型::C字节数量 => format!("size_t {}", 名),
            C类型::C有符号字节数量 => format!("ssize_t {}", 名),
            C类型::C字符 => format!("char {}", 名),
            C类型::C无符号字符 => format!("unsigned char {}", 名),
            C类型::结构体(_, _) => format!("void* {}", 名),
            C类型::函数指针(_) => format!("void* {}", 名),
            // 🆕 数组字段：C 侧为指针（规范数组映射表）
            C类型::数组(_) => format!("void* {}", 名),
        }
    }

    fn 收集结构体名(类型列表: &[C类型], 集合: &mut HashSet<String>) {
        for t in 类型列表 {
            if let C类型::结构体(名, _) = t {
                集合.insert(名.clone());
            }
        }
    }

    fn 生成结构体定义(类型: &C类型, 文件: &mut String) {
        if let C类型::结构体(名, 字段) = 类型 {
            文件.push_str(&format!("typedef struct {{\n"));
            for (i, 字段类型) in 字段.iter().enumerate() {
                let 字段名 = format!("字段_{}", i);
                文件.push_str(&format!("    {} {};\n", Self::类型到C声明(字段类型, &字段名), ""));
            }
            文件.push_str(&format!("}} {};\n\n", 名));
        }
    }
}

#[cfg(test)]
mod 测试 {
    use super::*;
    use crate::FFI代码生成器::{C类型, 导出函数元数据};

    #[test]
    fn 生成基本头文件() {
        let 导出 = vec![
            导出函数元数据 {
                导出序号: 0,
                智语言函数名: "加法".to_string(),
                c符号: "math_add".to_string(),
                形参类型: vec![C类型::整数64, C类型::整数64],
                返回类型: C类型::整数64,
            },
            导出函数元数据 {
                导出序号: 1,
                智语言函数名: "问候".to_string(),
                c符号: "greet".to_string(),
                形参类型: vec![C类型::字符串],
                返回类型: C类型::空值,
            },
        ];
        let 头文件 = 头文件生成器::生成头文件(&导出, "测试库");
        assert!(头文件.contains("#ifndef Z_EXPORT_H"));
        assert!(头文件.contains("int64_t"));
        assert!(头文件.contains("const char*"));
        assert!(头文件.contains("math_add"));
        assert!(头文件.contains("greet"));
        // 【2026-10-21 定稿】公开头文件只声明真实 C 签名（唯一 ABI）：
        // 不再出现扁平契约的 智运行时值 结构 / 标签常量 / 智语言_导出_N 别名。
        assert!(!头文件.contains("智运行时值"));
        assert!(!头文件.contains("智_标签_"));
        assert!(!头文件.contains("智语言_导出_"));
        // 有 文本 返回时给出回收函数声明
        let 带文本返回 = vec![导出函数元数据 {
            导出序号: 0,
            智语言函数名: "版本".to_string(),
            c符号: "VERSION".to_string(),
            形参类型: vec![],
            返回类型: C类型::字符串,
        }];
        let 头 = 头文件生成器::生成头文件(&带文本返回, "测试库");
        assert!(头.contains("const char* 智语言_CALL VERSION()"));
        assert!(头.contains("智语言_释放C字符串"));
    }

    #[test]
    fn 结构体头文件() {
        let 导出 = vec![
            导出函数元数据 {
                导出序号: 0,
                智语言函数名: "处理点".to_string(),
                c符号: "process_point".to_string(),
                形参类型: vec![C类型::结构体("点".to_string(), vec![C类型::整数64, C类型::整数64])],
                返回类型: C类型::空值,
            },
        ];
        let 头文件 = 头文件生成器::生成头文件(&导出, "结构体测试");
        assert!(头文件.contains("点"));
        assert!(头文件.contains("字段_0"));
        assert!(头文件.contains("字段_1"));
    }

    #[test]
    fn 类型到C声明测试() {
        assert_eq!(头文件生成器::类型到C声明(&C类型::整数64, "x"), "int64_t x");
        assert_eq!(头文件生成器::类型到C声明(&C类型::小数64, "y"), "double y");
        assert_eq!(头文件生成器::类型到C声明(&C类型::逻辑, "b"), "bool b");
        assert_eq!(头文件生成器::类型到C声明(&C类型::指针, "p"), "void* p");
        assert_eq!(头文件生成器::类型到C声明(&C类型::字符串, "s"), "const char* s");
        assert_eq!(头文件生成器::类型到C声明(&C类型::空值, "v"), "void v");
    }
}
