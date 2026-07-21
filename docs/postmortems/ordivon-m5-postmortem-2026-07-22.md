# Ordivon M5 全面工程复盘

- 复盘范围：M5.0 Harness Stabilization、M5.1 Limited Dogfood、M5.2 Shadow Comparison
- 分支：`agent/m5-limited-dogfood`
- 阶段基线：`97b609e65d024c881f85521b8812aaa634175758`
- 测量实现：`ff57d258c0697dba1335bf8a86ccb32ca85e9128`
- 最终提交：`568ad977f42b6d418b06aa693e7e780d46196969`
- Receipt：`receipts/governance/ordivon-m5-limited-dogfood-2026-07-22.json`

证据标签：

- **Observed**：执行轨迹、测试或正式证据直接证明。
- **Inferred**：由多个事实合理推导，尚未被独立实验完全分解。
- **Proposed**：下一阶段建议。
- **Unknown**：当前证据不足。

证据层级：正式配对 Benchmark > 真实集成 / Dogfood / Smoke > 单元测试 > 静态审查 > 架构推断。

## 1. 任务概况与目标回顾

M5 旨在把 M4 已证明可行的实验 MCP Adapter，从“协议和微基准成立”推进到“能够承担有边界的连续工程任务”。

M5.0 负责固定 SDK 身份、统一实验 Server 生命周期、消除 cwd 隐式依赖、验证失败清理、冻结 wire response 预算并增加仓库与精确 revision 准入。

M5.1 负责通过真实本地 MCP 覆盖读取、修改、测试、失败修复、真实 Rust 测试、大日志 Artifact、断连恢复和取消，同时禁止生产工作树、push、merge、部署、凭据和自动 Legacy fallback。

M5.2 负责对安全可双跑的旅程进行 Legacy 与 Ordivon Shadow Comparison，并比较完成率、语义结果、延迟、调用数、Context、HTTP 请求、修复轮次和 fallback。

**Observed：** M5.0、M5.1、M5.2 的本地阶段门槛全部通过，形成四个独立提交，最终无运行残留。

**Observed：** 七条 Dogfood 旅程全部成功，十二组 Shadow 配对全部语义等价。

**Observed：** 所有旅程由确定性 Harness 驱动，尚未运行真实自主模型规划循环。

总体判断：M5 阶段合同完成度约 93%；Limited Dogfood 成立，但真实 model-in-the-loop Dogfood 尚未成立。

## 2. 执行过程复盘

### 2.1 关键流程

M5.0 建立了 `SDK lock → build → transient systemd unit → health check → child execution → stop → worktree / Task / store / manifest cleanup → residual scan` 的统一实验栈。

**Observed：** `stack.py exec -- /usr/bin/false` 保留非零退出状态，同时完成资源清理。

M5 随后在 M4 Adapter 上增加可选的 `M5DogfoodPolicy`，将 `workspace.open` 限制到 canonical repository path 和 exact source revision。

**Observed：** 越界仓库与 revision 分别返回 `SOURCE_REPO_NOT_ALLOWED` 和 `SOURCE_REVISION_NOT_ALLOWED`。

Wire Contract 验证了七种工具响应预算、无重复成功 JSON、无默认 Trace 泄漏，以及 64 个并发读取下 Core、HTTP 和客户端 Trace ID 唯一性。

M5.1 执行七条真实本地 MCP 旅程；M5.2 对四类旅程各运行三组交替顺序配对；最后通过独立 checker、篡改负测、Receipt integrity、文档治理差分和残留扫描封存结论。

### 2.2 关键决策

**Observed：** 先完成 M5.0 再开始 Dogfood，避免将 SDK、cwd、Server 生命周期和清理噪声误判为 Runtime 失败。

**Observed：** Agent 工具面仍保持八工具薄腰；M5 只增加古典准入层，没有复制 Runner 或 Task 真值。

**Observed：** 修复操作撞上 `REVISION_MISMATCH` 后，修改的是 Harness 流程，而不是放宽 optimistic concurrency。

**Observed：** Shadow 总体门槛通过，但逐旅程退化仍被写入正式文档，未被聚合数字掩盖。

### 2.3 主要挑战与失败

1. **可选字段计量崩溃。** 成功任务省略空 `stderrTail`，Harness 将 `undefined` 传给 `Buffer.byteLength`。证据等级：真实 Dogfood Smoke。修复为 `undefined / null → 0 bytes`。
2. **修复流程违反 digest guard。** Harness 在未刷新文件身份时尝试覆盖已有文件。执行器正确返回 `REVISION_MISMATCH`。修复流程改为重新读取 digest 后 `REPLACE_EXACT`。
3. **Python stale bytecode。** 同大小源码在短时间内修改后可能复用旧 `.pyc`。证据等级：真实 Dogfood Smoke。修复为快速修复验证时设置 `PYTHONDONTWRITEBYTECODE=1`。
4. **空状态目录残留。** unit、Task、worktree 和 store 已删除，但空 state root 尚存。证据等级：最终残留扫描。修复 `stop_stack` 并重跑失败清理负测。
5. **真实模型 Dogfood 未实现。** 所有任务由脚本确定操作顺序。该问题未在 M5 解决。

### 2.4 资源和方法效率

高效部分包括统一 stack controller、精确 SHA 绑定、从 raw samples 独立重算、篡改拒绝，以及将每个实质失败转化为代码或测试。

低效部分是 Harness 规模快速增长：三个主要 Harness 文件约 177、506 和 608 行；Legacy 与 Ordivon 的 Shadow 业务逻辑存在重复；运行仍依赖单机 WSL、systemd、现有 8811 endpoint 和本地 npm cache。

**Inferred：** Harness 已接近需要 Journey DSL 与 Backend Adapter 抽象的阈值。

## 3. 成果与指标分析

### 3.1 M5.0 Wire 与并发

| 指标 | 结果 | 证据类型 |
|---|---:|---|
| 并发读取 | 64 | Observed |
| Core Trace | 71 | Observed |
| HTTP Trace | 77 | Observed |
| 客户端 Trace ID | 76 | Observed |
| 成功响应范围 | 94–299 B | Observed |
| Trace ID 重复 | 0 | Observed |
| 成功结果重复 JSON | 0 | Observed |
| 默认结果 Trace 泄漏 | 0 | Observed |

### 3.2 M5.1 Dogfood

七条旅程共 33 次工具调用、15,090 B Context、4,335 B 输出消费、1 次修复轮次、0 fallback；断连恢复和取消清理均通过。

**Observed：** 单文件 guarded edit、多文件测试、失败修复、真实 Rust 目标测试、日志截断、Artifact 读取、断连恢复和 Task 取消均有真实集成证据。

### 3.3 M5.2 Shadow 总体

| 指标 | Legacy | Ordivon M5 |
|---|---:|---:|
| 样本数 | 12 | 12 |
| 完成率 | 100% | 100% |
| 中位耗时 | 603 ms | 477 ms |
| 中位调用数 | 5 | 5 |
| 中位 Context | 1,722 B | 1,721 B |
| 中位 HTTP 请求 | 5 | 5 |
| 中位修复轮次 | 0 | 0 |
| fallback | 0 | 0 |

**Observed：** 全部配对 semantic digest 一致，总体 Limited Dogfood gate 通过。

### 3.4 逐旅程差异

| 旅程 | Legacy 耗时 | Ordivon 耗时 | Legacy Context | Ordivon Context |
|---|---:|---:|---:|---:|
| 只读审计 | 465 ms | 536 ms | 1,738 B | 1,280 B |
| 单文件修改 | 539 ms | 344 ms | 3,333 B | 3,376 B |
| 多文件测试 | 665 ms | 402 ms | 776 B | 976 B |
| 失败修复 | 711 ms | 578 ms | 1,706 B | 2,162 B |

**Observed：** Ordivon 并未在所有任务类型和所有指标上统一领先。

**Inferred：** 极短只读任务中，隔离 workspace 和 Durable Task 的固定成本占比过高。

**Unknown：** 只读延迟中 worktree、systemd、MCP 和 Git 分别贡献多少，尚无消融证据。

### 3.5 与初始目标直接对比

固定 SDK、消除 cwd 依赖、统一成功/失败清理、wire 预算、Trace 并发唯一性、仓库准入、七类 Dogfood、Shadow Comparison 和 no-production-cutover 均已达成。

真实模型连续 Dogfood未达成；事务 Registry、生产权限和非 root Worker属于明确后续结构阶段，而非 M5 范围。

## 4. 对比分析

### 4.1 与初始计划

M5 基本遵循 `M5.0 稳定化 → M5.1 Dogfood → M5.2 Shadow`。主要偏离是 M5.0 工作量显著高于预期，并最终形成正式测试基础设施；该偏离来自 M4 期间真实暴露的 SDK、cwd、Server 生命周期和清理问题，结果为正向。

**Observed：** Dogfood 仍是 scripted journeys，而非真实模型动态规划，因此 M5 只验证执行 substrate 和协议旅程。

### 4.2 与替代方案

Desktop Commander 工具覆盖广、短只读固定成本较低，但执行状态、恢复、Artifact 和 workspace identity 更难形成统一真值。正式结果显示 Legacy 在短只读旅程更快，而 Ordivon 在修改、多文件测试和修复旅程更快。

单一通用 Shell 工具表达能力强，但难以稳定表达 digest guard、Durable Task、Artifact 和终态。Ordivon 的结构化薄腰更适合长期 Agent 工程，但实现成本更高。

Temporal、Nomad 或 Kubernetes 已具备事务、Job/Attempt、retry、调度和 reconciliation；Ordivon 更轻且更贴近本地 Agent 工作区，但事务与并发成熟度明显较低。

### 4.3 与成熟实践

已符合：feature gating、精确版本与 binary identity、ephemeral 环境、失败清理、Shadow、交替顺序、语义比较、独立 checker、篡改负测和明确 no-production-cutover。

尚未达到：事务 Registry、idempotency reservation、专用非特权 Worker、startup reconciler、retention/GC、WSL reboot、真实模型评测、正式 CI 和可移植环境封装。

总体上，M5 在恢复模型、复杂任务延迟、证据可信度和生产隔离纪律方面更优；在 Harness 复杂度、短只读延迟、部分 Context、真实 Agent 覆盖和生产控制面方面更差。

## 5. 成功因素与关键教训

### 5.1 可复制的成功模式

1. **复盘问题转化为工程门槛。** M4 的 SDK、cwd、生命周期和清理问题分别形成 lock、路径推导、stack controller 和失败清理测试。**Observed。**
2. **不为 Dogfood 放宽一致性保护。** digest guard 拒绝修复后，调整任务流程而非削弱合同。**Observed。**
3. **总体结论保留逐旅程退化。** 正式文档保留只读延迟和复杂任务 Context 退化。**Observed。**
4. **证据可以拒绝自身篡改。** 修改 Shadow 中位数后 checker 退出 1。**Observed。**

### 5.2 Start / Stop / Continue

**Start：** 真实 model-in-the-loop Dogfood；逐旅程路由门槛；Journey DSL；只读成本消融；并发、崩溃和 reboot 矩阵。

**Stop：** 将 scripted harness 称为完整 Agent Dogfood；只凭总体中位数决定全部路由；继续复制大型双后端 Journey；把文档中的 forbidden capability 视为机器权限保证；在文件 Registry 上继续堆积并发补丁。

**Continue：** 精确 SHA 绑定；Shadow 与语义 digest；独立重算与篡改负测；生产隔离；严格区分阶段通过和生产成熟。

### 5.3 根因

**Inferred：** 测试 Harness 尚未被当作独立架构层治理，导致环境假设和业务逻辑重复。

**Inferred：** 统一执行路径在短只读任务上产生不成比例的固定成本。

**Inferred：** Observation 已结构化，但尚未实现任务类型感知的最小投影，因此多文件和修复旅程 Context 偏高。

## 6. 局限性、风险与改进空间

### 6.1 主要问题

| 问题 | 证据类型 | 主要影响 |
|---|---|---|
| Dogfood 不含真实自主模型 | Observed | 无法评估规划、工具选择和重规划 |
| Forbidden capability 未全部机器强制 | Observed / Inferred | 扩大 Dogfood 后可能权限漂移 |
| 文件 Task Registry 非事务化 | Observed | 并发、幂等和 crash window 风险 |
| Worker 不是专用非 root | Observed | 纵深防御不足 |
| 短只读路径固定成本高 | Observed / Inferred | 简单任务路由退化 |
| 部分复杂任务 Context 较高 | Observed | 模型成本和注意力负担 |
| Harness 文件大且重复 | Observed | 维护成本和评测漂移 |
| 未覆盖 WSL reboot 和并发矩阵 | Observed | 无法外推恢复和并发稳定性 |
| 本地环境不可完全移植 | Observed | 跨机器绝对性能复现有限 |
| 正式 Dogfood 未进入 CI | Observed | 回归主要依赖手工运行 |

### 6.2 改进方向

**Proposed：** 冻结任务分类和逐旅程门槛；建立资源 ledger；默认测试失败清理；功能提交与证据提交继续分离。

**Proposed：** 增加真实模型闭环、2/4/8 并发、Server/Runner SIGKILL、WSL reboot、重复提交、Registry 锁和部分写入测试。

**Proposed：** 抽象 Journey 定义与 Backend Adapter；固定仓库受控 SDK 依赖；建立 release/debug 双基准；版本化 wire snapshots。

**Proposed：** 路由至少区分短只读、workspace mutation、测试执行、长任务和修复循环，不再使用一个全局“Ordivon 更快”结论。

## 7. 可复现性、文档与工程成熟度

### 7.1 可复现性

评分：8.4 / 10。

优势包括实现和最终提交 SHA 固定、SDK 版本和 package digest 固定、Server/Runner identity 记录、一条命令启动和清理、原始样本保存、独立 checker 和篡改拒绝。

不足包括依赖当前 WSL、systemd 和 8811 Legacy Endpoint；SDK 文件仍来自本机 npm cache；没有容器或 Nix 环境；未固定 CPU、I/O 和系统负载；性能证据只来自单机。

**Observed：** 过程具有较高本机可复现性。

**Unknown：** 跨机器绝对延迟及相同比例优势是否保持。

### 7.2 文档完整性

已有架构文档、Wire/Dogfood/Shadow evidence、checker、Receipt、Registry、Harness 说明、claims-not-made 和逐旅程退化说明。

仍缺真实 Agent Dogfood 方法学、Capability Policy 独立合同、Journey DSL、M5 Runbook、故障注入矩阵、评测环境完整 manifest、资源生命周期状态机、逐任务 Route Policy 和证据保留策略。

### 7.3 工程成熟度

较好实践包括 feature gating、精确版本控制、分阶段提交、测试层级、失败路径验证、结构化错误、optimistic concurrency、自动清理、独立证据重算和 no-production-cutover。

不足包括 Harness 模块过大、Shadow 逻辑重复、权限声明与执行策略未完全统一、文件 Registry 无事务、无专用 Worker、无正式 CI、无真实模型层。

不得将测试全绿解释为生产成熟：负载有限，事务、身份、并发、reboot、远程环境和模型规划均未完成。

## 8. 整体评估

综合评分：**8.6 / 10**。

加分项：M4 复盘问题被实际修复；标准实验栈提高证据可信度；Dogfood 覆盖真实失败修复和 Rust 测试；Shadow 设计合理；局部退化未被掩盖；证据可独立重算；生产边界保持完整。

扣分项：Dogfood 不是真实模型驱动；Capability Policy 机器约束不完整；Harness 重复和体积较大；只读延迟与部分 Context 退化；文件 Registry 和 Worker 身份仍不成熟；环境不可完全移植。

M4 证明真实 Ordivon MCP 在有界微旅程中成立。M5 进一步证明它能够承担连续修改、测试、失败修复、真实仓库测试、日志 Artifact、恢复和取消。

M5 的关键项目价值是把阶段结论从功能存在和单次 Smoke，升级为可重复运行、Shadow 样本、逐任务指标和独立证据重算。

## 9. 后续行动与知识沉淀

### 9.1 债务分类

**本阶段应解决但未解决：** 真实 model-in-the-loop Dogfood、完整机器强制 Capability Policy、逐旅程路由策略、Harness 去重、三项历史文档治理违规。

**本阶段新发现、应进入下一阶段：** 构建缓存验证歧义、短只读固定成本、修复 Observation Context、资源生命周期正式建模、scripted journey 与 Agent evaluation 分层。

**长期结构性债务：** 事务 Job/Attempt Registry、idempotency、concurrency reservation、startup reconciler、非 root Worker、retention/GC、reboot、网络和凭据授权、远程身份、高风险 Authority、多节点。

**明确非目标：** Cloudflare、OAuth、Git push/merge、生产 8811 切流、外部部署、Tool Foundry、Kubernetes、多节点和金融执行。

### 9.2 问题优先级

| 问题 | 严重度 | 概率 | 成本 | 建议阶段 |
|---|---|---:|---:|---|
| 文件 Registry 非事务化 | High | 高 | High | M6 P0 |
| 缺少 idempotency / reservation | High | 高 | High | M6 P0 |
| Capability Policy 未完全机器强制 | High | 中高 | Medium | M6 P0 |
| 无真实模型 Dogfood | High | 高 | Medium | M6 / M6.1 P0 |
| 缺少 startup reconciler | High | 中高 | High | M6 P0 |
| 未覆盖并发 Job | High | 中高 | Medium | M6 P1 |
| 未覆盖 WSL reboot | High | 中 | Medium | M6 P1 |
| Worker 非专用非 root | High | 中 | High | M7 P0 |
| 短只读任务延迟退化 | Medium | 高 | Medium | M6.1 P1 |
| 修复旅程 Context 较高 | Medium | 高 | Medium | M6.1 P1 |
| Harness 重复代码 | Medium | 高 | Medium | M6.0 P1 |
| 构建缓存验证歧义 | Medium | 中 | Low | M6.0 P1 |
| 环境不可跨机器完全复现 | Medium | 中 | High | M7 P2 |
| Dogfood 未进入 CI | Medium | 高 | Medium | M6 P1 |
| 历史文档治理违规 | Low | 高 | Low/Medium | 独立治理轮次 |
| 生产切流 | Critical，但未启用 | 低 | High | 禁止进入 M6 |

### 9.3 决策与行动

总体决定：**继续，但进入结构性控制面建设。**

**Observed：** M5 的任务完成率、恢复、取消和 Shadow 门槛均通过。

**Inferred：** 继续在文件 Task store 上扩展 Dogfood 会提高竞态和治理复杂度。

**Proposed：** 下一阶段进入 M6 Transactional Job and Attempt Control Plane。

M6 P0：

1. **事务 Job/Attempt Registry**：SQLite 事务、唯一 idempotency key、并发 reservation、状态约束、重复请求与 crash window 测试。
2. **Startup Reconciler**：比对 Registry、systemd unit、runner result、boot ID；明确恢复为 running、failed、cancelled、lost 或 orphaned；禁止静默重跑。
3. **机器强制 Capability Route Policy**：绑定 executable/profile、repo/revision、network、credential 和外部副作用；拒绝必须可审计。
4. **真实 model-in-the-loop Dogfood**：模型只获得目标和工具 Schema，记录工具选择、失败解释、重规划和停止条件，并与 scripted harness 分开报告。

M6 P1：Journey DSL 与 Backend Adapter；2/4/8 并发与故障矩阵；逐旅程 Route Budget；轻量 Dogfood CI。

不应顺便进入 M6：Cloudflare、OAuth、远程 MCP、push/merge、生产切流、非本地凭据、部署、Tool Foundry、Kubernetes、多节点和金融执行。

应固化的长期规则：

1. SDK、Runner、Server 和工具链必须记录版本、绝对路径和 digest。
2. Agent 修复必须执行“失败观察 → 重新读取 → 获取当前 identity → 精确修改 → 验证”。
3. 总体 Gate 必须附带逐 Journey Gate 和明确路由范围。
4. Scripted journey 证明 Runtime 可执行；model-in-the-loop 才证明 Agent 会使用 Runtime。
5. 每个 unit、Task、workspace、store、token 和临时文件必须拥有 owner、runId 和 cleanup 路径。
6. 当任务撞上安全或一致性合同，优先修正任务流程，放宽合同必须独立评审。

## 10. 一句话总结

M5 证明 Ordivon 已能在有边界的本地工程旅程中稳定完成修改、测试、修复、恢复和取消，但当前证据主要证明执行平台而非自主 Agent；M6 应停止横向扩展任务数量，转向事务 Job/Attempt 真值、机器权限策略、恢复控制器和真实模型 Dogfood。
