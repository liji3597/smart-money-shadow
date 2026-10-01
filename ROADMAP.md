# Smart-Money Shadow — 路线图

> Solana 聪明钱实时追踪与跟单引擎。数据与交易链路全部跑在 Solami 上。
> Bounty: [Build Something Live on Solana Data](https://superteam.fun/earn/listing/build-something-live-on-solana-data)

## Solami 产品使用矩阵

| 产品 | 用途 | 阶段 |
| --- | --- | --- |
| Blur (gRPC 流) | swap / token_create / pool_create / meme / graduation 实时事件 | MVP |
| Blur REST (api.solami.dev) | 交易者排行榜 → 自动发现聪明钱；代币报告回填 | MVP |
| RPC 扩展方法 | `get_token_largest_accounts_v2` / `get_token_accounts_by_mint_v2` 计算持仓集中度 | MVP |
| Beam (SWQOS) | 跟单交易落地，默认 dry-run | MVP（dry-run） |
| Yellowstone gRPC (原生 tx 流) | 追踪聪明钱钱包的原始交易，捕捉 Blur 未覆盖的 DEX | Phase 2 |
| Webhooks | 信号推送到用户自定义 endpoint | Phase 3 |

## Phase 1 — MVP（最小可跑通全链路）

目标：`cargo run` 起后端，`pnpm dev` 起前端，能看到实时事件流、聪明钱信号、风险评分、dry-run 跟单日志。

- [x] 调研：solami crate v0.1.58 API、Blur 事件 schema、参考项目
- [x] Cargo workspace：`core` / `ingest` / `engine` / `trader` / `api` 五 crate + `web` 前端
- [x] `ingest`：Blur gRPC 流接入（swap/token_create/pool_create/meme/graduation/surge），指数退避重连，broadcast 扇出（SQLite 持久化移到 Phase 2，MVP 用内存 + trades.jsonl 审计日志）
- [x] `engine`：
  - 聪明钱列表（config 静态种子 + Blur REST `/leaderboard/traders` 自动发现）
  - 信号引擎：聪明钱买入 ≥ 阈值 / Blur surge ≥ 倍数 → Signal
  - 风险评分：top-10 持仓集中度（`getTokenLargestAccountsV2` + `getTokenSupply`）+ 买卖压力 + 币龄
- [x] `trader`：dry-run 跟单（完整记录将发送的交易参数 + 每日预算上限）；Beam 健康检查（自转账落地 + 延迟指标）；实盘 swap 构建在 Phase 2
- [x] `api`：axum，`GET /api/health` `/api/metrics` `/api/signals` `/api/tokens` `/api/tokens/{mint}` `/api/smart-money` `/api/trades` + `WS /ws`
- [x] `web`：Next.js 仪表盘 —— 指标条、实时信号流、代币看板、跟单记录、事件 ticker（`pnpm build` 通过）
- [x] README + `.env.example`
- [ ] 端到端联调：填入真实 SOLAMI_API_KEY 跑通（需要 key）

## Phase 2 — 真实跟单 + 原生 gRPC

- [ ] pump.fun / PumpSwap swap 指令构建（参考 rpcpool/yellowstone-grpc 与 0xfnzero/pumpfun-sdk 的指令布局），Beam 真实落地小额跟单
- [ ] 仓位管理：单笔上限、每日上限、滑点保护、止损线
- [ ] 跟单盈亏追踪：每笔信号通过 Blur REST 回填结果，胜率统计
- [ ] Yellowstone 原生 tx 流追踪聪明钱（`subscribe_transactions` + `account_include`），覆盖 Blur 未解码的 DEX
- [ ] 断线 slot replay（`from_slot`）补齐缺口
- [ ] 聪明钱排行榜页（按 7d/30d 胜率）

## Phase 3 — 打磨与提交

- [ ] Telegram 告警推送
- [ ] Webhooks：信号推送到用户 endpoint
- [ ] Docker Compose 一键启动
- [ ] 2-3 分钟主网 demo 视频脚本与录制
- [ ] README 精修（架构图、截图、五分钟跑通指引）
- [ ] 提交 Superteam Earn

## 提交物 Checklist

- [ ] 公开 repo，README 任何人可跑（setup / env vars / 换自己的 key）
- [ ] `.env.example`，零硬编码 key
- [ ] demo 视频：主网实时数据 → 信号触发 → 跟单（dry-run 或小额实盘）
- [ ] 提交描述列明用到的 Solami 产品
