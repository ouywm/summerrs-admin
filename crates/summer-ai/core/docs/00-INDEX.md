# summer-ai-core 重构与规划文档索引

> 生成时间：2026-04-05
> 模型：Claude Opus 4.6 (1M context)

## 文档列表

| 序号 | 文档 | 内容 |
|------|------|------|
| 01 | [现状诊断：core crate 问题分析](./01-current-diagnosis.md) | 对现有 core 的架构问题做全面诊断 |
| 02 | [目标架构：core 重构方案](./02-target-architecture.md) | 重构后的 core 架构设计 |
| 03 | [Provider 适配器体系重设计](./03-provider-redesign.md) | ProviderAdapter trait 拆分与策略模式优化 |
| 04 | [类型系统重构](./04-type-system-redesign.md) | OpenAI-compatible types → 统一 AI 类型系统 |
| 05 | [summer-ai 整体 DDD 架构](./05-overall-ddd-architecture.md) | core + hub + model 三 crate 在 DDD 下的职责划分 |
| 06 | [Hub DDD 模块清单与优先级](./06-hub-module-inventory.md) | 从 backup 恢复的所有模块的 DDD 归类和实施顺序 |
| 07 | [实施路线图](./07-implementation-roadmap.md) | 分阶段重构计划与里程碑 |

## 快速导航

- **想了解 core 有什么问题** → [01-current-diagnosis](./01-current-diagnosis.md)
- **想看重构后 core 长什么样** → [02-target-architecture](./02-target-architecture.md)
- **想了解整体 DDD 怎么分层** → [05-overall-ddd-architecture](./05-overall-ddd-architecture.md)
- **想马上开始干活** → [07-implementation-roadmap](./07-implementation-roadmap.md)