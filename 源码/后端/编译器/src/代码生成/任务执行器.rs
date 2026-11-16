// 文件: 后端/编译器/src/代码生成/任务执行器.rs
// 职责: 底层表示 → Cranelift IR → 目标文件，完整支持结构体/数组/函数调用（含参数传递）和DWARF调试信息

use cranelift_codegen::ir::{types, AbiParam, InstBuilder, MemFlags, Signature, StackSlotData, StackSlotKind};
use cranelift_codegen::ir::condcodes::IntCC;
use cranelift_codegen::isa::CallConv;
use cranelift_codegen::Context;
use cranelift_frontend::FunctionBuilder;
use cranelift_module::{Linkage, Module, FuncId};
use cranelift_object::ObjectModule;
use target_lexicon::Triple;

use frontend::底层表示::{指令集::三地址码, 模块结构::底层表示模块};
use base::值类型::{标签_整数, 标签_句柄, 标签_空值};

/// 统一值表示：每个局部 = (标签, 载荷) 双槽 i64，与运行时 运行时值 / 智语言_堆操作 / 智语言_系统调用 同一套语言。
use crate::代码生成::编译器;

impl 编译器 {
    pub(crate) fn 生成任务thunk(
        &mut self,
        模块: &mut ObjectModule,
        函数索引: usize,
        返回类型: &frontend::底层表示::类型定义::底层表示类型,
    ) -> Result<FuncId, String> {
        // ⚡双模式⚡ 根据返回类型分支：空返回任务体签名 0 参数（不传 sret），非空返回任务体签名带 sret 指针。
        // 此前无条件传 sret 导致命名空返回函数任务（如 并发.启动任务(安静工作)）编译失败：got 1, expected 0。
        let 是空返回 = *返回类型 == frontend::底层表示::类型定义::底层表示类型::空值;
        let mut 签名 = Signature::new(CallConv::triple_default(&Triple::host()));
        签名.params.push(AbiParam::new(types::I64)); // 输出指针
        签名.params.push(AbiParam::new(types::I64)); // 参数指针（无参任务未使用）
        签名.params.push(AbiParam::new(types::I64)); // 数量（未使用）
        let 名称 = format!("任务thunk_{}", 函数索引);
        let 函数标识 = 模块
            .declare_function(&名称, Linkage::Export, &签名)
            .map_err(|e| format!("声明任务 thunk 失败: {}", e))?;

        let mut 上下文 = Context::new();
        上下文.func.signature = 签名.clone();
        let mut 构建上下文 = std::mem::take(&mut self.函数构建上下文);
        {
            let mut 构建器 = FunctionBuilder::new(&mut 上下文.func, &mut 构建上下文);
            let 入口块 = 构建器.create_block();
            构建器.append_block_params_for_function_params(入口块);
            构建器.switch_to_block(入口块);
            let 参数们 = 构建器.func.dfg.block_params(入口块);
            let 输出指针 = 参数们[0];

            let 任务体标识 = *self.函数名映射.get(&函数索引)
                .ok_or_else(|| format!("未知任务函数 {}", 函数索引))?;
            // D5 修复：任务体（非空返回）改用 sret——thunk 分配 16 字节栈槽作 sret，传地址调用，
            // 再从栈槽读回 (标签,载荷) 经 智语言_打包结果 写入输出指针。
            // 空返回任务体无 sret 参数，直接无参调用并把结果打包为空值。
            let 任务体调用 = 模块.declare_func_in_func(任务体标识, &mut 构建器.func);
            let (标签值, 载荷值) = if 是空返回 {
                let _ = 构建器.ins().call(任务体调用, &[]);
                let 空标签 = 构建器.ins().iconst(types::I64, base::值类型::标签_空值 as i64);
                let 零 = 构建器.ins().iconst(types::I64, 0);
                (空标签, 零)
            } else {
                let sret槽 = 构建器.create_sized_stack_slot(StackSlotData::new(StackSlotKind::ExplicitSlot, 32, 4));
                let sret地址 = 构建器.ins().stack_addr(types::I64, sret槽, 0);
                let _ = 构建器.ins().call(任务体调用, &[sret地址]);
                let 标签值 = 构建器.ins().load(types::I64, MemFlags::new(), sret地址, 0);
                let 载荷值 = 构建器.ins().load(types::I64, MemFlags::new(), sret地址, 8);
                (标签值, 载荷值)
            };

            // ⚡双模式⚡ D2：任务体若抛出异常（运行时 CALL 120 设线程局部异常，或经其 异常返回块 返回空值），
            // 将该异常打包为 拥有值::异常，使 任务等待(161) → 拥有值转运行时值 在等待线程重抛，
            // 与解释器子 VM 抛异常后由 任务等待 重抛一致（此前异常被转为文本串、不再传播）。
            let mut 待处理签名 = Signature::new(CallConv::triple_default(&Triple::host()));
            待处理签名.returns.push(AbiParam::new(types::I64));
            let 待处理函数 = 模块
                .declare_function("智语言_异常待处理", Linkage::Import, &待处理签名)
                .map_err(|e| format!("声明 智语言_异常待处理 失败: {}", e))?;
            let 待处理引用 = 模块.declare_func_in_func(待处理函数, &mut 构建器.func);
            let 待处理调用 = 构建器.ins().call(待处理引用, &[]);
            let 待处理标志 = 构建器.func.dfg.first_result(待处理调用);
            let 零 = 构建器.ins().iconst(types::I64, 0);
            let 有异常 = 构建器.ins().icmp(cranelift_codegen::ir::condcodes::IntCC::NotEqual, 待处理标志, 零);

            let 正常块 = 构建器.create_block();
            let 异常块 = 构建器.create_block();
            构建器.ins().brif(有异常, 异常块, &[], 正常块, &[]);

            构建器.switch_to_block(异常块);
            {
                let mut 打包签名 = Signature::new(CallConv::triple_default(&Triple::host()));
                打包签名.params.push(AbiParam::new(types::I64));
                let 打包函数 = 模块
                    .declare_function("智语言_打包当前异常", Linkage::Import, &打包签名)
                    .map_err(|e| format!("声明 智语言_打包当前异常 失败: {}", e))?;
                let 打包调用 = 模块.declare_func_in_func(打包函数, &mut 构建器.func);
                构建器.ins().call(打包调用, &[输出指针]);
            }
            构建器.ins().return_(&[]);

            构建器.switch_to_block(正常块);
            {
                let mut 打包签名 = Signature::new(CallConv::triple_default(&Triple::host()));
                打包签名.params.push(AbiParam::new(types::I64));
                打包签名.params.push(AbiParam::new(types::I64));
                打包签名.params.push(AbiParam::new(types::I64));
                let 打包函数 = 模块
                    .declare_function("智语言_打包结果", Linkage::Import, &打包签名)
                    .map_err(|e| format!("声明 智语言_打包结果 失败: {}", e))?;
                let 打包调用 = 模块.declare_func_in_func(打包函数, &mut 构建器.func);
                构建器.ins().call(打包调用, &[标签值, 载荷值, 输出指针]);
            }
            构建器.ins().return_(&[]);
            构建器.seal_all_blocks();
            构建器.finalize();
        }
        模块.define_function(函数标识, &mut 上下文)
            .map_err(|e| format!("定义任务 thunk 失败: {:#?}", e))?;
        Ok(函数标识)
    }

    // 生成HTTP处理器thunk（2026-08-19 阶段二已删除）：
    // HTTP 处理器统一经 base 执行后台用户函数 调度，编译模式回退到 回调桥
    // （回调候选集播种 http处理器集合，见 代码生成核心.rs 第1.6b 遍末尾）。

    /// 🆕 FFI 回调桥（编译模式）：生成「按用户函数索引调用」的 thunk，注册为 用户函数调用器，
    /// 承接 智语言_系统调用回调 → 执行用户函数回调 的最后一跳。
    /// 契约（显式输出指针，规避 Cranelift 返回 32 字节结构体 sret 歧义，与既有 thunk 一致）：
    ///   extern "C" fn(ctx: usize, 函数索引: usize, 参数指针: *const 运行时值, 数量: usize, 输出指针: *mut 运行时值)
    /// 分派逻辑：按 函数索引 匹配 回调候选函数集，加载 K 个 运行时值 实参（每个 32 字节）、按原生调用约定
    /// （sret + (标签,载荷) 对）调用本机函数、将结果经 智语言_打包运行时值 写回 输出指针；
    /// 未命中则写回空值（安全降级）。解释模式不调用此 thunk（走 解释器回调桥）。
    pub(crate) fn 生成回调桥(
        &mut self,
        模块: &mut ObjectModule,
        底层表示模块: &底层表示模块,
    ) -> Result<FuncId, String> {
        let 候选集 = self.回调候选函数集.clone();
        if 候选集.is_empty() {
            return Err("生成回调桥：无回调候选函数".to_string());
        }
        let mut 签名 = Signature::new(CallConv::triple_default(&Triple::host()));
        签名.params.push(AbiParam::new(types::I64)); // ctx（编译模式恒 0，thunk 自含函数表）
        签名.params.push(AbiParam::new(types::I64)); // 函数索引（≥200）
        签名.params.push(AbiParam::new(types::I64)); // 参数指针
        签名.params.push(AbiParam::new(types::I64)); // 数量
        签名.params.push(AbiParam::new(types::I64)); // 输出指针
        let 函数标识 = 模块
            .declare_function("智语言回调桥", Linkage::Export, &签名)
            .map_err(|e| format!("声明回调桥失败: {}", e))?;

        let mut 上下文 = Context::new();
        上下文.func.signature = 签名.clone();
        let mut 构建上下文 = std::mem::take(&mut self.函数构建上下文);
        {
            let mut 构建器 = FunctionBuilder::new(&mut 上下文.func, &mut 构建上下文);
            let 入口块 = 构建器.create_block();
            构建器.append_block_params_for_function_params(入口块);
            构建器.switch_to_block(入口块);
            let 参数们 = 构建器.func.dfg.block_params(入口块);
            let 函数索引 = 参数们[1];
            let 参数指针 = 参数们[2];
            let 输出指针 = 参数们[4];

            let n = 候选集.len();
            let 检查块们: Vec<_> = (0..n).map(|_| 构建器.create_block()).collect();
            let 案例块们: Vec<_> = (0..n).map(|_| 构建器.create_block()).collect();
            let 默认块 = 构建器.create_block();

            构建器.ins().jump(检查块们[0], &[]);

            for i in 0..n {
                构建器.switch_to_block(检查块们[i]);
                let 候选值 = 构建器.ins().iconst(types::I64, 候选集[i] as i64);
                let 命中 = 构建器.ins().icmp(IntCC::Equal, 函数索引, 候选值);
                构建器.ins().brif(命中, 案例块们[i], &[], 默认块, &[]);
            }

            for i in 0..n {
                构建器.switch_to_block(案例块们[i]);
                let 候选 = 候选集[i];
                let 原生标识 = *self
                    .函数名映射
                    .get(&候选)
                    .ok_or_else(|| format!("回调桥：未知函数 {}", 候选))?;
                let 原生函数 = 模块.declare_func_in_func(原生标识, &mut 构建器.func);
                let 返回类型 = self
                    .函数返回类型
                    .get(&候选)
                    .cloned()
                    .unwrap_or(frontend::底层表示::类型定义::底层表示类型::空值);
                let 有返回值 =
                    !matches!(返回类型, frontend::底层表示::类型定义::底层表示类型::空值);
                let 位置 = (候选 as usize).saturating_sub(frontend::底层表示::模块结构::用户函数索引基址);
                let 参数数 = 底层表示模块
                    .函数列表
                    .get(位置)
                    .map(|f| f.形参列表.len())
                    .unwrap_or(0);

                let mut 调用实参 = Vec::new();
                let mut sret地址 = None;
                if 有返回值 {
                    let sret槽 = 构建器.create_sized_stack_slot(StackSlotData::new(
                        StackSlotKind::ExplicitSlot,
                        16,
                        4,
                    ));
                    let s = 构建器.ins().stack_addr(types::I64, sret槽, 0);
                    sret地址 = Some(s);
                    调用实参.push(s);
                }
                for k in 0..参数数 {
                    let 偏移 = 构建器
                        .ins()
                        .iconst(types::I64, (k as i64) * 32);
                    let 基址 = 构建器.ins().iadd(参数指针, 偏移);
                    let (标签, 载荷) = self.读运行时值(&mut 构建器, 基址);
                    调用实参.push(标签);
                    调用实参.push(载荷);
                }
                构建器.ins().call(原生函数, &调用实参);
                let (结果标签, 结果载荷) = if let Some(s) = sret地址 {
                    let t = 构建器.ins().load(types::I64, MemFlags::new(), s, 0);
                    let p = 构建器.ins().load(types::I64, MemFlags::new(), s, 8);
                    (t, p)
                } else {
                    (
                        构建器.ins().iconst(types::I64, 标签_空值 as i64),
                        构建器.ins().iconst(types::I64, 0),
                    )
                };
                let mut 打包签名 = Signature::new(CallConv::triple_default(&Triple::host()));
                打包签名.params.push(AbiParam::new(types::I64));
                打包签名.params.push(AbiParam::new(types::I64));
                打包签名.params.push(AbiParam::new(types::I64));
                let 打包函数 = 模块
                    .declare_function("智语言_打包运行时值", Linkage::Import, &打包签名)
                    .map_err(|e| format!("声明打包运行时值失败: {}", e))?;
                let 打包调用 = 模块.declare_func_in_func(打包函数, &mut 构建器.func);
                构建器
                    .ins()
                    .call(打包调用, &[结果标签, 结果载荷, 输出指针]);
                构建器.ins().return_(&[]);
            }

            // 默认块：写回空值（安全降级）
            构建器.switch_to_block(默认块);
            let mut 打包签名 = Signature::new(CallConv::triple_default(&Triple::host()));
            打包签名.params.push(AbiParam::new(types::I64));
            打包签名.params.push(AbiParam::new(types::I64));
            打包签名.params.push(AbiParam::new(types::I64));
            let 打包函数 = 模块
                .declare_function("智语言_打包运行时值", Linkage::Import, &打包签名)
                .map_err(|e| format!("声明打包运行时值失败: {}", e))?;
            let 打包调用 = 模块.declare_func_in_func(打包函数, &mut 构建器.func);
            let 空标签 = 构建器.ins().iconst(types::I64, 标签_空值 as i64);
            let 空载荷 = 构建器.ins().iconst(types::I64, 0);
            构建器.ins().call(打包调用, &[空标签, 空载荷, 输出指针]);
            构建器.ins().return_(&[]);

            构建器.seal_all_blocks();
            构建器.finalize();
        }
        模块.define_function(函数标识, &mut 上下文)
            .map_err(|e| format!("定义回调桥失败: {:#?}", e))?;
        Ok(函数标识)
    }

    /// 导出块：为导出函数生成**规范 §6.5.3 规定的真实 C 签名** thunk。
    ///   整数 → `int64_t`，小数 → `double`，逻辑 → `bool`，文本 → `const char*`，空值 → `void`
    ///
    /// 对内部被导出函数的调用遵守统一值表示 ABI（单一真相源 = `函数返回类型` 映射）：
    ///   非空返回 ⇒ 形参最前有一个 sret 指针；空值返回 ⇒ 无 sret（void）。
    /// 边界编组只做两件事：标量在本 thunk 内直接落 `(标签, 载荷)`；文本转调 base 编组层
    ///   （`智语言_文本入参` / `智语言_文本出参`），使 C 字符串 ↔ 堆文本 的字节搬运只有一处实现。
    /// 文本入参产生的临时堆文本按值语义由被调用方深拷贝，这里按「宁可泄漏、绝不误回收」不释放。
    ///
    /// 【2026-10-21 定稿】扁平契约 `void f(智语言运行时值*, 数量, 输入数组*)` 是运行时**内部**
    /// 约定（规范未定义、C 侧无法自然调用、且会与真实签名同符号冲突），已不再作为导出符号。
    pub(crate) fn 生成导出thunk(
        &mut self,
        模块: &mut ObjectModule,
        函数索引: usize,
        形参类型: &[frontend::底层表示::类型定义::底层表示类型],
        声明返回类型: &frontend::底层表示::类型定义::底层表示类型,
        c符号: &str,
    ) -> Result<FuncId, String> {
        use frontend::底层表示::类型定义::底层表示类型;

        // 各导出类型的 C 形参/返回值映射。导出块只允许核心类型（规范 §6.5.3），
        // 其余类型（结构体/数组/映射/枚举/接口/指针）显式报错——不提供语义错误的假能力。
        fn 映射C形参(类型: &底层表示类型, c符号: &str) -> Result<cranelift_codegen::ir::Type, String> {
            match 类型 {
                底层表示类型::整数64 => Ok(types::I64),
                底层表示类型::小数64 => Ok(types::F64),
                底层表示类型::逻辑 => Ok(types::I8),
                底层表示类型::文本 => Ok(types::I64), // const char*
                底层表示类型::空值 => Err(format!(
                    "导出函数【{}】的形参不能是空值：C 没有 void 形参", c符号
                )),
                其它 => Err(format!(
                    "导出函数【{}】的形参类型 {:?} 不受支持：导出块仅允许核心类型（整数/小数/文本/逻辑/空值）",
                    c符号, 其它
                )),
            }
        }
        fn 映射C返回(类型: &底层表示类型, c符号: &str) -> Result<Option<cranelift_codegen::ir::Type>, String> {
            match 类型 {
                底层表示类型::空值 => Ok(None), // void
                底层表示类型::整数64 => Ok(Some(types::I64)),
                底层表示类型::小数64 => Ok(Some(types::F64)),
                底层表示类型::逻辑 => Ok(Some(types::I8)),
                底层表示类型::文本 => Ok(Some(types::I64)),
                其它 => Err(format!(
                    "导出函数【{}】的返回类型 {:?} 不受支持：导出块仅允许核心类型（整数/小数/文本/逻辑/空值）",
                    c符号, 其它
                )),
            }
        }

        // 公开 C 签名取「声明的返回类型」（与 生成导出头文件 同源，保证头文件与二进制一致）；
        // 内部 sret 判定取「覆盖后的返回类型」（与 编译函数/所有调用点 同源，保证实参个数正确）。
        let 返回类型 = 声明返回类型.clone();
        let 内部返回类型 = self.函数返回类型.get(&函数索引).cloned().unwrap_or(底层表示类型::空值);
        let 是空返回 = 内部返回类型 == 底层表示类型::空值;
        if 返回类型 != 底层表示类型::空值 && 是空返回 {
            return Err(format!(
                "导出函数【{}】声明的返回类型 {:?} 与内部返回类型不一致（内部为 空值）",
                c符号, 返回类型
            ));
        }

        let mut 签名 = Signature::new(CallConv::triple_default(&Triple::host()));
        for 类型 in 形参类型 {
            签名.params.push(AbiParam::new(映射C形参(类型, c符号)?));
        }
        if let Some(c返回) = 映射C返回(&返回类型, c符号)? {
            签名.returns.push(AbiParam::new(c返回));
        }

        // §6.5.3：导出符号名取 `关联` 指定的 C 符号名；缺省为智语言函数名
        let 函数标识 = 模块
            .declare_function(c符号, Linkage::Export, &签名)
            .map_err(|e| format!("声明导出 thunk 失败: {}", e))?;

        let mut 上下文 = Context::new();
        上下文.func.signature = 签名.clone();
        let mut 构建上下文 = std::mem::take(&mut self.函数构建上下文);
        {
            let mut 构建器 = FunctionBuilder::new(&mut 上下文.func, &mut 构建上下文);
            let 入口块 = 构建器.create_block();
            构建器.append_block_params_for_function_params(入口块);
            构建器.switch_to_block(入口块);
            let 参数们: Vec<cranelift_codegen::ir::Value> =
                构建器.func.dfg.block_params(入口块).to_vec();
            let 标志 = MemFlags::new();

            // sret 栈槽（32 字节：标签 + 载荷）——仅「非空返回」的被导出函数才有 sret 形参
            let sret地址 = if 是空返回 {
                None
            } else {
                let sret槽 = 构建器.create_sized_stack_slot(StackSlotData::new(StackSlotKind::ExplicitSlot, 32, 4));
                Some(构建器.ins().stack_addr(types::I64, sret槽, 0))
            };
            // 标量 ↔ 位模式转换槽（小数按 f64 位模式承载于 整数载荷）
            let 转换槽 = 构建器.create_sized_stack_slot(StackSlotData::new(StackSlotKind::ExplicitSlot, 8, 3));
            let 转换槽地址 = 构建器.ins().stack_addr(types::I64, 转换槽, 0);
            // 文本编组临时槽（32 字节 运行时值）
            let 编组槽 = 构建器.create_sized_stack_slot(StackSlotData::new(StackSlotKind::ExplicitSlot, 32, 4));
            let 编组槽地址 = 构建器.ins().stack_addr(types::I64, 编组槽, 0);

            // 文本编组 helper（按需声明，未使用时不产生导入引用）
            let 文本入参引用 = if 形参类型.iter().any(|t| t == &底层表示类型::文本) {
                let mut 入参签名 = Signature::new(CallConv::triple_default(&Triple::host()));
                入参签名.params.push(AbiParam::new(types::I64)); // const char*
                入参签名.params.push(AbiParam::new(types::I64)); // 输出 运行时值 槽
                let 函数 = 模块.declare_function("智语言_文本入参", Linkage::Import, &入参签名)
                    .map_err(|e| format!("声明文本入参失败: {}", e))?;
                Some(模块.declare_func_in_func(函数, &mut 构建器.func))
            } else {
                None
            };
            let 文本出参引用 = if 返回类型 == 底层表示类型::文本 {
                let mut 出参签名 = Signature::new(CallConv::triple_default(&Triple::host()));
                出参签名.params.push(AbiParam::new(types::I64)); // 标签
                出参签名.params.push(AbiParam::new(types::I64)); // 载荷
                出参签名.returns.push(AbiParam::new(types::I64)); // const char*
                let 函数 = 模块.declare_function("智语言_文本出参", Linkage::Import, &出参签名)
                    .map_err(|e| format!("声明文本出参失败: {}", e))?;
                Some(模块.declare_func_in_func(函数, &mut 构建器.func))
            } else {
                None
            };

            // 实参序 = [sret?] + 每形参 (标签, 载荷)；无 sret 时不占位。
            let mut 调用参数 = Vec::new();
            if let Some(地址) = sret地址 {
                调用参数.push(地址);
            }
            for (i, 类型) in 形参类型.iter().enumerate() {
                let 值 = 参数们[i];
                match 类型 {
                    底层表示类型::整数64 => {
                        调用参数.push(构建器.ins().iconst(types::I64, base::值类型::标签_整数 as i64));
                        调用参数.push(值);
                    }
                    底层表示类型::小数64 => {
                        // 小数的内部载荷 = f64 位模式（与 读运行时值/写运行时值 的 offset16 契约一致）
                        构建器.ins().store(标志, 值, 转换槽地址, 0);
                        let 位模式 = 构建器.ins().load(types::I64, 标志, 转换槽地址, 0);
                        调用参数.push(构建器.ins().iconst(types::I64, base::值类型::标签_小数 as i64));
                        调用参数.push(位模式);
                    }
                    底层表示类型::逻辑 => {
                        调用参数.push(构建器.ins().iconst(types::I64, base::值类型::标签_逻辑 as i64));
                        let 扩展 = 构建器.ins().uextend(types::I64, 值);
                        调用参数.push(扩展);
                    }
                    底层表示类型::文本 => {
                        let 引用 = 文本入参引用.ok_or_else(|| "文本入参 helper 未声明".to_string())?;
                        构建器.ins().call(引用, &[值, 编组槽地址]);
                        let (标签, 载荷) = self.读运行时值(&mut 构建器, 编组槽地址);
                        调用参数.push(标签);
                        调用参数.push(载荷);
                    }
                    其它 => return Err(format!(
                        "导出函数【{}】的形参类型 {:?} 不受支持", c符号, 其它
                    )),
                }
            }

            // 调用真实函数（[sret?] + 全部 (标签, 载荷)）
            let 实函数标识 = *self.函数名映射.get(&函数索引)
                .ok_or_else(|| format!("未知导出函数索引 {}", 函数索引))?;
            let 调用 = 模块.declare_func_in_func(实函数标识, &mut 构建器.func);
            构建器.ins().call(调用, &调用参数);

            // 读回 (标签, 载荷)：sret 约定把标签/载荷落在 offset0/8
            let (返回标签, 返回载荷) = if let Some(地址) = sret地址 {
                (
                    构建器.ins().load(types::I64, 标志, 地址, 0),
                    构建器.ins().load(types::I64, 标志, 地址, 8),
                )
            } else {
                let 空标签 = 构建器.ins().iconst(types::I64, base::值类型::标签_空值 as i64);
                let 零载荷 = 构建器.ins().iconst(types::I64, 0);
                (空标签, 零载荷)
            };

            // 返回值按规范 C 类型写回
            match &返回类型 {
                底层表示类型::空值 => {
                    构建器.ins().return_(&[]);
                }
                底层表示类型::整数64 => {
                    构建器.ins().return_(&[返回载荷]);
                }
                底层表示类型::小数64 => {
                    构建器.ins().store(标志, 返回载荷, 转换槽地址, 0);
                    let 小数 = 构建器.ins().load(types::F64, 标志, 转换槽地址, 0);
                    构建器.ins().return_(&[小数]);
                }
                底层表示类型::逻辑 => {
                    let 零 = 构建器.ins().iconst(types::I64, 0);
                    let 真值 = 构建器.ins().icmp(cranelift_codegen::ir::condcodes::IntCC::NotEqual, 返回载荷, 零);
                    构建器.ins().return_(&[真值]);
                }
                底层表示类型::文本 => {
                    let 引用 = 文本出参引用.ok_or_else(|| "文本出参 helper 未声明".to_string())?;
                    let 调用结果 = 构建器.ins().call(引用, &[返回标签, 返回载荷]);
                    let 指针 = 构建器.inst_results(调用结果)[0];
                    构建器.ins().return_(&[指针]);
                }
                其它 => return Err(format!(
                    "导出函数【{}】的返回类型 {:?} 不受支持", c符号, 其它
                )),
            }

            构建器.seal_all_blocks();
            构建器.finalize();
        }
        模块.define_function(函数标识, &mut 上下文)
            .map_err(|e| format!("定义导出 thunk 失败: {:#?}", e))?;
        Ok(函数标识)
    }

    pub(crate) fn 生成任务thunk闭包(
        &mut self,
        模块: &mut ObjectModule,
        参数数: usize,
    ) -> Result<FuncId, String> {
        let mut 签名 = Signature::new(CallConv::triple_default(&Triple::host()));
        签名.params.push(AbiParam::new(types::I64)); // 输出指针
        签名.params.push(AbiParam::new(types::I64)); // 参数指针（运行时值数组）
        签名.params.push(AbiParam::new(types::I64)); // 数量
        // 无返回值(void) — 与 生成任务thunk 一致
        let 名称 = format!("闭包任务thunk_{}", 参数数);
        let 函数标识 = 模块
            .declare_function(&名称, Linkage::Export, &签名)
            .map_err(|e| format!("声明闭包任务 thunk 失败: {}", e))?;

        let mut 上下文 = Context::new();
        上下文.func.signature = 签名.clone();
        let mut 构建上下文 = std::mem::take(&mut self.函数构建上下文);
        {
            let mut 构建器 = FunctionBuilder::new(&mut 上下文.func, &mut 构建上下文);
            let 入口块 = 构建器.create_block();
            构建器.append_block_params_for_function_params(入口块);
            构建器.switch_to_block(入口块);
            let 参数们 = 构建器.func.dfg.block_params(入口块);
            let 输出指针 = 参数们[0];
            let 参数指针 = 参数们[1];
            let _数量 = 参数们[2];

            let 整数标签 = 构建器.ins().iconst(types::I64, 标签_整数 as i64);
            let 闭包标签 = 构建器.ins().iconst(types::I64, 标签_句柄 as i64);
            let 闭包句柄 = 构建器.ins().load(types::I64, MemFlags::new(), 参数指针, 8);
            // 🔁 阶段 10（2026-09-23）**闭包表示统一**：闭包值 = `堆数据::闭包环境`
            //   ⇒ 经 base 专用入口取函数索引（原先是"按数组下标 1 读 func_idx"）✓
            let (_, func_idx值) = self.运行时堆单元(&mut 构建器, 模块,
                base::堆操作::堆_闭包函数索引, 1,
                &[(闭包标签, 闭包句柄)])?;

            if self.闭包索引列表.is_empty() {
                构建器.ins().return_(&[]);
            } else {
                // ⚡仅分发「闭包实参数 == 本 thunk 参数数」的闭包：
                // 任务启动的闭包（无参/单参）与其他任意签名闭包（如 加法器 2 参）共存时，
                // 若不按签名过滤，thunk 会按本参数数调用其他签名闭包 → Verifier 参数数不匹配。
                let 匹配闭包列表: Vec<usize> = self.闭包索引列表.iter()
                    .copied()
                    .filter(|索引| {
                        self.闭包参数数映射.get(索引).copied().unwrap_or(0) == 参数数
                    })
                    .collect();
                if 匹配闭包列表.is_empty() {
                    构建器.ins().return_(&[]);
                } else {
                let 合并块 = 构建器.create_block();
                let 错误块 = 构建器.create_block();
                let n = 匹配闭包列表.len();
                let mut 匹配块_ids = Vec::new();
                for _ in 0..n { 匹配块_ids.push(构建器.create_block()); }
                let mut 下一比较块_ids = Vec::new();
                for i in 0..n {
                    下一比较块_ids.push(if i < n - 1 { 构建器.create_block() } else { 错误块 });
                }
                for i in 0..n {
                    let 比较块 = if i == 0 { 入口块 } else { 下一比较块_ids[i - 1] };
                    if i > 0 { 构建器.switch_to_block(比较块); }
                    let 索引常量 = 构建器.ins().iconst(types::I64, 匹配闭包列表[i] as i64);
                    let 相等 = 构建器.ins().icmp(
                        cranelift_codegen::ir::condcodes::IntCC::Equal, func_idx值, 索引常量);
                    构建器.ins().brif(相等, 匹配块_ids[i], &[], 下一比较块_ids[i], &[]);
                    构建器.seal_block(比较块);

                    // 匹配块：调用该闭包的编译函数
                    构建器.switch_to_block(匹配块_ids[i]);
                    let 函数标识 = *self.函数名映射.get(&匹配闭包列表[i])
                        .ok_or_else(|| format!("闭包任务thunk：未知闭包函数 {}", 匹配闭包列表[i]))?;
                    let 返回类型 = self.函数返回类型.get(&匹配闭包列表[i])
                        .cloned().unwrap_or(frontend::底层表示::类型定义::底层表示类型::空值);
                    let 有返回值 = !matches!(返回类型, frontend::底层表示::类型定义::底层表示类型::空值);

                    let mut 调用参数 = vec![];
                    // 仅非空值返回时分配 sret 并传地址
                    if 有返回值 {
                        let sret槽 = 构建器.create_sized_stack_slot(
                            StackSlotData::new(StackSlotKind::ExplicitSlot, 16, 4));
                        let sret地址 = 构建器.ins().stack_addr(types::I64, sret槽, 0);
                        调用参数.push(sret地址);
                    }
                    // 加载捕获：闭包数组索引 3+j (1-based)
                    let 该闭包捕获数 = *self.闭包捕获数映射.get(&匹配闭包列表[i]).unwrap_or(&0);
                    for j in 0..该闭包捕获数 {
                        // 🔁 阶段 10（2026-09-23）：闭包环境按 **0-based** 下标读捕获 ✓
                        let 下标常量 = 构建器.ins().iconst(types::I64, j as i64);
                        let (c标签, c载荷) = self.运行时堆单元(&mut 构建器, 模块,
                            base::堆操作::堆_闭包捕获值, 2,
                            &[(闭包标签, 闭包句柄), (整数标签, 下标常量)])?;
                        调用参数.push(c标签);
                        调用参数.push(c载荷);
                    }
                    // 加载任务参数：参数数组[1..]（元素0=闭包句柄，元素1..=任务参数）
                    for k in 0..参数数 {
                        let 元素偏移 = ((k + 1) as i64) * 32;
                        let 元素地址 = 构建器.ins().iadd_imm(参数指针, 元素偏移);
                        let a标签 = 构建器.ins().load(types::I64, MemFlags::new(), 元素地址, 0);
                        let a载荷 = 构建器.ins().load(types::I64, MemFlags::new(), 元素地址, 8);
                        调用参数.push(a标签);
                        调用参数.push(a载荷);
                    }
                    let 函数引用 = 模块.declare_func_in_func(函数标识, &mut 构建器.func);
                    构建器.ins().call(函数引用, &调用参数);

                    // 读取 sret 结果（仅非空值返回时可用）
                    let 结果标签;
                    let 结果载荷;
                    if 有返回值 {
                        结果标签 = 构建器.ins().load(types::I64, MemFlags::new(), 调用参数[0], 0);
                        结果载荷 = 构建器.ins().load(types::I64, MemFlags::new(), 调用参数[0], 8);
                    } else {
                        结果标签 = 构建器.ins().iconst(types::I64, 标签_空值 as i64);
                        结果载荷 = 构建器.ins().iconst(types::I64, 0);
                    }

                    // 异常检查：任务体若抛出异常，打包为 拥有值::异常
                    let mut 待处理签名 = Signature::new(CallConv::triple_default(&Triple::host()));
                    待处理签名.returns.push(AbiParam::new(types::I64));
                    let 待处理函数 = 模块
                        .declare_function("智语言_异常待处理", Linkage::Import, &待处理签名)
                        .map_err(|e| format!("声明 智语言_异常待处理 失败: {}", e))?;
                    let 待处理引用 = 模块.declare_func_in_func(待处理函数, &mut 构建器.func);
                    let 待处理调用 = 构建器.ins().call(待处理引用, &[]);
                    let 待处理标志 = 构建器.func.dfg.first_result(待处理调用);
                    let 零 = 构建器.ins().iconst(types::I64, 0);
                    let 有异常 = 构建器.ins().icmp(
                        cranelift_codegen::ir::condcodes::IntCC::NotEqual, 待处理标志, 零);
                    let 正常块 = 构建器.create_block();
                    let 异常块 = 构建器.create_block();
                    构建器.ins().brif(有异常, 异常块, &[], 正常块, &[]);

                    构建器.switch_to_block(异常块);
                    {
                        let mut 打包签名 = Signature::new(CallConv::triple_default(&Triple::host()));
                        打包签名.params.push(AbiParam::new(types::I64));
                        let 打包函数 = 模块
                            .declare_function("智语言_打包当前异常", Linkage::Import, &打包签名)
                            .map_err(|e| format!("声明 智语言_打包当前异常 失败: {}", e))?;
                        let 打包调用 = 模块.declare_func_in_func(打包函数, &mut 构建器.func);
                        构建器.ins().call(打包调用, &[输出指针]);
                    }
                    构建器.ins().jump(合并块, &[]);
                    构建器.seal_block(异常块);

                    构建器.switch_to_block(正常块);
                    if 有返回值 {
                        let mut 打包签名 = Signature::new(CallConv::triple_default(&Triple::host()));
                        打包签名.params.push(AbiParam::new(types::I64));
                        打包签名.params.push(AbiParam::new(types::I64));
                        打包签名.params.push(AbiParam::new(types::I64));
                        let 打包函数 = 模块
                            .declare_function("智语言_打包结果", Linkage::Import, &打包签名)
                            .map_err(|e| format!("声明 智语言_打包结果 失败: {}", e))?;
                        let 打包调用 = 模块.declare_func_in_func(打包函数, &mut 构建器.func);
                        构建器.ins().call(打包调用, &[结果标签, 结果载荷, 输出指针]);
                    }
                    构建器.ins().jump(合并块, &[]);
                    构建器.seal_block(正常块);
                    构建器.seal_block(匹配块_ids[i]);
                }
                // 错误块：无匹配闭包 → void return
                构建器.switch_to_block(错误块);
                构建器.ins().jump(合并块, &[]);
                构建器.seal_block(错误块);
                // 合并块：void return
                构建器.switch_to_block(合并块);
                构建器.ins().return_(&[]);
                构建器.seal_block(合并块);
                }
            }
            构建器.seal_all_blocks();
            构建器.finalize();
        }
        self.函数构建上下文 = 构建上下文;
        模块.define_function(函数标识, &mut 上下文)
            .map_err(|e| format!("定义闭包任务 thunk 失败: {:#?}", e))?;
        Ok(函数标识)
    }

    pub(crate) fn 编译任务系统调用(
        &mut self,
        构建器: &mut FunctionBuilder,
        模块: &mut ObjectModule,
        指令: &三地址码,
        索引: usize,
        参数值: &[(cranelift_codegen::ir::Value, cranelift_codegen::ir::Value)],
    ) -> Result<(), String> {
        let argc = 参数值.len();
        let 数组字节 = (argc as i64) * 32;
        let 数组指针 = self.分配缓冲(构建器, 模块, 数组字节)?;

        for (i, (标签值, 载荷值)) in 参数值.iter().enumerate() {
            let 基址 = 构建器.ins().iadd_imm(数组指针, (i as i64) * 32);
            构建器.ins().store(MemFlags::new(), *标签值, 基址, 0);
            构建器.ins().store(MemFlags::new(), *载荷值, 基址, 8);
            构建器.ins().store(MemFlags::new(), *载荷值, 基址, 16);
            构建器.ins().store(MemFlags::new(), *载荷值, 基址, 24);
        }

        let 输出缓冲 = self.分配缓冲(构建器, 模块, 32)?;
        let 索引值 = 构建器.ins().iconst(types::I64, 索引 as i64);
        let argc值 = 构建器.ins().iconst(types::I64, argc as i64);

        let 系统调用函数 = self.取系统调用函数(模块)?;
        let 调用 = 模块.declare_func_in_func(系统调用函数, &mut 构建器.func);
        构建器.ins().call(调用, &[索引值, 数组指针, argc值, 输出缓冲]);

        // ⚡统一⚡ 读回运行时值 复用 操作数执行器::读运行时值（按标签从对应字段选载荷）
        let (结果标签, 结果载荷) = self.读运行时值(构建器, 输出缓冲);
        self.存储值对(构建器, &指令.结果, 结果标签, 结果载荷);

        Ok(())
    }

}
