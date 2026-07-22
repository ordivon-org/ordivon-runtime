# Ordivon M0–M7 本地执行迁移阶段总复盘

- 复盘对象：`ORDIVON-MIGRATION-M0-M7-2026-07-22`
- 起点：Desktop Commander 任务路径与云端 ChatGPT 到本地 WSL 的执行断层
- 终点：`279ad80d3c2b6aee325fbd892ed96f9a451e69a7`
- 最终被测 Runtime：`c62f4f0a90a0c1866cc336fc819f2e860d457d41`
- 阶段决定：`M0_M7_SERIES_CLOSED_BOUNDED_LOCAL_EXECUTION_NO_PRODUCTION_CUTOVER`

证据标签：

- **Observed**：正式证据、配对 Benchmark、真实 systemd/WSL 集成或执行轨迹直接证明。
- **Inferred**：由多项事实合理推导，但未由单项实验完全隔离。
- **Proposed**：阶段关闭后的建议或重新启动条件。
- **Unknown**：当前证据不足。

证据层级：正式配对 Benchmark > 真实 reboot/systemd/Dogfood > 单元测试 > 静态审查 > 架构推断。

## 1. 任务概况与原始目标

M 系列最初不是为了制造一个新的 Coding Agent，也不是为了与 Pi、Claude Code、Codex、OpenCode 或 OpenHands 竞争认知层。

原始问题是：

> 云端 ChatGPT 如何在用户拥有的 WSL 中执行一个完整工程任务，并使任务生命周期、输出和恢复不依赖某次 MCP Session、临时 launcher 或 Desktop Commander 的内存状态？

最初需要证明的链路是：

```text
ChatGPT / MCP Client
→ isolated workspace
→ structured mutation
→ durable local execution
→ bounded observation
→ caller disconnect
→ later recovery
→ cgroup cancellation
→ semantic comparison with Legacy
```

成功标准最初只有四类：

1. **Observed**：任务可以脱离发起进程持续运行并被后续调用恢复。
2. **Observed**：所有修改位于隔离 Workspace，不写生产工作树。
3. **Observed**：新路径至少在可靠性或效率上优于 Legacy Desktop Commander。
4. **Observed**：生产 8811、Cloudflare、凭据、网络和外部副作用不因实验被隐式扩大。

一句话结果：

> **Observed：M0–M7 已在单机 WSL、本地实验边界内建立持久、紧凑、真实 MCP、事务化、非 root payload、可重启恢复且具生命周期治理的执行控制面；生产切流与远程能力没有被授权。**

整体目标达成度：**约 90%**。剩余部分主要属于生产化与新的产品问题，而不是原始迁移证明的遗漏。

## 2. 执行过程与阶段逻辑

### M0：先建立迁移证明规则

**Observed**：M0 冻结 Task、Artifact、Backend Route、Performance Sample 和 Legacy Fallback Record，要求按完整任务旅程比较，而不是按工具数量或“连接成功”判断能力。

关键规则：

- Legacy fallback 必须显式；
- task-control、生产、凭据、网络和外部副作用不得自动 fallback；
- 新后端必须用完成率、调用、Context、延迟、恢复和重复执行证据证明价值。

M0 避免了“先切换、后解释”的迁移方式。

### M1：任务生命周期脱离调用进程

**Observed**：M1 建立 detached Git worktree、文件式 Task Registry、systemd/cgroup Runner、stdout/stderr/result Artifact、跨进程 `task-get` 和 cgroup cancellation。

关键架构变化：

```text
MCP Session / CLI process ≠ Task owner
systemd + persisted evidence = durable mechanical substrate
```

M1 有意使用文件 Registry，仅用于证明最小垂直链路，不宣称生产控制面成立。

### M2：第一次差分评测暴露 Agent 接口退化

**Observed**：相同任务中，M1 中位耗时 723 ms，Legacy 为 973 ms；M1 可断连恢复，但调用从 7 增至 10，Context 从 6,441 B 增至 9,355 B。

结论不是“新系统获胜”，而是：

> 机械执行层更可靠，但模型接口更繁琐，尚不具备默认路由资格。

### M3：把复杂性留在控制面，不推给模型

**Observed**：M3 引入 `task-run`、`task-await`、批量 mutation、紧凑 read/diff 和 bounded output tail。

结果：

| 旅程 | Legacy | Ordivon M3 | 结论 |
|---|---:|---:|---|
| Full-read latency | 926 ms | 707 ms | M3 更快 |
| Full-read calls | 7 | 6 | M3 更少 |
| Full-read Context | 6,447 B | 6,205 B | 无回归 |
| Targeted latency | 1,015 ms | 694 ms | M3 更快 |
| Targeted calls | 5 | 5 | 持平 |
| Targeted Context | 1,491 B | 1,409 B | M3 更少 |

**Inferred**：M3 是 M 系列中最重要的接口原则之一——内部持久性可以复杂，但 Agent-facing thin waist 必须保持紧凑。

### M4：消除 CLI 对 MCP 的比较不对称

**Observed**：M4 建立独立 localhost Streamable HTTP MCP Adapter，冻结八工具薄腰、Bearer、Host、Origin、body limit、Native MCP Tasks 和无 Session 真值依赖。

```text
workspace.open / read / mutate / diff
workspace.exec
task.observe / cancel
artifact.read
```

M4 证明 Ordivon 可以作为真实 MCP 后端，而不是只存在于本地 CLI Harness 中。

### M5：从 Benchmark 进入有限真实 Dogfood

**Observed**：M5 执行 7 条真实 MCP 旅程，包括审计、修改、多文件测试、失败修复、Rust 测试、大日志、断连恢复和取消；33 次调用、1 次修复、0 fallback。

正式 Shadow：

| 指标 | Legacy | M5 |
|---|---:|---:|
| 中位延迟 | 603 ms | 477 ms |
| 调用 | 5 | 5 |
| Context | 1,722 B | 1,721 B |

Dogfood 同时暴露文件式 Task 模型的结构极限：客户端 Task ID、非事务容量、无 Job/Attempt 分离、无完整 reconciliation、root payload 和无生命周期治理。

### M6：事务 Job/Attempt 控制面

**Observed**：M6 将文件 Task 升级为 SQLite Job/Attempt Registry：

```text
Job      = 稳定意图与幂等边界
Attempt  = 一次具体 dispatch
Runner   = systemd/cgroup 进程树
Artifact = Attempt 产生的证据
MCP Task = Job 的协议投影
```

主要合同：

- server-generated UUIDv7；
- `(principal, clientRequestId)` 原子幂等；
- global/profile reservation；
- immutable bundle 与 persisted dispatch intent；
- ambiguous dispatch 禁止自动重发；
- terminal、Artifact、reservation 和 event 同事务；
- startup/targeted reconciliation；
- orphaned 持续占用容量。

**Observed**：2/4/8 并发、8 条真实 systemd crash matrix、Registry p95、8 条 Dogfood、1 条 model-in-the-loop 和独立 tamper checker 通过。

M5→M6 的事务成本：509 ms → 603 ms，Context 1,687 B → 1,754 B，处于冻结的 25% 预算内。

### M7：不可信 payload、完整重启与长期状态

**Observed**：M7 建立：

- 静态非登录 `ordivon-worker`；
- root-owned trusted Runner supervisor；
- payload 永久降权；
- per-Attempt mount namespace；
- root-owned bundle/result 与 worker-owned workspace/output；
- quota、hold、two-phase GC、backup/restore；
- evidence-digest-bound orphan remediation；
- Windows 编排的完整 `wsl.exe --shutdown` recovery matrix。

正式 reboot 结果：

| 场景 | Attempt | Reservation |
|---|---|---|
| running at reboot | lost | released |
| cancel pending | lost | released |
| result pending commit | succeeded | released |
| dispatch intent unbound | lost | released |
| held orphaned | orphaned | held_orphaned |

Attempt `5 → 5`，无自动 redispatch。

20 对 M6/M7 Shadow：635 ms → 689 ms，调用相同，Context 1,812 B → 1,808 B，硬化成本约 8.5%。

## 3. 为什么范围逐步扩大

### 3.1 自然扩展

大部分扩展不是“看到功能就加入”，而是前一阶段证据暴露新的证明义务：

```text
能执行
→ 调用方断开后能否恢复
→ 模型成本是否合理
→ 真实 MCP 是否成立
→ 多步 Dogfood 是否成立
→ 并发与崩溃下谁是真值
→ 不可信代码能否伪造终态
→ 主机重启与状态增长如何治理
```

**Inferred**：M0–M7 仍围绕同一个核心问题——“一个 Agent 动作如何在用户拥有的主机上可靠地成为真实执行”。

### 3.2 战略泛化开始出现的位置

后续对 Pi、OpenCode、Claude Code、Codex、Cline 和 OpenHands 的调研推导出：Ordivon 可能成为多 Harness 共用的执行治理层。

这一路线具有逻辑连续性，但必须明确：

- **Observed**：它不是 M0 的原始范围；
- **Inferred**：它是 M1–M7 结果产生的产品可能性；
- **Proposed**：是否进入该方向必须重新立项，不能自动命名为 M8。

## 4. 关键成功因素

### 4.1 证据先于路由

**Observed**：每一阶段都先保留 Legacy/旧后端，只有配对语义和门槛通过后才进入下一种本地资格；从未因“功能存在”修改生产 8811。

可复制规则：

```text
Connection ≠ capability
Capability ≠ eligibility
Eligibility ≠ production authorization
```

### 4.2 失败被固化为合同

**Observed**：

- M2 调用/Context 退化推动 M3；
- 快速失败误判 lost 推动 Reconciler 二次读取；
- `/tmp` 与 PrivateTmp 冲突推动私有路径拒绝；
- PID namespace 身份差异推动 Core/Runner 双身份证明；
- reused unit 推动 InvocationID 优先分类；
- distro-only terminate 不改变 kernel boot ID，推动 full WSL shutdown。

### 4.3 不为测试降低隔离

**Observed**：没有关闭 PrivateTmp、PrivatePIDs、ProcSubset 或 cgroup 隔离来换取测试通过；而是重新划分 Core、Supervisor 和 Payload 责任。

### 4.4 性能门槛没有被事后放宽

**Observed**：M6 初始 Shadow 超预算后，通过减少 WAL PRAGMA、连接 churn、重复 projection 和写后重读降至预算内；M7 也用正式 Shadow证明硬化成本。

### 4.5 古典控制面与 Agentive 面分离

**Observed**：模型可生成程序、修复错误和选择工具，但 Job identity、Authority、Policy、Reservation、Reconciliation、GC 和 Receipt 不由模型自行决定。

## 5. 主要失败、摩擦与根因

### 5.1 阶段体量过大

**Observed**：M6 和 M7 都横跨 Runtime、MCP、Harness、真实 OS 测试、性能和治理证据，导致执行链很长，易受工具中断影响。

根因：阶段定义围绕“关闭一个风险域”，但交付包同时承担实现、评测和治理全部工作。

改进规则：未来若重新立项，每个阶段只允许一个主要风险主题和一个独立证明提交。

### 5.2 Evidence Envelope 建立过晚

**Observed**：M6/M7 后期曾补充 implementation SHA、harness digest、claims-not-made 和 raw sample 重算。

根因：最初把 Evidence 当作输出文件，而不是受版本治理的第一类 Artifact。

### 5.3 工具和桥接中断增加了恢复成本

**Observed**：完整 WSL shutdown 后 Ordivon WSL Operator 曾返回 502；长链执行也出现工具会话回收。

**Inferred**：执行 Harness 自身也需要成为被治理基础设施，而不是假设永远在线。

### 5.4 宿主语义不可仅靠文档推断

**Observed**：PrivateTmp、ProcSubset、PID namespace、systemd InvocationID 和 WSL kernel/userspace epoch 均需要真实探针才能确定。

规则：涉及 OS identity、namespace、reboot 和 process ownership 的结论必须来自真实系统证据。

### 5.5 复杂度增长具有自我强化风险

**Inferred**：每增加一类治理对象，就会产生 schema、migration、runbook、checker、GC 和兼容性成本。即使每项都合理，组合后也可能超过实际用户价值。

因此，M 系列必须在 M7 结束，而不能因架构仍可继续扩展就自动进入下一阶段。

## 6. 最终成果与指标

### 6.1 能力闭环

**Observed**：M0–M7 现已证明：

- exact-revision isolated workspace；
- digest-guarded structured mutation；
- durable systemd/cgroup execution；
- caller-disconnect recovery；
- bounded Artifact retrieval；
- real Streamable HTTP MCP；
- limited model-visible Dogfood；
- transactional Job/Attempt truth；
- atomic idempotency and capacity reservation；
- at-most-once ambiguity handling；
- non-root payload and trusted evidence split；
- lifecycle quota/GC/backup/restore；
- orphan remediation；
- full WSL kernel-restart reconciliation。

### 6.2 最终 M7 指标

| 类别 | 结果 |
|---|---|
| Local matrix | 10/10 commands passed |
| M6 real recovery | 8/8 journeys |
| M7 hardening/remediation | 4/4 journeys |
| Lifecycle contracts | 4/4 |
| M7 MCP Wire | 64 concurrent reads, trace identities unique |
| M7 Dogfood | 8 journeys, 40 calls, 1 repair, 0 fallback |
| M6 compatibility | Wire/transport/Dogfood/2-4-8 concurrency passed |
| M6→M7 Shadow | 20 pairs, semantic equivalence |
| Full WSL reboot | 5 scenarios, no redispatch |
| Evidence checker | `M7_EVIDENCE_PASS` |
| Tamper negative | modified summary rejected, exit 1 |
| Runtime residue | none after closeout |

### 6.3 不能由这些指标推出的结论

- **Unknown**：长期运行数周后的稳定性；
- **Unknown**：高并发持续吞吐；
- **Unknown**：远程攻击环境；
- **Unknown**：凭据和网络委托；
- **Unknown**：多主机一致性；
- **Unknown**：广泛 Coding Agent 自主质量；
- **Unknown**：生产用户体验和运维成本。

## 7. 与替代方案的对比

### 7.1 继续使用 Desktop Commander

优势：简单、现成、直接 Host 权限。

缺点：Session/进程状态恢复弱，缺少 Job/Attempt、幂等、reservation、进程身份、终态交易和生命周期治理。

结论：Legacy 仍可作为当前生产 WSL Operator，但不再是成熟执行控制面的设计基线。

### 7.2 直接采用现有 Coding Harness

Pi、Codex、Claude Code、OpenCode、Cline 等在认知、上下文、编辑体验和人机交互上更成熟。

但它们通常不完整解决本系列重点：

```text
principal/authority
atomic idempotency
Job/Attempt truth
ambiguous dispatch
host process identity
orphan capacity
lifecycle evidence
```

结论：Ordivon 不应复制其认知层；未来若有明确需求，应作为其执行治理后端，而不是第九个 Harness。

### 7.3 直接采用 Kubernetes、Temporal、Nomad 或远程 Agent 平台

优势：成熟调度、恢复、远程 Workspace 和集群能力。

缺点：对于当前单用户、单 WSL、本地控制面，部署、攻击面和运维成本显著过高。

结论：M6/M7 吸收 Job/Attempt、reconciliation 和 durable intent 思想是合理的；复制完整分布式平台不是当前问题。

## 8. 工程成熟度评估

综合评分：**8.8 / 10**。

| 维度 | 评分 | 判断 |
|---|---:|---|
| 架构语义 | 9.3 | Job/Attempt、identity、evidence 和 lifecycle 边界清晰 |
| 真实系统验证 | 9.4 | systemd、cgroup、namespace、完整 WSL reboot 均有实证 |
| Agent-facing efficiency | 8.8 | M3 后维持薄接口，M7 Context 无增长 |
| 安全边界 | 8.9 | non-root payload、mount view、root-owned evidence；远程身份仍缺失 |
| 可运维性 | 8.3 | quota/GC/backup/remediation 已有，但长期 soak 与灾备不足 |
| 范围控制 | 7.8 | 未触碰生产和凭据，但 M6/M7 规模仍偏大 |
| 可复现性 | 9.2 | exact SHA、Harness digest、raw samples、checker、tamper negative |
| 产品价值证明 | 7.6 | 本地执行价值已证，生产用户价值和替代成本尚未证 |

特别说明：测试通过不等于生产成熟；局部 Shadow 不得外推为所有工作负载；一次 reboot 不得外推为长期可靠性。

## 9. 债务分类与关闭决策

### 9.1 本阶段应解决且已关闭的问题

- M1 文件 Task 与 Session 生命周期耦合；
- M2 Agent-facing 调用与 Context 退化；
- CLI/MCP 比较不对称；
- 文件 Registry 无原子幂等和 reservation；
- Runner/payload root 身份混合；
- trusted result 可被同 UID payload 攻击的风险；
- 无真实 WSL kernel-restart recovery；
- 无 quota、GC、backup/restore；
- held-orphaned 无证据化 remediation；
- evidence 缺少统一 SHA、Harness digest 和 tamper rejection。

### 9.2 新发现但不自动进入下一阶段的问题

| 问题 | 严重度 | 概率 | 成本 | 决定 |
|---|---|---|---|---|
| repeated reboot / long soak | High | Medium | Medium | 候选独立可靠性项目，不自动继续 |
| toolchain/cache provenance | High | Medium | High | 启用更广构建生态前再立项 |
| remote principal authentication | Critical if remote | Low while local | High | 远程接入前独立 P0 |
| network/credential delegation | Critical if enabled | Low while disabled | High | 明确非目标 |
| multi-host Registry | Medium current / High future | Low | High | 当前非目标 |
| production routing and SLO | High | Unknown | High | 需要产品价值证明后再决定 |
| three historical doc violations | Low | High | Low/Medium | 独立文档线处理 |

### 9.3 项目长期结构性债务

- 单节点 SQLite；
- 一个静态 Worker 身份；
- 远程身份与组织 Policy 尚未建立；
- tool/skill 供应链治理尚未建立；
- 生产监控、SLO、容量规划和事故演练缺失；
- 真实用户旅程与维护成本尚未量化。

### 9.4 明确非目标

M 系列关闭后，不得“顺便”加入：

- M8；
- 多 Harness Adapter 矩阵；
- 自有 TUI/IDE；
- Memory/Skill 平台；
- Cloudflare 生产路由；
- OAuth/远程多用户；
- 凭据和任意网络；
- Git push/merge/deploy；
- live broker 或金融外部执行；
- Kubernetes/Temporal/Nomad 重实现；
- 自动重试 ambiguous dispatch。

### 9.5 总体决定

# **暂停新增能力，关闭 M0–M7 系列**

理由：

1. **Observed**：原始本地执行迁移问题已经形成完整可证明闭环。
2. **Observed**：生产切流从未被授权，继续扩张不会自动增加当前用户价值。
3. **Inferred**：M7 之后的问题属于远程产品化、供应链或规模化，而不是原始迁移的自然小步。
4. **Proposed**：进入维护和真实使用观察期；任何新工作必须重新定义问题，比较成熟替代方案，并设置停止条件。

### 9.6 重新启动工程线的准入标准

任何后续阶段必须同时满足：

- 有明确用户问题，而不是“技术上可以”；
- 说明为什么现有 Pi/Codex/Claude/OpenCode/OpenHands 或普通 systemd/Docker 方案不足；
- 冻结单一风险主题；
- 给出非目标；
- 给出可量化成功和停止标准；
- 不默认沿用 M8 名称；
- 不自动获得生产、凭据、网络或外部副作用权限。

### 9.7 可固化知识

1. `MCP Session != Task owner`。
2. `Harness permission != OS sandbox != transactional authority`。
3. ambiguous dispatch 永不自动重发。
4. 跨文件系统和 systemd 分类前必须重读易变证据。
5. namespace identity 与 host process identity 必须分别证明。
6. payload output 不是 trusted terminal evidence。
7. Session、Workspace、Job、Attempt 和 Receipt 必须分离。
8. Evidence 必须绑定被测 SHA、Harness digest、raw samples 和 claims-not-made。
9. 基准失败时先优化，不因实现失败放宽冻结门槛。
10. “可以扩展”不是“应该扩展”的充分条件。

## 10. 总结

> M0–M7 把一个“让 ChatGPT 更可靠地操作 WSL”的局部问题，逐层推进为持久、紧凑、事务化、非 root、可重启恢复且具生命周期证据的本地执行控制面。该证明链已经足够完整；下一步最成熟的行为不是继续制造 M8，而是停止扩张、观察真实使用，并在新问题出现时重新立项。
