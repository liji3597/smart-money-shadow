# Smart-Money Shadow — 路线图

> Solana 聪明钱实时追踪与跟单引擎。数据与交易链路全部跑在 Solami 上。
> Bounty: [Build Something Live on Solana Data](https://superteam.fun/earn/listing/build-something-live-on-solana-data)

## Solami 产品使用矩阵

| 产品 | 用途 | 状态 |
| --- | --- | --- |
| Blur (WebSocket 流) | swap / token_create / pool_create / meme / graduation / surge / metadata 实时事件(~5000 事件/分) | ✅ 上线 |
| Yellowstone gRPC | `subscribe_transactions` 追踪聪明钱钱包原始交易,filter 热更新;捕捉 Blur 未解码 DEX | ✅ 上线 |
| Data REST (api.solami.dev) | 排行榜→聪明钱发现;`/token/price`→信号盈亏回填;`/token/metadata`→名称回填 | ✅ 上线 |
| RPC 扩展方法 | `getTokenLargestAccountsV2` + `getTokenSupply` 持仓集中度;bonding curve / pool 状态读取 | ✅ 上线 |
| Beam (SWQoS) | 跟单买卖交易落地(pump.fun / PumpSwap),健康检查延迟指标 | ✅ 上线(dry-run 默认) |
| Webhooks | 信号推送到用户 endpoint | 评估后不做(与 Blur WS 功能重叠,见 README) |

## Phase 1 — MVP ✅

- [x] Cargo workspace:`core` / `ingest` / `engine` / `trader` / `api` 五 crate + `web` 前端
- [x] Blur 流接入(WS 默认,gRPC 备选),指数退避重连
- [x] 信号引擎:聪明钱买入 / volume surge;风险评分(top-10 集中度 + 买卖压力 + 币龄)
- [x] dry-run 跟单 + 每日预算上限 + trades.jsonl 审计
- [x] axum REST + WS;Next.js 仪表盘;端到端主网联调

## Phase 2 — 真实跟单 + 原生 gRPC ✅

- [x] pump.fun(18 账户)/ PumpSwap(26 账户)买入指令构建,链上曲线/池状态报价,滑点保护(min_out),Beam 落地(已用真实主网数据验证报价数学,误差 <2%)
- [x] Yellowstone gRPC 钱包追踪(含 USDC 计价识别限制、空投排除、DEX program 识别表)
- [x] 信号盈亏回填(1h/24h 胜率,`/api/performance`)
- [x] 噪声过滤:窗口买入额阈值 / gRPC 路径 SOL 花费阈值 / 蓝筹与报价币黑名单(实测过滤 93% 信号)
- [x] 名称回填(REST metadata)修复"只显示合约地址"
- [x] 卖出闭环:持仓跟踪 + 止盈 +50% / 止损 -30% / 24h 时间止损,pump.fun/PumpSwap 卖出指令(毕业曲线自动降级 PumpSwap),dry-run 按价格比率结算
- [ ] 断线 slot replay(`from_slot`)——暂缓:钱包追踪对缺口不敏感(补历史不影响实时信号),如需再做
- [ ] 聪明钱排行榜页(7d/30d 胜率)——数据通路已通(`pnl/leaderboard`),UI 待定

## Phase 3 — 打磨与提交

- [x] 公开仓库 + MIT License:https://github.com/liji3597/smart-money-shadow
- [x] GitHub Actions CI(cargo check/test/clippy + pnpm build)
- [x] 49 个单元测试(报价数学、事件解析、买入判定、过滤器)
- [x] 前端性能修复(WS 事件批处理,渲染从 ~80 次/秒降到 1 Hz)
- [x] Beam 实盘彩排:5 完整闭环上链(止盈+止损双向),净 +0.0144 SOL,落地延迟 ~8.7s(见 README「Proven on mainnet」)
- [x] 持仓簿持久化(positions.json,重启后恢复未平仓)——实盘事故驱动修复
- [x] 仪表盘钱包实时余额(Solami RPC 轮询,30s)
- [x] `/account` 账户分析页(资金曲线、胜率、盈亏比、round-trip 表 + 每笔 K 线进出场标记;`/api/ohlcv` 服务端代理)
- [x] dex 名称归一化修复(`pump.fun`/空 dex 信号曾全部误丢)+ 交易历史启动恢复(trades.jsonl → store)
- [ ] 2-3 分钟主网 demo 视频
- [ ] 提交 Superteam Earn(截止 2026-10-13 06:59 UTC)

## 提交物 Checklist

- [x] 公开 repo,README 任何人可跑(setup / env vars / 换自己的 key)
- [x] `.env.example`,零硬编码 key
- [ ] demo 视频:主网实时数据 → 信号触发 → 跟单 → 止盈止损闭环 → 胜率面板
- [x] 提交描述列明用到的 Solami 产品(README 产品矩阵 + 平台问题记录)
