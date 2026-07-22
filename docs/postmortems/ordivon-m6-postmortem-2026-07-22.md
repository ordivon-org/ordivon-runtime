# Ordivon M6 事务化 Job / Attempt 控制面工程复盘

- 复盘对象：`ORDIVON-MIGRATION-M6-2026-07-22`
- 最终分支：`agent/m6-transactional-job-attempt-control-plane`
- 最终 HEAD：`62367357e97ae717c7f487c52c3472b85e6edbef`
- 实测实现：`ffaca2f1989573d8d085d5cbeaa96b6a91726b35`
- 阶段状态：`LOCAL_TRANSACTIONAL_DOGFOOD_GATES_PASS_NO_PRODUCTION_CUTOVER`

证据标签：

- **Observed**：正式证据、真实集成、测试或执行轨迹直接证明。
- **Inferred**：由多项观测合理推导，但未被单独实验完全分解。
- **Proposed**：对后续阶段的建议。
- **Unknown**：当前证据不足。

证据层级：正式配对 Benchmark > 真实集成/Dogfood/Smoke > 单元测试 > 静态审查 > 架构推断。

## 1. 任务概况与目标回顾

M6 的目标不是增加更多执行工具，而是把 M1–M5 的文件式 Durable Task 升级为事务 Job/Attempt 控制面。
核心链路为：

```text
Job → Attempt → Reservation → Immutable Bundle → Dispatch Intent
→ systemd/cgroup Runner → Result/Artifact → Terminal Transaction
→ Reconciliation → MCP Task Projection
```

成功标准包括 SQLite 唯一真值、原子幂等、容量占位、保守的 at-most-once dispatch、真实恢复矩阵、MCP 投影、性能预算和证据封存。

**Observed：** M6.0–M6.6 全部完成，未修改生产 8811、Cloudflare、凭据、网络或外部副作用权限。

**总体结论：** M6 已建立本地事务执行真值；阶段合同完成度约 **92%**。未完成部分主要是非特权 Worker、真实 WSL reboot、生命周期治理和远程信任链。

## 2. 执行过程复盘

### 2.1 关键阶段

1. 统一早期 `JobRecord` 与 M1–M5 Universal Task 语义。
2. 建立八表 SQLite Registry 和事务约束。
3. 复用现有受限 Runner，接入 immutable bundle 与 dispatch intent。
4. 建立 lost/orphaned、取消、终态和 Artifact 事务。
5. 新建独立 M6 MCP Adapter，Job ID 直接作为 Native MCP Task ID。
6. 完成 Wire、Transport、Dogfood、并发、Shadow、模型旅程和 Registry p95 评测。
### 2.2 关键决策

- **Observed：** `MCP Task = Job 投影`，不再保留独立 projection 文件真值。
- **Observed：** ambiguous dispatch 无匹配 launch evidence 时标记 `lost`，禁止自动重发。
- **Observed：** 面对 `PrivateTmp`、`PrivatePIDs` 和 `ProcSubset` 问题，没有关闭 sandbox，而是重分 Core/Runner 证据职责。
- **Observed：** 初始 Shadow 延迟增幅 35.8% 未被接受，直到优化到 18.5%。
- **Observed：** scripted Dogfood 与 model-in-the-loop 证据分开报告。

### 2.3 主要挑战与处理

| 事件 | 证据类型 | 处理 |
|---|---|---|
| `rusqlite` 与既有 SQLx SQLite sys crate 冲突 | Observed / 编译 | 选择兼容的 `rusqlite 0.32.1`，不降级 SQLx |
| `/tmp` 在 `PrivateTmp` 下不可见 | Observed / 真实 systemd | Store 移至私有持久路径并拒绝 private-tmp 路径 |
| Runner 无权读取 boot ID | Observed / sandbox | 改由 Core 绑定宿主 boot identity |
| namespace PID 与 systemd MainPID 不同 | Observed / OS 实测 | Runner 证明 namespace identity，Core 证明 host identity |
| 极短失败偶发误判 `lost` | Observed / Shadow | 提交 lost 前二次读取 Runner evidence，增加 10 次竞态回归 |
| Shadow 性能超出预算 | Observed / 正式 Benchmark | 移除重复 WAL PRAGMA、连接 churn 和重复 snapshot 查询 |
| 部分证据缺失实现 SHA | Observed / 证据审查 | 统一 evidence envelope 并在最终 SHA 上重跑 |
| 工具链中断后一度报告旧 SHA | Observed / 执行轨迹 | 恢复后重新核对 HEAD、工作树、资源和证据 |
### 2.4 工具与方法效率

**Observed：** Registry、Runtime、MCP、性能和证据采用独立提交，便于定位回归；真实 systemd/cgroup 证据优先于 mock。

**Inferred：** 阶段范围仍偏大：相对设计基线变更 48 个文件、约新增 11,362 行。统一 evidence envelope 与 Registry 微基准若更早建立，可减少后期收口成本。

## 3. 成果与指标分析

### 3.1 核心产出

- 八表 SQLite Registry；
- Job/Attempt、幂等键、容量 Reservation、append-only events；
- immutable bundle、dispatch intent、terminal transaction；
- lost/orphaned Reconciler；
- 独立 `ordivon-m6-http`；
- 八类正式证据、独立 checker 和 Receipt。

### 3.2 测试和验证

| 层级 | 结果 |
|---|---:|
| 默认 Workspace | 95 通过 |
| M6 feature `ordivon-exec` | 95 通过 |
| M6 MCP | 9 通过 |
| 真实 systemd/cgroup 集成 | 8 通过 |
| Registry 对抗合同 | 12 通过 |
| 快速失败竞态 | 10/10 正确归类 |
| Evidence checker | `M6_EVIDENCE_PASS` |
| 篡改负测 | 正确拒绝，exit 1 |
### 3.3 运行指标

**Observed：** 64 个并发读取成功，73 条 Core Trace、87 条 HTTP Trace，Trace ID 冲突为 0；401/403/413 Transport 边界成立。

**Observed：** 8 条 Dogfood 为 8/8 通过：39 次工具调用、19,817 B Context、1 次修复、0 fallback，恢复、取消和幂等 replay 均成立。

**Observed：** 2/4/8 并发批次全部通过，14 个 Job/Attempt 身份唯一，最终 active reservation 为 0。

### 3.4 性能

| 指标 | M5 | M6 | 变化 |
|---|---:|---:|---:|
| 完成率 | 100% | 100% | 持平 |
| 中位耗时 | 509 ms | 603 ms | +18.5% |
| 工具调用 | 5 | 5 | 持平 |
| Context | 1,687 B | 1,754 B | +4.0% |
| HTTP 请求 | 5 | 5 | 持平 |
| Fallback | 0 | 0 | 持平 |

Registry-only p95：Admission 20.30 ms、Replay 0.97 ms、Status 0.86 ms、List-100 5.11 ms、Terminal 21.15 ms，均低于冻结预算。

### 3.5 与目标对比

**Observed：** SQLite Registry、原子幂等、并发容量、Runner 接入、终态交易、Reconciler、MCP bridge、Shadow 和模型旅程均达成。

**Unknown：** 当前证据不足以证明宿主重启后的完整恢复、高并发长期稳定性、跨机器一致性或生产远程安全。

## 4. 对比分析
### 4.1 与初始计划

**Observed：** 实施基本遵循 M6.0–M6.6。主要偏离是增加性能收口、model-in-the-loop、Registry 微基准和统一 SHA 证据绑定。

**Inferred：** 这些偏离提高了结论可信度，但说明性能模型和证据合同设计得过晚。

### 4.2 与 M5 文件后端

M5 更简单、短任务更快、人工可读性更强。M6 增加约 18.5% 延迟和约 4% Context，换取原子幂等、容量占位、Job/Attempt 分离、终态交易、Artifact lineage、cancel intent 与 lost/orphaned 分类。

**Inferred：** 对治理型执行系统，这一交换是合理的；对极短、低风险、无副作用工具调用则未必必要。

### 4.3 与常见替代方案

- 继续文件 store：实现简单，但无法可靠完成跨对象原子性与并发控制。
- 只把 metadata 搬进 SQLite：会保留意图、执行实例和协议 Task 的语义混淆。
- ambiguous dispatch 自动 retry：提高表面可用性，但可能重复副作用。
- 直接采用大型调度系统：恢复能力成熟，但对当前单 WSL 阶段过重。

### 4.4 与成熟实践

**Observed：** server-generated IDs、幂等键、Job/Attempt、terminal immutability、append-only events、immutable bundle、reconciliation 和精确证据绑定已接近成熟控制面实践。

**Observed：** 非特权 Worker、真实 reboot、retention/GC、backup/restore、operator remediation、远程身份和凭据治理仍缺失。

## 5. 成功因素与关键教训
### 5.1 成功因素

1. **Observed：** 没有为测试关闭 `PrivateTmp`、`PrivatePIDs` 或 `ProcSubset`。通过重新划分 Core/Runner 证据职责解决兼容问题。
2. **Observed：** 性能门槛未被事后放宽，35.8% 的退化被阻断，直到降至 18.5%。
3. **Observed：** 极短失败竞态被固化为连续 10 次真实回归测试。
4. **Observed：** Job ID 直接成为 MCP Task ID，消除了协议投影第二真值。
5. **Observed：** 正式证据可独立重算并能拒绝篡改。
6. **Observed：** 生产、Cloudflare、凭据和外部副作用边界始终未扩大。

### 5.2 Start / Stop / Continue

**Start**

- 统一所有阶段的 evidence envelope。
- 在控制面实现前建立微基准和性能预算。
- 建立 sandbox 属性的宿主/Runner 证据职责矩阵。
- 最终回答前执行自动状态快照。

**Stop**

- 停止在同一阶段叠加控制面、远程接入和生产权限。
- 停止让各 Harness 自定义 revision 字段。
- 停止在同一 Observation 路径重复打开 Registry 读取同一 Job。
- 停止把 scripted completion 称为广泛自主 Agent 能力。

**Continue**

- 继续真实 systemd/cgroup 集成。
- 继续 at-most-once ambiguity。
- 继续把真实故障固化为回归。
- 继续独立 checker、tamper negative 和 no-production-cutover。
### 5.3 根因分析

**极短任务误判 lost**

1. 首次文件检查未看到 result。
2. Runner 随后写入 result。
3. systemd 查询后未再次检查文件证据。
4. 分类逻辑错误地把多 substrate 观察当成单一时间快照。

**Inferred 根因：** 跨 substrate 终态提交前缺少“重读易变证据”的统一规则。

**初始性能超标**

1. 每个 Job 产生固定控制面开销。
2. Registry 多次打开连接并重复设置 WAL。
3. Job/Attempt/Plan 被重复读取。
4. 写事务后重新开连接读取刚提交状态。

**Inferred 根因：** 正确性优先形成了 N 次小查询，但请求级 snapshot 与连接生命周期设计过晚。

**中断后旧 SHA 报告**

**Inferred 根因：** 最终状态核对依赖对话记忆，而非机器生成的关闭清单。

## 6. 局限性、风险与改进空间

### 6.1 问题清单

| 问题 | 证据判断 |
|---|---|
| Worker 仍运行于 root 环境 | Observed：扩大 Runner 缺陷的影响范围 |
| 未执行真实 WSL reboot | Unknown：不证明宿主重启后的收敛 |
| 无 retention、quota、GC | Observed：长期运行会持续增长 |
| 无 backup/restore | Observed：SQLite 已成为唯一真值 |
| held-orphaned 无完整 remediation | Observed：容量可能长期冻结 |
| 单节点 SQLite | Observed：不支持多主机和高写并发 |
| 性能余量有限 | Observed：18.5% 通过 25% 预算但余量有限 |
| 工作负载覆盖窄 | Observed：主要是本地短任务和最多 8 并发 |
| 模型旅程只有一条 | Observed：不能外推复杂 Agent 质量 |
| 远程身份和 Authority 未接入 | Observed：本地 Bearer 不是生产认证 |
| Network/Credential 治理未实现 | Observed：当前通过关闭能力规避风险 |
| 阶段体量过大 | Observed：48 文件、约 11k 新增行 |
| 三项历史文档违规 | Observed：M6 未新增，但基线未清理 |
| Evidence envelope 起初不统一 | Observed：后期补 SHA 绑定 |
| 完整崩溃窗口未全部真实覆盖 | Inferred：8 条矩阵覆盖关键但非全部排列 |

### 6.2 改进建议

1. **Proposed：** 使用专用非特权 `ordivon-worker`，禁止其修改 Registry、Policy 和 Runner binary。
2. **Proposed：** 建立真实 WSL reboot harness，验证 boot ID、Job、Attempt、reservation 和重复执行。
3. **Proposed：** 实现 retention、quota、GC、WAL checkpoint、backup/restore。
4. **Proposed：** 实现 orphan inspect/release，必须证明 unit/cgroup/PID 已消失并记录 operator evidence。
5. **Proposed：** 所有 Harness 使用统一 evidence library。
6. **Proposed：** 固化 Registry 微基准和相对 Shadow 性能门槛。
7. **Proposed：** 增加长 repair、多语言构建、并发依赖和跨 Session 模型任务。
8. **Proposed：** 后续阶段按单一主要风险主题拆分。

## 7. 可复现性、文档与工程成熟度

### 7.1 可复现性

**Observed：** 精确 SHA、feature gate、固定 SDK、stack controller、原始样本、benchmark example、checker、tamper negative 和 Receipt 均已存在。

**Unknown：** 其他发行版、systemd 版本和机器负载下的绝对延迟未验证。
### 7.2 文档完整性

已具备设计冻结、实施文档、八类正式证据、Receipt、Registry、Harness README 和独立 checker。

仍缺：数据库运维 Runbook、backup/restore、orphan remediation、reboot procedure、状态迁移图、宿主/namespace 证据矩阵和性能波动说明。

### 7.3 工程实践

做得较好：feature gating、migration checksum、数据库约束、类型化错误、不可变状态、真实集成、strict Clippy、分阶段提交和明确非目标。

需改进：阶段代码量、root Worker、生命周期治理、模型样本、reboot/backup 和 property/model-based 状态机验证。

### 7.4 成熟度边界

**Observed：** 测试通过仅证明当前受控环境中的已测合同成立，不等同生产成熟。

**Unknown：** 当前不证明主机重启、高并发长期吞吐、跨主机一致性、远程攻击环境、凭据委托或生产工作负载适配。

## 8. 整体评估

综合评分：**8.8 / 10**。

- 架构正确性：9.3
- 实现质量：8.8
- 测试与证据：9.4
- 性能：8.2
- 安全边界：8.9
- 可运维性：7.2
- 范围控制：8.0
- 可复现性：9.1

**Inferred：** M6 是 Ordivon 从工程工具集合转向治理型 Agent 执行基础设施的关键转折点。
## 9. 后续行动与知识沉淀

### 9.1 债务分类

**本阶段应解决但未解决**

- Evidence envelope 应在阶段开始时统一。
- Registry snapshot 与连接生命周期应更早设计。
- 设计文档中的全部崩溃窗口尚未逐项真实验证。
- 阶段关闭前的状态快照尚未完全自动化。

**本阶段新发现、应进入下一阶段**

- Dedicated non-root Worker；
- 真实 WSL/systemd reboot recovery；
- retention、quota、GC、backup/restore；
- held-orphaned remediation；
- 统一 evidence envelope 与关闭模板；
- 更广的 Agentive Evaluation。

**项目长期结构性债务**

- 单节点 SQLite；
- 远程身份、OAuth、principal mapping；
- Network/Credential delegation；
- 外部副作用执行模型；
- 长期 SLO、容量规划与 operator workflow；
- 历史文档治理债务。

**明确非目标**

Cloudflare 生产路由、生产 8811、真实凭据、任意网络、push/merge、部署、Broker 执行、多节点调度、自动 retry、Tool Foundry 和通用 coding-agent 产品化。
### 9.2 优先级评估

| 问题 | 严重度 | 概率 | 成本 | 阶段 |
|---|---|---|---|---|
| Worker 仍为 root | High | High | Medium | M7 P0 |
| 未验证真实 reboot | High | Medium | Medium | M7 P0 |
| Evidence envelope 不统一 | Medium | Medium | Low | M7 P0 流程 |
| 阶段关闭状态易漂移 | Medium | Medium | Low | M7 P0 流程 |
| 无 retention/quota/GC | High | High（长期） | Medium | M7 P1 |
| 无 backup/restore | High | Medium | Medium | M7 P1 |
| orphaned 无 remediation | High | Medium | Medium | M7 P1 |
| 性能余量有限 | Medium | Medium | Medium | 持续门禁 |
| 模型任务覆盖狭窄 | Medium | High | Medium | M7 P1 |
| 单节点 SQLite | Medium | Low（当前） | High | 长期 |
| 无远程 principal auth | High | 低（当前本地） | High | 远程接入前 |
| 无 Network/Credential 治理 | Critical（启用后） | 当前低 | High | 启用前，当前非目标 |
| 历史文档违规 | Low | High | Low/Medium | 独立治理线 |

### 9.3 决策与行动

**决定：继续。**

**Observed：** M6 核心事务、Runtime、MCP、并发、恢复和性能门槛均通过。

**Inferred：** 下一主要风险已从事务正确性转移到权限隔离、宿主重启和生命周期治理。

**Proposed：** 下一阶段定义为 **M7 Runtime Hardening and Lifecycle**，不得直接进入生产或远程接入。
M7 P0：

1. Dedicated non-root Worker；
2. 真实 reboot recovery；
3. 统一 evidence envelope；
4. 自动阶段关闭模板。

M7 P1：

1. retention/quota/GC；
2. WAL checkpoint 与 backup/restore；
3. orphan remediation；
4. 更完整 crash matrix；
5. 更长的模型驱动任务；
6. 相对性能回归。

应固化的规则：

1. ambiguous dispatch 永不自动重发；
2. 跨 substrate 分类前必须重读易变证据；
3. namespace identity 不能替代 host identity；
4. WAL 只在初始化时验证，不在每次连接重复设置；
5. 性能门槛失败时先分解固定开销，不降低持久性；
6. scripted Dogfood 与模型自主性必须分开声明；
7. evidence 绑定被测实现 SHA，治理提交单独记录。

## 10. 复盘总结

M6 成功建立了 Ordivon 的本地事务执行真值，并证明事务安全、恢复能力和有限 Agentive 使用可在约 18.5% 的短任务延迟成本内成立。下一阶段不应扩大权限，而应集中解决非特权 Worker、真实重启恢复、生命周期治理和操作员 remediation。
