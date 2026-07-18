# LingShu CIL v2 架构评审报告

> 评审日期: 2026-07-18
> 对标: Claude Code / Codex CLI / Aider
> 目标: 10 个以内核心模块实现完整的 Coding Agent Runtime

---

## 一、当前架构总览

```
lingshu-cil/src/
├── main.rs      (104行)   CLI 入口 + REPL 循环
├── cil.rs       (482行)   ⚠️ GOD MODULE
├── commands.rs  (127行)   命令解析器
├── tools.rs     (352行)   工具分发器
├── context.rs   (247行)   工作区扫描
├── llm.rs       (144行)   LLM 客户端
├── model.rs     (226行)   类型定义 (15个导出类型)
├── markdown.rs  (307行)   💀 死代码
├── logging.rs   ( 67行)   ⚠️ 过度设计
├── mcp.rs       (1189行)  ⚠️ 功能蔓延
└── Cargo.toml   ( 25行)   15 依赖, 5 个未使用
─────────────────────────────────────────
总计: 3245 行  |  9 模块  |  15 导出类型
```

---

## 二、模块冗余分析

### 2.1 两套平行的命令系统

```
用户输入 → commands.rs → cil.rs::cmd_*()           [用户命令路径]
                    ↓
LLM 调用 → tools::execute_tool() → tools::execute_*()  [工具调用路径]
```

**问题**: cil.rs 有 15 个 cmd_* 函数直接执行操作, tools.rs 有 9 个 execute_* 函数做同样的事。
cmd_open() 和 execute_read_file() 都是读文件, 但签名不同、路径不同、错误处理不同。

**结论**: 合并为同一套 Tool 系统。

### 2.2 cil.rs — God Module (482行)

CilRuntime 承担 5 个职责:
1. 状态管理 — 项目路径、配置、任务、内存
2. 命令执行 — 15 个 cmd_* 方法
3. AI Agent — run_ai_task() 工具循环
4. LLM 编排 — stream_to_string()
5. 项目管理 — 目录切换、上下文扫描

### 2.3 mcp.rs — 功能蔓延 (1189行, 占代码库 37%)

| 内部组件 | 行数 | 评估 |
|---|---|---|
| JSON-RPC 协议 | ~80 | ✅ 必需 |
| Resource 定义 | ~50 | ✅ 必需 |
| Tool 定义 | ~120 | ⚠️ chat tool 阻塞事件循环 |
| CircuitBreaker | ~80 | 💀 从未触发 |
| SessionManager | ~120 | 💀 record_usage/get_session/all_sessions 未调用 |
| UsageTracker | ~100 | 💀 circuit_breaker_mut 未调用 |
| Model Catalog | ~150 | ⚠️ 与 model.rs 重复 |
| call_deepseek_api | ~80 | ⚠️ blocking reqwest 阻塞 |

### 2.4 依赖膨胀

| 依赖 | 状态 | 说明 |
|---|---|---|
| ratatui / crossterm / syntect | 💀 未使用 | 仅 markdown.rs 引用 |
| tokio / uuid | 💀 未使用 | 全局无引用 |
| fuzzy-matcher | ⚠️ 过剩 | 可用 grep 替代 |

---

## 三、缺失核心能力 (vs Claude Code)

### P0 — 必须有

| 能力 | Claude Code | LingShu CIL | 差距 |
|---|---|---|---|
| 精确文件编辑 | SEARCH/REPLACE block | 简单 string.replace | ❌ |
| 差异应用+撤销 | git apply + checkout | 无 | ❌ |
| 项目理解 | AST+依赖图+类型感知 | 纯文件列表 | ❌ |
| 工具去重 | 自动去重连续相同调用 | 无 | ❌ |
| 错误恢复 | 重试+回退策略 | 仅返回错误字符串 | ❌ |
| 子代理 | 并行子任务 | 无 | ❌ |

### P1 — 应该有

| 能力 | Claude Code | LingShu CIL | 差距 |
|---|---|---|---|
| 成本跟踪 | 每次 LLM 调用显示 token | 无 | ❌ |
| 配置系统 | .clauderc / 环境变量 | 硬编码 | ❌ |
| 会话持久化 | 自动保存/恢复 | /memory 手动 | ❌ |
| 交互式审批 | 危险操作确认 | 无 | ❌ |
| LSP 集成 | 跳转定义、类型查询 | 无 | ❌ |

---

## 四、建议删除的模块

| 模块 | 行数 | 理由 | 操作 |
|---|---|---|---|
| markdown.rs | 307 | 全部死代码, 无引用 | 🗑️ 删除 |
| mcp.rs | 1189 | 功能独立但膨胀严重 | 🔄 精简到 ~200 行 |
| logging.rs | 67 | buffer 从未读取 | 🔄 替换为 stderr eprintln! |

---

## 五、建议保留的核心模块 (≤10个)

```
lingshu-cil/src/
├── main.rs       CLI Entry
├── agent.rs      🆕 Agent 核心循环 (Plan → Act → Observe)
├── cmds.rs       🆕 用户命令 → Tool 映射
├── tools/
│   ├── mod.rs    🆕 Tool trait + Registry
│   ├── file.rs   🆕 文件操作 (read/write/edit/patch)
│   ├── shell.rs  🆕 Shell/Git/Cargo 执行
│   └── search.rs 🆕 代码搜索
├── llm.rs        LLM 客户端
├── ctx.rs        🆕 项目理解
├── patch.rs      🆕 Diff 生成 + 应用 + 撤销
└── types.rs      🆕 精简类型 (≤8个)
```

### 5.1 模块依赖图

```
main.rs
  │
  ├── cmds.rs ──→ agent.rs ←── llm.rs
  │                 │           │
  │                 │     tools/mod.rs
  │                 │       ├── file.rs
  │                 │       ├── shell.rs
  │                 │       └── search.rs
  │                 │
  │                 ├── patch.rs
  │                 └── ctx.rs
  │
  └── types.rs  ←── (所有模块共享)
```

无循环依赖。agent.rs 是唯一编排者。

### 5.2 Agent 核心循环

```
                    ┌──────────────────────┐
                    │  1. 接收用户输入/任务  │
                    └──────────┬───────────┘
                               ↓
                    ┌──────────────────────┐
                    │  2. LLM 思考 + 决策   │ ← llm.rs
                    └──────────┬───────────┘
                               ↓
              ┌────────────────────────────────┐
              │  3. 解析 Tool Call             │
              │     有? ─→ 4. 执行工具 ─→ 回2  │ ← tools/
              │     无? ─→ 5. 输出最终答案      │
              └────────────────────────────────┘
                               ↓
                    ┌──────────────────────┐
                    │  6. Patch 应用       │ ← patch.rs
                    │  7. 验证 (cargo check)│
                    │  8. 记录 Session     │
                    └────────────────────────┘
```

### 5.3 关键设计决策

**决策1: 统一命令和工具**
```
用户 "/open src/main.rs" → cmds::handle("/open src/main.rs")
LLM 调用 "read_file"    → tools::execute("read_file", args)
                            ↓
                      tools/file.rs::read("src/main.rs")
```
**决策2: Patch-first 编辑**
```
LLM 产出 → patch.rs::generate_diff(old, new)
         → patch.rs::apply(diff)
         → patch.rs::revert(diff)   // 支持撤销
```
**决策3: 精简 types.rs 到 8 个类型**
```
ToolCall, ToolResult, ToolDef, LlmMessage,
ModelConfig, Patch, Session, StreamEvent
```
删除: PermissionMode, Task, TaskStatus, ModelPricing, Delta, DeltaToolCall, DeltaFunction, StreamChunk, StreamChoice

---

## 六、开发路线图

### Phase 1 — 架构清理 (1-2天)
```
Day 1 ─ 删除 markdown.rs, 精简 mcp.rs
       ─ 清理 Cargo.toml 依赖
       ─ 创建 types.rs (8个类型)
       ─ cargo check + clippy 零告警

Day 2 ─ 重构 cil.rs → agent.rs + cmds.rs
       ─ 合并 cmd_* 和 tools::execute_* 
       ─ 测试: 所有命令可用
```

### Phase 2 — 核心 Agent (3-5天)
```
Day 3 ─ patch.rs: diff 生成+应用+撤销
       ─ tool 执行闭环验证

Day 4 ─ llm.rs: tool calling 端到端
       ─ agent.rs: Plan→Act→Observe 循环

Day 5 ─ 错误恢复 (重试+回退)
       ─ 成本跟踪 (token 计数)
```

### Phase 3 — 项目理解 (2-3天)
```
Day 6 ─ ctx.rs: 依赖图解析
       ─ 语言检测 + 文件路由

Day 7 ─ 智能上下文窗口
       ─ LSP 轻量集成
```

### Phase 4 — 生产化 (2-3天)
```
Day 8 ─ config.rs: 配置文件 + 密钥管理
       ─ session.rs: 持久化 + 恢复

Day 9 ─ 交互式审批
       ─ 子代理调度
```

---

## 七、总结

| 指标 | 当前 (v2) | 目标 (v3) | 改进 |
|---|---|---|---|
| 核心模块数 | 10 (含死代码) | 8 (全部活跃) | -20% |
| 总代码行数 | 3245 | ~2000 | -38% |
| 死代码占比 | ~30% | <2% | -93% |
| 依赖数 | 15 | ~10 | -33% |
| 冗余命令系统 | 2套并行 | 1套统一 | -50% |

### 核心结论

> LingShu CIL v2 方向正确 (Coding Agent 而非 Chatbot), 但有三座大山需要清除:
> 1. **God Module**: cil.rs (482行) 拆为 agent.rs + cmds.rs
> 2. **功能蔓延**: mcp.rs (1189行) 精简 90%
> 3. **死代码**: markdown.rs (307行) 删除
> 
> 清除后用 8 个模块实现 Plan→Act→Observe 完整循环, 比 Claude Code 轻量但功能等价。
