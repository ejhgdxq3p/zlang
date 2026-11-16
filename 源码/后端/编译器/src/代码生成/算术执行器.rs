// 文件: 后端/编译器/src/代码生成/算术执行器.rs
// 职责: 底层表示 → Cranelift IR → 目标文件，完整支持结构体/数组/函数调用（含参数传递）和DWARF调试信息

use cranelift_codegen::ir::{types, AbiParam, InstBuilder, MemFlags, Signature};
use cranelift_codegen::isa::CallConv;
use cranelift_frontend::FunctionBuilder;
use cranelift_module::{Linkage, Module};
use cranelift_object::ObjectModule;
use target_lexicon::Triple;

use frontend::底层表示::指令集::{操作数, 三地址码};
use base::值类型::标签_句柄;

/// 统一值表示：每个局部 = (标签, 载荷) 双槽 i64，与运行时 运行时值 / 智语言_堆操作 / 智语言_系统调用 同一套语言。
use crate::代码生成::编译器;

impl 编译器 {
    pub(crate) fn 运行时加法(
        &mut self,
        构建器: &mut FunctionBuilder,
        模块: &mut ObjectModule,
        左标签: cranelift_codegen::ir::Value,
        左载荷: cranelift_codegen::ir::Value,
        右标签: cranelift_codegen::ir::Value,
        右载荷: cranelift_codegen::ir::Value,
    ) -> Result<(cranelift_codegen::ir::Value, cranelift_codegen::ir::Value), String> {
        self.运行时二元算术(构建器, 模块, base::堆操作::堆_加法, 左标签, 左载荷, 右标签, 右载荷)
    }

    pub(crate) fn 运行时减法(
        &mut self,
        构建器: &mut FunctionBuilder,
        模块: &mut ObjectModule,
        左标签: cranelift_codegen::ir::Value,
        左载荷: cranelift_codegen::ir::Value,
        右标签: cranelift_codegen::ir::Value,
        右载荷: cranelift_codegen::ir::Value,
    ) -> Result<(cranelift_codegen::ir::Value, cranelift_codegen::ir::Value), String> {
        self.运行时二元算术(构建器, 模块, base::堆操作::堆_减法, 左标签, 左载荷, 右标签, 右载荷)
    }

    pub(crate) fn 运行时乘法(
        &mut self,
        构建器: &mut FunctionBuilder,
        模块: &mut ObjectModule,
        左标签: cranelift_codegen::ir::Value,
        左载荷: cranelift_codegen::ir::Value,
        右标签: cranelift_codegen::ir::Value,
        右载荷: cranelift_codegen::ir::Value,
    ) -> Result<(cranelift_codegen::ir::Value, cranelift_codegen::ir::Value), String> {
        self.运行时二元算术(构建器, 模块, base::堆操作::堆_乘法, 左标签, 左载荷, 右标签, 右载荷)
    }

    pub(crate) fn 运行时除法(
        &mut self,
        构建器: &mut FunctionBuilder,
        模块: &mut ObjectModule,
        左标签: cranelift_codegen::ir::Value,
        左载荷: cranelift_codegen::ir::Value,
        右标签: cranelift_codegen::ir::Value,
        右载荷: cranelift_codegen::ir::Value,
    ) -> Result<(cranelift_codegen::ir::Value, cranelift_codegen::ir::Value), String> {
        self.运行时二元算术(构建器, 模块, base::堆操作::堆_除法, 左标签, 左载荷, 右标签, 右载荷)
    }

    pub(crate) fn 运行时整除(
        &mut self,
        构建器: &mut FunctionBuilder,
        模块: &mut ObjectModule,
        左标签: cranelift_codegen::ir::Value,
        左载荷: cranelift_codegen::ir::Value,
        右标签: cranelift_codegen::ir::Value,
        右载荷: cranelift_codegen::ir::Value,
    ) -> Result<(cranelift_codegen::ir::Value, cranelift_codegen::ir::Value), String> {
        self.运行时二元算术(构建器, 模块, base::堆操作::堆_整除, 左标签, 左载荷, 右标签, 右载荷)
    }

    pub(crate) fn 运行时二元算术(
        &mut self,
        构建器: &mut FunctionBuilder,
        模块: &mut ObjectModule,
        操作码索引: usize,
        左标签: cranelift_codegen::ir::Value,
        左载荷: cranelift_codegen::ir::Value,
        右标签: cranelift_codegen::ir::Value,
        右载荷: cranelift_codegen::ir::Value,
    ) -> Result<(cranelift_codegen::ir::Value, cranelift_codegen::ir::Value), String> {
        // 布局与 编译堆操作 一致：argc=2 → 标签数组@缓冲, 值数组@缓冲+16。
        // 直接用 store 内置 offset 消除 iadd_imm，避免 Cranelift 优化引入的常量错乱。
        let 缓冲 = self.分配缓冲(构建器, 模块, 32)?;
        构建器.ins().store(MemFlags::new(), 左标签, 缓冲, 0);   // t[0] @ +0
        构建器.ins().store(MemFlags::new(), 右标签, 缓冲, 8);   // t[1] @ +8
        构建器.ins().store(MemFlags::new(), 左载荷, 缓冲, 16);  // v[0] @ +16
        构建器.ins().store(MemFlags::new(), 右载荷, 缓冲, 24);  // v[1] @ +24
        let 值数组指针 = 构建器.ins().iadd_imm(缓冲, 16);

        // 输出缓冲需容纳完整 运行时值（32字节：标签@0,整数@8,小数@16,句柄@24）。
        let 输出缓冲 = self.分配缓冲(构建器, 模块, 32)?;
        let 输出2缓冲 = self.分配缓冲(构建器, 模块, 32)?;

        let mut 签名 = Signature::new(CallConv::triple_default(&Triple::host()));
        for _ in 0..6 { 签名.params.push(AbiParam::new(types::I64)); }
        let 函数 = 模块.declare_function("智语言_堆操作", Linkage::Import, &签名)
            .map_err(|e| format!("声明堆操作函数失败: {}", e))?;
        let 调用 = 模块.declare_func_in_func(函数, &mut 构建器.func);
        let 操作码值 = 构建器.ins().iconst(types::I64, 操作码索引 as i64);
        let argc值 = 构建器.ins().iconst(types::I64, 2);
        构建器.ins().call(调用, &[操作码值, argc值, 缓冲, 值数组指针, 输出缓冲, 输出2缓冲]);

        // 按结果标签从对应字段读回载荷（与 编译堆操作 非取末尾路径对称）。
        Ok(self.读运行时值(构建器, 输出缓冲))
    }

    /// 通用堆单元操作：调用 C ABI `智语言_堆操作(op, argc, 标签_ptr, 值_ptr, 输出, 输出2)`。
    /// 输入参数布局与 `编译堆操作` 一致（标签块@0..argc*8，值块@16..16+argc*8），
    /// 输出自 `输出缓冲` 按结果标签读回 (标签,载荷)。用于 by-ref 后端：
    /// 取地址 / 间接加载 / 间接存储。
    pub(crate) fn 运行时堆单元(
        &mut self,
        构建器: &mut FunctionBuilder,
        模块: &mut ObjectModule,
        op: usize,
        argc: usize,
        args: &[(cranelift_codegen::ir::Value, cranelift_codegen::ir::Value)],
    ) -> Result<(cranelift_codegen::ir::Value, cranelift_codegen::ir::Value), String> {
        // 缓冲布局须与 编译堆操作 一致：标签区 [0, argc*8)，载荷区 [argc*8, 2*argc*8)。
        // 此前固定 32 字节 + 载荷偏移 16，对 argc≥3 时「arg0 载荷(offset 16)」与「arg2 标签(offset 16)」重叠，
        // 导致 创建闭包 的 func_idx 被后续捕获标签覆盖 → 闭包调用走错误分支。
        let 总字节 = (argc as i64) * 16;
        let 缓冲 = self.分配缓冲(构建器, 模块, 总字节)?;
        for (i, &(t, v)) in args.iter().enumerate() {
            构建器.ins().store(MemFlags::new(), t, 缓冲, (i as i32) * 8);
            构建器.ins().store(MemFlags::new(), v, 缓冲, (argc as i32) * 8 + (i as i32) * 8);
        }
        let 值数组指针 = 构建器.ins().iadd_imm(缓冲, (argc as i64) * 8);
        let 输出缓冲 = self.分配缓冲(构建器, 模块, 32)?;
        let 输出2缓冲 = self.分配缓冲(构建器, 模块, 32)?;

        let mut 签名 = Signature::new(CallConv::triple_default(&Triple::host()));
        for _ in 0..6 { 签名.params.push(AbiParam::new(types::I64)); }
        let 函数 = 模块.declare_function("智语言_堆操作", Linkage::Import, &签名)
            .map_err(|e| format!("声明堆操作函数失败: {}", e))?;
        let 调用 = 模块.declare_func_in_func(函数, &mut 构建器.func);
        let 操作码值 = 构建器.ins().iconst(types::I64, op as i64);
        let argc值 = 构建器.ins().iconst(types::I64, argc as i64);
        构建器.ins().call(调用, &[操作码值, argc值, 缓冲, 值数组指针, 输出缓冲, 输出2缓冲]);

        Ok(self.读运行时值(构建器, 输出缓冲))
    }

    /// ⚡双模式⚡ 整数算术快速路径：经 base C ABI `智语言_整数算术` 计算整数 OP 整数。
    /// 禁止在 翻译执行器 整数快速块内联 iadd/isub/imul——语义必须与 base::堆操作::整数算术 一致，
    /// 由 base 单源保证（编译器改 base 整数溢出语义时自动跟随，杜绝分叉）。
    pub(crate) fn 整数算术快速(
        &mut self,
        构建器: &mut FunctionBuilder,
        模块: &mut ObjectModule,
        操作码: usize,
        左载荷: cranelift_codegen::ir::Value,
        右载荷: cranelift_codegen::ir::Value,
    ) -> Result<cranelift_codegen::ir::Value, String> {
        let mut 签名 = Signature::new(CallConv::triple_default(&Triple::host()));
        for _ in 0..3 { 签名.params.push(AbiParam::new(types::I64)); }
        签名.returns.push(AbiParam::new(types::I64));
        let 函数 = 模块.declare_function("智语言_整数算术", Linkage::Import, &签名)
            .map_err(|e| format!("声明整数算术函数失败: {}", e))?;
        let 调用 = 模块.declare_func_in_func(函数, &mut 构建器.func);
        let 操作码值 = 构建器.ins().iconst(types::I64, 操作码 as i64);
        let 调用指令 = 构建器.ins().call(调用, &[操作码值, 左载荷, 右载荷]);
        Ok(构建器.inst_results(调用指令)[0])
    }

        pub(crate) fn 编译堆操作(&mut self, 构建器: &mut FunctionBuilder, 模块: &mut ObjectModule, 指令: &三地址码, 操作码索引: usize) -> Result<(), String> {
        let mut 操作数列表: Vec<操作数> = Vec::new();
        if !matches!(指令.操作数1, 操作数::无) { 操作数列表.push(指令.操作数1.clone()); }
        if let Some(o) = &指令.操作数2 { 操作数列表.push(o.clone()); }
        if let Some(o) = &指令.操作数3 { 操作数列表.push(o.clone()); }
        let argc = 操作数列表.len().min(4);

        let 总字节 = (argc as i64) * 16;
        let 缓冲 = self.分配缓冲(构建器, 模块, 总字节)?;
        let 标签数组指针 = 缓冲;
        let 值数组指针 = 构建器.ins().iadd_imm(缓冲, (argc as i64) * 8);
        for (i, op) in 操作数列表.iter().enumerate().take(argc) {
            // 字符串常量必须运行时分配为文本句柄（与 推送实参 一致），
            // 否则作为堆操作参数（如 映射存储 的键）时载荷为 0，引发键冲突/丢失。
            let (标签, 载荷) = match op {
                操作数::字符串常量(索引) => {
                    let 标签v = 构建器.ins().iconst(types::I64, 标签_句柄 as i64);
                    let 载荷v = self.分配字符串常量句柄(构建器, 模块, *索引)?;
                    (标签v, 载荷v)
                }
                _ => self.取操作数标签与值(构建器, op)?,
            };
            let 标签地址 = 构建器.ins().iadd_imm(标签数组指针, (i as i64) * 8);
            构建器.ins().store(MemFlags::new(), 标签, 标签地址, 0);
            let 载荷地址 = 构建器.ins().iadd_imm(值数组指针, (i as i64) * 8);
            构建器.ins().store(MemFlags::new(), 载荷, 载荷地址, 0);
        }

        // ⚡双模式⚡: 输出缓冲必须容纳完整 运行时值（32字节：标签@0,整数@8,小数@16,句柄@24），
        // 与 编译系统调用 一致。运行时 智语言_堆操作 通过 std::ptr::write 写入完整 32 字节结构，
        // 此前误分配 16 字节导致每次堆操作越界写入相邻堆内存 → 随机堆损坏崩溃(0xC0000374)。
        let 输出缓冲 = self.分配缓冲(构建器, 模块, 32)?;
        let 输出2缓冲 = self.分配缓冲(构建器, 模块, 32)?;

        let mut 签名 = Signature::new(CallConv::triple_default(&Triple::host()));
        for _ in 0..6 { 签名.params.push(AbiParam::new(types::I64)); }
        let 函数 = 模块.declare_function("智语言_堆操作", Linkage::Import, &签名)
            .map_err(|e| format!("声明堆操作函数失败: {}", e))?;
        let 调用 = 模块.declare_func_in_func(函数, &mut 构建器.func);
        let 操作码值 = 构建器.ins().iconst(types::I64, 操作码索引 as i64);
        let argc值 = 构建器.ins().iconst(types::I64, argc as i64);
        构建器.ins().call(调用, &[操作码值, argc值, 标签数组指针, 值数组指针, 输出缓冲, 输出2缓冲]);

        // 读出结果 (标签,载荷)。运行时已写出完整 运行时值 结构（32字节）：
        // 标签@0, 整数@8, 小数@16, 句柄@24。按标签从对应字段读取载荷，
        // 避免句柄/小数被误读为整数字段（offset8）而丢失（与 编译系统调用 对称）。
        // ⚡统一⚡ 数组取末尾(操作码5) 结果来自 输出2缓冲，其余来自 输出缓冲。
        let 结果缓冲 = if 操作码索引 == 5 { 输出2缓冲 } else { 输出缓冲 };
        let (结果标签, 结果载荷) = self.读运行时值(构建器, 结果缓冲);
        // ⚡双模式⚡: 异常检查改由 翻译指令 主循环依 操作码可抛 单表自动注入（见 翻译执行器.rs），
        //   此处不再手插，避免与循环注入双重注入。
        self.存储值对(构建器, &指令.结果, 结果标签, 结果载荷);
        Ok(())
    }

}
