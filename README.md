<p align="center">
  <img src="assets/logo/logo.png" alt="CodexManager Logo" width="220" />
</p>

<h1 align="center">CodexManager</h1>

<p align="center">本地桌面端 + 服务进程的 Codex 账号管理器+网关转发</p>

<p align="center">
  <a href="README.en.md">English</a>|
  <a href="https://github.com/qxcnm/Codex-Manager">GitHub 主仓库</a>|
  <a href="https://qxnm.top">官网</a>|
  <a href="#赞助商">赞助商</a>
</p>

<p align="center"><strong>本地桌面端 + 服务进程的 Codex 账号池管理器</strong></p>
<p align="center">统一管理账号、用量与平台 Key，并提供本地网关能力。</p>

## 认可社区
<p align="left">
  <a href="https://linux.do/t/topic/1688401" title="LINUX DO">
    <img
      src="https://cdn3.linux.do/original/4X/d/1/4/d146c68151340881c884d95e0da4acdf369258c6.png"
      alt="LINUX DO"
      width="100"
      hight="100"
    />
  </a>
</p>

## 源码说明：
> 本产品完全由本人指挥+AI打造 Codex（98%） Gemini (2%) 如果在使用过程中产生问题请友好交流，因为开源只是觉得有人能用的上，基本功能也没什么问题，不喜勿喷。
> 其次是本人没有足够的环境来验证每个包都有没有问题，本人也要上班(我只是个穷逼买不起mac之类的)，本人只保证win的桌面端的可用性，如果其他端有问题，请在交流群反馈或者在充分测试后提交Issues，有时间我自会处理
> 最后感谢各位使用者在交流群反馈的各个平台的问题和参与的部分测试。


## 免责声明

- 本项目仅用于学习与开发目的。

- 使用者必须遵守相关平台的服务条款（例如 OpenAI、Anthropic）。

- 作者不提供或分发任何账号、API Key 或代理服务，也不对本软件的具体使用方式负责。

- 请勿使用本项目绕过速率限制或服务限制。

## 赞助商

感谢以下朋友与伙伴对 CodexManager 的支持。
    末端夏：感谢提供 token 支持。他的 GPT 卡网支持自助购买、自助兑换激活，稳定不到车，带质保，支持 Codex 5.4。官网：[小末AI](https://www.aixiamo.com)

 [Wonderdch](https://github.com/Wonderdch)、 Catch_Bat、 [suxinwl](https://github.com/suxinwl)、 [Hermit](https://github.com/HermitChen)、 [Suifeng023](https://github.com/Suifeng023)、 [HK-hub](https://github.com/HK-hub)


## ☕ 支持项目 (Support)

如果您觉得本项目对您有所帮助，欢迎打赏作者！
<table>
  <tr>
    <th>支付宝 (Alipay)</th>
    <th>微信支付 (WeChat)</th>
  </tr>
  <tr>
    <td align="center"><img src="assets/images/AliPay.jpg" alt="支付宝赞助码" width="220" /></td>
    <td align="center"><img src="assets/images/wechatPay.jpg" alt="微信赞助码" width="220" /></td>
  </tr>
</table>

## Star History

<a href="https://www.star-history.com/?repos=qxcnm%2FCodex-Manager&type=date&legend=top-left">
 <picture>
   <source media="(prefers-color-scheme: dark)" srcset="https://api.star-history.com/image?repos=qxcnm/Codex-Manager&type=date&theme=dark&legend=top-left" />
   <source media="(prefers-color-scheme: light)" srcset="https://api.star-history.com/image?repos=qxcnm/Codex-Manager&type=date&legend=top-left" />
   <img alt="Star History Chart" src="https://api.star-history.com/image?repos=qxcnm/Codex-Manager&type=date&legend=top-left" />
 </picture>
</a>

## 首页导览
| 你要做什么 | 直接进入 |
| --- | --- |
| 首次启动、部署、Docker、macOS 放行 | [运行与部署指南](docs/report/运行与部署指南.md) |
| 配置端口、代理、数据库、Web 密码、环境变量 | [环境变量与运行配置](docs/report/环境变量与运行配置说明.md) |
| 排查账号不命中、导入失败、挑战拦截、请求异常 | [FAQ 与账号命中规则](docs/report/FAQ与账号命中规则.md) |
| 排查后台任务账号跳过、禁用与停用原因 | [后台任务账号跳过说明](docs/report/后台任务账号跳过说明.md) |
| 插件中心最小接入、快速对接 | [插件中心最小接入说明](docs/report/插件中心最小接入说明.md) |
| 对接插件中心、查看接口清单、市场模式与 Rhai 接口 | [插件中心对接与接口清单](docs/report/插件中心对接与接口清单.md) |
| 系统全部可对接内部接口 | [系统内部接口总表](docs/report/系统内部接口总表.md) |
| 本地构建、打包、发版、脚本调用 | [构建发布与脚本说明](docs/release/构建发布与脚本说明.md) |

## 最近变更
  - 当前最新版本：`v0.1.18`（2026-04-06，release）
  - 本地网关已补强 Gemini CLI 兼容，请求可转发到 `/v1/responses`，并补齐 Gemini SSE 流、权限确认、tools、MCP 与 skill 调用链路。
  - 修复 Gemini `response.completed` 中工具结果被误当成最终正文的问题，减少 `已修改 Desktop\\gemini.txt。` 这类工具结果串到最终回答里的情况。
  - 修复令牌刷新轮询边界问题，轮询现在会预抓“下一轮轮询窗口内即将进入刷新窗口”的 token，降低当轮询间隔与提前刷新窗口都为 `600s` 时偶发 refresh 踩线失效的概率。
  - 请求日志现在会同时区分“客户端显式服务等级”和“最终生效服务等级”，可以直接看出平台 Key 默认 `Fast` 是否真的在请求改写后生效，不会再把 `auto` 和最终上游行为混为一谈。
  - 平台 Key 的普通协议类型已经收敛成“通配兼容 (Codex / Claude Code)”，默认按请求路径自动分流：`/v1/messages*` 走 Claude 语义，其它标准路径走 Codex / OpenAI 语义，不再需要为不同客户端重复维护两套 Key。
  - 设置页新增“模型转发规则”，支持使用 `pattern=target` 做全局模型改写，例如 `spark*=gpt-5.4-mini`；如果平台 Key 自身绑定了固定模型，则平台 Key 绑定值优先。
  - 账号页的“额度详情”悬浮卡位置也做了收口，浮层现在会按额度概览卡片的中线对齐，查看 5 小时 / 7 天 / 专属额度时更贴近视觉锚点，不会再明显偏上。
  - 这轮版本收口也已完成：workspace、前端包、Tauri 桌面端、锁文件、README 和 CHANGELOG 的版本说明已统一到 `0.1.18`。

### 近期提交摘要
- `f4a03df`：修复 Gemini completed 事件把工具输出误当成最终正文的问题。
- `2d3b583`：修复 Gemini CLI SSE 工具调用链路，补强 `/v1/responses`、tools、MCP、skill 兼容。
- `a2c0e05`：平台 Key 协议切换为按请求路径通配，并新增全局模型转发规则。
- `4389764`：请求日志新增“最终生效服务等级”，区分客户端显式值与实际生效值。
- `83bdb96`：补齐账号页与用量弹窗的全部额度展示，刷新后会把标准额度和附加额度一起回显。
- `41375a4`：新增 `/v1/responses` WebSocket 请求支持，并补齐 transport-aware 请求日志链路。
- `b762a65`：修复 `service_tier` 日志口径，并为 HTTP / WS 增加客户端原始 `service_tier` 诊断事件。
- `7e7b76f`：整理上一轮遗留的纯格式化改动，避免和功能提交混在一起。
- `be73359`：调整词元缩写显示保留两位小数，首页、日志和平台 Key 页的数字展示更稳定。
- `dfb4494`：合并 PR #86，集中修复 Anthropic SSE 工具调用参数在流式桥接中的兼容问题。
- `981bc6e`：将 `chat.completion` 用量别名映射到 OpenAI `prompt/completion tokens`，减少统计口径不一致。
- `480f847`：修复 completed 事件里空 `edits` 覆盖已流出的编辑参数问题。
- `7bbc5fc`：修复 `chat/completions` SSE 在已有内容时未正确合并 completed 工具参数的问题。
- `aa2c09c`：在 Anthropic SSE 转换前先合并流式工具参数，避免完成态丢参。
- `29c3b6b`：避免占位工具参数清空真实编辑载荷，继续补强流式工具调用稳定性。
- `c1844b7`：统一流式断连提示为“网络抖动”，减少用户误判。
- `a89cd9c`：保留上游原始错误文案并收敛日志提示，方便排查真实故障。
- `8d619a0`：支持按选中导出账号，并优化用量刷新时的切号体验。

## 功能概览
- 账号池管理：分组、标签、排序、备注、封禁识别与封禁筛选
- 批量导入 / 导出：支持多文件导入、桌面端文件夹递归导入 JSON、按账号导出单文件
- 用量展示：支持标准 5 小时 + 7 日窗口、仅 7 日单窗口账号，以及 Code Review / Spark 等附加额度窗口；刷新后会统一展示各额度的剩余百分比与重置时间
- 授权登录：浏览器授权 + 手动回调解析
- 平台 Key：生成、禁用、删除、模型绑定、推理等级、服务等级（跟随请求 / Fast / Flex）
- 聚合 API：管理第三方最小转发上游，支持创建、编辑、测试连通性、供应商名称、顺序优先级，以及按 Codex / Claude 分类展示
- 插件中心：路由为 `/plugins/`，支持内置精选、企业私有、自定义源三种市场模式，并提供插件清单、任务、日志与 Rhai 对接接口
- 设置页：支持“系统推导”按钮、单账号并发上限，以及更保守的高并发退化策略
- 系统内部接口总表：列出当前桌面端与服务端所有可对接命令、RPC 方法、以及插件内建函数
- 本地服务：自动拉起、可自定义端口与监听地址
- 本地网关：为 Codex CLI、Gemini CLI、Claude Code 和第三方工具提供统一 OpenAI 兼容入口；Gemini 请求可转发到 `/v1/responses`，并兼容 SSE、tools、MCP、skill 等调用链路

## 截图
![仪表盘](assets/images/dashboard.png)
![账号管理](assets/images/accounts.png)
![平台 Key](assets/images/platform-key.png)
![聚合 API](assets/images/aggregate-api.png)
![插件中心](assets/images/plug.png)
![日志视图](assets/images/log.png)
![设置页](assets/images/themes.png)

## 快速开始
1. 启动桌面端，点击“启动服务”。
2. 进入“账号管理”，添加账号并完成授权。
3. 如回调失败，粘贴回调链接手动完成解析。
4. 刷新用量并确认账号状态。

## 默认数据目录
- 桌面端默认会把 SQLite 数据库写到应用数据目录下，文件名固定为 `codexmanager.db`。
- Windows：`%APPDATA%\\com.codexmanager.desktop\\codexmanager.db`
- macOS：`~/Library/Application Support/com.codexmanager.desktop/codexmanager.db`
- Linux：`~/.local/share/com.codexmanager.desktop/codexmanager.db`
- 如需调整数据库、代理、监听地址等运行配置，可继续查看 [环境变量与运行配置](docs/report/环境变量与运行配置说明.md)。

## 页面展示
### 桌面端
- 账号管理：集中导入、导出、刷新账号与用量，支持低配额 / 封禁筛选与重置时间展示
- 平台 Key：按模型、推理等级、服务等级绑定平台 Key，并查看调用日志
- 插件中心：`/plugins/` 路由，内置精选 / 企业私有 / 自定义源市场切换，插件安装、启停、任务、日志、Rhai 对接
- 设置页：统一管理端口、监听地址、代理、主题、自动更新、后台行为

### Service 版
- `codexmanager-service`：提供本地 OpenAI 兼容网关
- `codexmanager-web`：提供浏览器管理页面
- `codexmanager-start`：一键拉起 service + web

## 常用文档
- 版本历史：[CHANGELOG.md](CHANGELOG.md)
- 协作约定：[CONTRIBUTING.md](CONTRIBUTING.md)
- 架构说明：[ARCHITECTURE.md](ARCHITECTURE.md)
- 测试基线：[TESTING.md](TESTING.md)
- 安全说明：[SECURITY.md](SECURITY.md)
- 文档索引：[docs/README.md](docs/README.md)

## 专题页面
| 页面 | 内容 |
| --- | --- |
| [运行与部署指南](docs/report/运行与部署指南.md) | 首次启动、Docker、Service 版、macOS 放行 |
| [环境变量与运行配置](docs/report/环境变量与运行配置说明.md) | 应用配置、代理、监听地址、数据库、Web 安全 |
| [FAQ 与账号命中规则](docs/report/FAQ与账号命中规则.md) | 账号命中、挑战拦截、导入导出、常见异常 |
| [后台任务账号跳过说明](docs/report/后台任务账号跳过说明.md) | 后台任务过滤、禁用账号、workspace 停用原因 |
| [最小排障手册](docs/report/最小排障手册.md) | 快速定位服务启动、请求转发、模型刷新异常 |
| [插件中心对接与接口清单](docs/report/插件中心对接与接口清单.md) | 插件中心路由、市场模式、Tauri/RPC 接口、清单字段、Rhai 内建函数 |
| [构建发布与脚本说明](docs/release/构建发布与脚本说明.md) | 本地构建、Tauri 打包、Release workflow、脚本参数 |
| [发布与产物说明](docs/release/发布与产物说明.md) | 各平台发版产物、命名、是否 pre-release |
| [脚本与发布职责对照](docs/report/脚本与发布职责对照.md) | 各脚本负责什么、什么场景该用哪个 |
| [协议兼容回归清单](docs/report/协议兼容回归清单.md) | `/v1/chat/completions`、`/v1/responses`、tools 回归项 |
| [当前网关与 Codex 请求头和参数差异表](docs/report/当前网关与Codex请求头和参数差异表.md) | 当前网关参数传递、请求头和请求参数与 Codex 的对照说明 |
| [系统内部接口总表](docs/report/系统内部接口总表.md) | 桌面端、服务端、插件中心全部可对接内部接口 |
| [CHANGELOG.md](CHANGELOG.md) | 最新发版内容、未发版更新与完整版本历史 |

## 目录结构
```text
.
├─ apps/                # 前端与 Tauri 桌面端
│  ├─ src/
│  ├─ src-tauri/
│  └─ dist/
├─ crates/              # Rust core/service
│  ├─ core
│  ├─ service
│  ├─ start              # Service 版本一键启动器（拉起 service + web）
│  └─ web                # Service 版本 Web UI（可内嵌静态资源 + /api/rpc 代理）
├─ docs/                # 正式文档目录
├─ scripts/             # 构建与发布脚本
└─ README.md
```

## 鸣谢与参考项目

- Codex（OpenAI）：本项目在请求链路、登录语义与上游兼容行为上参考了该项目的实现与源码结构 <https://github.com/openai/codex>
- CLIProxyAPI（CPA）：Gemini 路线（Gemini→Codex/Responses 请求转换与工具调用约定）参考其实现与约定 <https://github.com/router-for-me/CLIProxyAPI>



## 联系方式
- 公众号：七线牛马
- 微信： ProsperGao

- 交流群：答案是项目名：CodexManager

  <img src="assets/images/qq_group.jpg" alt="交流群二维码" width="280" />

- Telegram 群聊：[CodexManager TG 群](https://t.me/+OdpFa9GvjxhjMDhl)
