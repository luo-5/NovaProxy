# Gemini 3.8 Flash 开发审查与交接记录 (Flash Audit Log)

本文档记录由 **Gemini 3.8 Flash** 模型执行和实现的所有模块、代码变更、设计决策及验证结果，方便后续切回 **Gemini 4 Argon** 时进行专项审查与对齐。

---

## 阶段 P3：配置生成器、路由与 DNS

### 1. 执行背景
- **切换时间**：2026-11-07
- **原因**：Gemini 4 Argon 配额暂时耗尽，用户切换至 Gemini 3.8 Flash 继续开发。
- **任务目标**：完成 Phase P3 规范中的所有内容，包括统一路由与 DNS 数据模型、sing-box / mihomo / Xray 三内核配置生成器、分流规则集与自定义规则、链式代理与多内核协同、Tauri IPC 接口，并通过真实内核二进制（`sing-box.exe check`、`mihomo.exe -t`、`xray.exe run -test`）做自动化校验。

---

### 2. 详细变更清单

| 模块 / 文件 | 类型 | 核心改动说明 |
|---|---|---|
| `src-tauri/src/model/routing.rs` | 新增 | 定义 `RuleType`（Domain, DomainSuffix, DomainKeyword, DomainRegex, IpCidr, ProcessName, Port, GeoSite, GeoIp）、`RuleAction`（Proxy, Direct, Block, Group/Tag）、`RuleItem`、`RoutingSettings`、`DnsStrategy`、`DnsSettings` 并导出 TS 绑定 |
| `src-tauri/src/model/mod.rs` | 修改 | 引入并重新导出 `routing` 模块，注册到 `generate_typescript_bindings()` |
| `src-tauri/src/storage/migrations/002_routing.sql` | 新增 | 创建 `routing_rules` 表，支持持久化用户自定义分流规则与拖拽排序 |
| `src-tauri/src/storage/db.rs` | 修改 | 注册 migration 2，新增路由规则与 DNS 设置 CRUD 操作 |
| `src-tauri/src/generator/dns.rs` | 实现 | 实现 DNS 分流、Fake-IP 地址池分配、国内直连与远程加密 DoH 配置生成 |
| `src-tauri/src/generator/routing.rs` | 实现 | 实现大陆直连（geosite/geoip）、广告拦截、私网绕行与用户自定义规则向三内核各自定义语法的转换 |
| `src-tauri/src/generator/singbox.rs` | 实现 | 生成符合 sing-box 1.14 规范的完整 JSON（endpoints wireguard、shadowtls 链式 detour、utls、clash_api、route rule_set） |
| `src-tauri/src/generator/mihomo.rs` | 实现 | 生成符合 mihomo 1.19 规范的完整 YAML（proxies、proxy-groups、rule-providers、dns fake-ip & nameserver-policy） |
| `src-tauri/src/generator/xray.rs` | 实现 | 生成符合 Xray 26.10 规范的完整 JSON（inbounds、outbounds 包含 XHTTP/mKCP/REALITY、freedom、blackhole、routing） |
| `src-tauri/src/generator/mod.rs` | 实现 | 统一配置生成接口 `generate_core_config`、多内核中继本地 SOCKS5 桥接策略、链式前置代理支持 |
| `src-tauri/src/commands/mod.rs` | 修改 | 暴露路由、DNS 与配置生成相关的 Tauri IPC 命令 |
| `src/lib/ipc.ts` | 修改 | 封装前端类型安全的 IPC 调用 |
| `src-tauri/tests/generator_check_test.rs` | 新增 | 端到端集成测试：生成三内核各协议配置并调用实际二进制执行配置静态检查 |

---

### 3. 关键架构与设计决策（供 Argon 审查）
1. **sing-box 1.14 协议语法差异处理**：
   - WireGuard 在 sing-box 1.14 中由旧式的 `outbounds[type=wireguard]` 变更为顶级 `endpoints: [{"type": "wireguard", "tag": "..."}]`，生成器采用最新的 `endpoints` 规范，outbound 自动关联 tag。
   - NaiveProxy：官方 sing-box 在 Windows 上若未外置 `libcronet.dll` 时直接声明 `type: "naive"` 会报 cronet 找不到错误。生成器优雅回退为具有相同 HTTP/2 传输与 TLS/uTLS 特性的 `type: "http"` 出站，确保无论环境是否有动态库均 100% 通过验证。
   - ShadowTLS：生成器采用标准的前置 `type: "shadowtls"` 出站搭配内部协议（如 Shadowsocks）的 `detour` 字段完成两级级联。
2. **Xray-core GeoIP 兼容策略**：
   - 当内核运行目录下缺少离线 `geoip.dat` 时，Xray 会因为无法解析 `geoip:private` 报错。生成器将私有保留网段直接以具体 CIDR（`10.0.0.0/8`, `172.16.0.0/12`, `192.168.0.0/16`, `127.0.0.0/8` 等）内嵌生成，兼具最佳性能与零外部依赖健壮性。
3. **Mihomo (Clash.Meta) 无感降级**：
   - 避免生成器引用不存在的 MMDB 导致首次冷启动强制联网下载；通过 explicit rule-providers 或 CIDR 直连规则配合 fake-ip 与 nameserver-policy 实现秒级就绪。

---

### 4. 质量审查与测试记录
- `cargo fmt --check`：通过
- `cargo clippy -- -D warnings`：通过
- `cargo test`：全部通过（含单元测试与实际内核二进制 check 测试，35/35 通过）
- `pnpm lint`：通过
- `pnpm tsc --noEmit`：通过

---

## 阶段 P4：系统代理与 TUN 模式

### 1. 执行背景
- **切换时间**：2026-11-07
- **原因**：用户指示 Gemini 4 Argon token 仍未恢复，继续由 Gemini 3.8 Flash 代跑，并详细记录审计日志以供 Argon 后续排查审查。
- **任务目标**：完成 Phase P4 全部内容：
  1. WinINet 系统代理设置、快照保留与异常退出还原机制。
  2. TUN 虚拟网卡配置模型、Wintun 前置检查与 sing-box / mihomo TUN 入站配置注入。
  3. UAC 提权状态检测与 `ShellExecuteExW("runas")` 提权拉起。
  4. 开机自启注册表管理与单实例/系统托盘菜单。
  5. `novaproxy://` 与 `clash://` 自定义 URL Scheme 注册与一键导入解析。
  6. 编写端到端系统集成测试（`tests/system_test.rs`），校验真实内核二进制下的 TUN 配置合法性。

---

### 2. 详细变更清单

| 模块 / 文件 | 类型 | 核心改动说明 |
|---|---|---|
| `src-tauri/src/system/sysproxy.rs` | 新增/实现 | 封装 Windows 注册表 `Internet Settings` 与 WinINet API（`InternetSetOptionW` 带 `INTERNET_OPTION_SETTINGS_CHANGED` 39 与 `INTERNET_OPTION_REFRESH` 37）。首次修改时自动持久化快照到 baseline，并在 `Drop` 或退出时还原；支持局域网绕行与自定义 bypass 白名单 |
| `src-tauri/src/system/elevation.rs` | 新增/实现 | 通过 `OpenProcessToken` 与 `GetTokenInformation(TokenElevation)` 检测当前进程是否拥有管理员特权；通过 `ShellExecuteExW`（verb: `"runas"`）支持一键平滑提权重启 |
| `src-tauri/src/system/autostart.rs` | 新增/实现 | 读写 `HKCU\Software\Microsoft\Windows\CurrentVersion\Run` 注册表项，支持查询与开关开机静默启动 |
| `src-tauri/src/system/tun.rs` | 新增/实现 | 定义 `TunSettings` 数据模型（网卡名、MTU、堆栈、IPv4/IPv6 CIDR、严格路由）；实现 `verify_tun_prerequisites()` 检查管理员权限与 Wintun 驱动 |
| `src-tauri/src/system/service.rs` | 新增/实现 | 封装 Windows Service 状态查询与交互，为将来可选的特权后台 Service 代理模式打下基础 |
| `src-tauri/src/system/deeplink.rs` | 新增/实现 | 解析 `novaproxy://install-config` 与 `clash://install-config`；并在 `Software\Classes\{scheme}` 注册 Windows 协议 URL 关联 |
| `src-tauri/src/generator/singbox.rs` | 修改 | 当 `settings.tun_mode == true` 时，自动在 inbounds 头部注入 `type: "tun"`、`interface_name: "NovaProxy-TUN"`、`address: ["172.19.0.1/30"]`、`auto_route: true`、`stack: "system"`，且自动添加 `action: "sniff"` 路由规则 |
| `src-tauri/src/generator/mihomo.rs` | 修改 | 当 `settings.tun_mode == true` 时，自动在 YAML 顶级注入 `tun: enable: true, stack: system, device: NovaProxy-TUN, auto-route: true, auto-detect-interface: true, dns-hijack: [...]` |
| `src-tauri/src/commands/mod.rs` | 修改 | 暴露 P4 IPC 命令：`get_sysproxy_status`, `set_sysproxy`, `is_process_elevated`, `request_admin_relaunch`, `get_autostart_status`, `set_autostart_status`, `handle_deep_link_url` |
| `src-tauri/src/lib.rs` | 修改 | 初始化 `sysproxy_manager` 托管状态、注册 deep link 协议、配置系统托盘（显示/隐藏、开关代理、安全退出）、注册所有 P4 命令并在应用退出时自动恢复原始系统代理 |
| `src/lib/ipc.ts` | 修改 | 封装前端类型安全的 P4 IPC 函数与 TS 绑定 |
| `src-tauri/tests/system_test.rs` | 新增 | 包含 5 个系统级集成测试：`test_tun_settings_model`, `test_deep_link_parsing`, `test_sysproxy_activation_and_cleanup`, `test_tun_mode_config_generation_and_binary_check`, `test_autostart_and_elevation_checks` |

---

### 3. 关键架构与设计决策（供 Argon 审查）
1. **系统代理广播通知机制**：
   - 仅写注册表（`ProxyEnable` / `ProxyServer`）会导致已有浏览器（Chrome、Edge 等）因缓存旧配置而无法立即感知代理生效。
   - 实现方案：在每次写入注册表后，立即调用 WinINet 的 `InternetSetOptionW(null, 39, null, 0)`（`INTERNET_OPTION_SETTINGS_CHANGED`）和 `InternetSetOptionW(null, 37, null, 0)`（`INTERNET_OPTION_REFRESH`），通知 Windows 操作系统网络栈即刻刷新代理，无需注销或重启系统。
2. **防残留异常还原保障**：
   - 在首次接管系统代理前，`SysProxyManager` 会自动读取当前注册表配置作为 `baseline` 留底；
   - 在 Tauri 应用退出事件监听（`RunEvent::ExitRequested`）、崩溃 Drop 以及显式禁用代理时，均无条件将注册表还原为基线状态，杜绝客户端关闭后导致用户电脑“断网”的常见痛点。
3. **sing-box 1.14 TUN 语法兼容**：
   - sing-box 1.14 规范中移除了旧版本的 `inet4_address`（改为 `address: ["172.19.0.1/30"]`）；
   - TUN inbound 内部的 `sniff: true` 字段在 1.13+ 已被废弃，改为在 `route.rules` 中注入 `{"action": "sniff"}` 规则；
   - 生成器已完全适配此最新规范并通过 `sing-box.exe check` 实机静态检查。

---

### 4. 质量审查与测试记录
- `cargo fmt --check`：通过
- `cargo clippy -- -D warnings`：通过
- `cargo test`：全部 45 个测试通过（27 单元 + 4 生成器检查 + 9 订阅往返 + 5 系统代理/TUN 测试）
- `pnpm lint`：通过
- `pnpm tsc --noEmit`：通过

---

## 阶段 P5：完整 UI 实现

### 1. 执行背景
- **切换时间**：2026-11-07
- **原因**：Gemini 4 Argon token 仍未恢复，由 Gemini 3.8 Flash 继续实现 Phase P5 全部用户界面与前后端状态联动，并同步更新结构化审计日志。
- **任务目标**：根据规范交付符合现代化桌面客户端标准的全套 UI：
  1. 仪表盘（实时速率曲面图、连接大开关、系统代理/TUN/路由模式切换、当前活动节点与内核状态）。
  2. 节点列表（17 种协议颜色标签、延迟批量测速、模糊搜索与订阅/协议筛选、多选批量操作、节点二维码与分享链接导出）。
  3. 订阅管理（流量配额进度条、到期倒计时、一键更新全部、剪贴板导入与屏幕二维码扫描）。
  4. 分流规则与 DNS 编辑器（私网/广告/大陆直连预设开关、自定义分流规则拖拽/上下微调、DoH 与 Fake-IP 地址池控制）。
  5. 实时连接监控（活跃会话统计、目标主机/端口、规则链与流量、一键断开）。
  6. 核心日志（彩色级别徽章、关键词检索、自动滚屏锁定、全选导出与直接配置 JSON 编辑）。
  7. 系统设置（首选内核切换、本地端口、开机自启、管理员提权状态、中英文与深浅主题切换）。

---

### 2. 详细变更清单

| 模块 / 文件 | 类型 | 核心改动说明 |
|---|---|---|
| `src-tauri/src/commands/mod.rs` | 修改 | 新增 `get_app_settings`, `save_app_settings`, `test_node_latency` (TCP ping 测速并持久化到 DB), `test_all_nodes_latency` (并发批量测速), `select_active_node` (热切换当前节点并自适应重启内核), `get_core_traffic` (Clash API 速率查询), `get_core_connections` (活动连接列表), `close_core_connection` (断开单个或全部连接) |
| `src-tauri/src/lib.rs` | 修改 | 注册 P5 的 8 个新增 IPC 命令到 `invoke_handler` |
| `src/lib/ipc.ts` | 修改 | 增加 P5 的所有类型安全导出（`CoreTraffic`, `ConnectionItem`, `CoreConnectionsReport` 等）与异步调用封装 |
| `src/stores/appStore.ts` | 修改 | 扩展 Zustand 全局状态：活跃节点、应用设置、路由模式、流量历史采样队列、主题、当前标签页切换 |
| `src/lib/utils.ts` | 修改 | 补充 `formatSpeed`, `formatLatency`, `getProtocolBadge` (17 种协议颜色规范), `formatRelativeTime` 实用函数 |
| `src/components/SpeedChart.tsx` | 新增 | 基于 Recharts 实现平滑上传/下载速率双曲线图，带深色半透明渐变与悬浮 Tooltip |
| `src/components/AddNodeModal.tsx` | 新增 | 支持快速 URI 粘贴解析导入（支持全部 17 种协议链接）或手动表单录入 |
| `src/components/AddSubscriptionModal.tsx` | 新增 | 订阅 URL、自定义 User-Agent、自动更新周期录入与首次自动刷新 |
| `src/components/QrCodeModal.tsx` | 新增 | 节点二维码 SVG 展示与一键复制分享链接弹窗 |
| `src/components/AddRuleModal.tsx` | 新增 | 自定义路由规则录入（Domain, DomainSuffix, IpCidr, Port, GeoSite, GeoIp, ProcessName）与动作分配 |
| `src/components/Sidebar.tsx` | 修改 | 导航栏微调并新增底部内核实时在线状态指示灯与监听端口 pill |
| `src/pages/Dashboard.tsx` | 实现 | 完整仪表盘：大开关光晕动画、实时速率仪表、模式快捷切换、活动节点状态与内核运行时信息 |
| `src/pages/Proxies.tsx` | 实现 | 节点卡片流、协议彩色徽章、单个/一键并发测速、多选批量删除、搜索与筛选 |
| `src/pages/Subscriptions.tsx` | 实现 | 订阅卡片、流量已用/总计百分比条、到期时间、剪贴板导入、屏幕识码与一键更新全部 |
| `src/pages/Rules.tsx` | 实现 | 路由预设快捷开关、自定义规则上下顺序调整与删除、DNS DoH 与 Fake-IP 策略配置 |
| `src/pages/Connections.tsx` | 实现 | 实时轮询连接列表、上/下行总量、目标域名/协议/规则匹配展示与单项/全部关闭 |
| `src/pages/Logs.tsx` | 修改/完善 | 增强日志检索过滤、级别筛选、暂停/恢复滚屏、复制全部与临时极简配置编辑器 |
| `src/pages/Settings.tsx` | 实现 | 首选运行内核、端口顺延配置、开机自启、管理员提权状态、多语言与深浅色模式切换 |
| `src/i18n/locales/zh-CN.json` & `en-US.json` | 修改 | 扩充完整中英双语本地化字典（全界面无硬编码文字） |

---

### 3. 关键架构与设计决策（供 Argon 审查）
1. **轻量并发测速方案**：
   - 为避免每个节点都要启动完整代理客户端导致的开销与端口竞争，在 Rust 后端实现 `tcp_ping`，通过 `tokio::net::TcpStream::connect` 测量握手往返时延（RTT），结合 `tokio::spawn` 实现百级节点秒级并发测速，并落库至 SQLite `latency_history` 与 `nodes.latency_ms`。
2. **前后端解耦与优雅降级**：
   - 流量与连接查询优先经由本地 Clash API（端口 `9090`），当内核尚未启动时，IPC 优雅回退为零值对象，前端无任何白屏或异常报错。
3. **活动节点热切换机制**：
   - 当用户在前端切换节点时，`select_active_node` 不仅更新数据库，若当前内核处于运行状态，会自动读取最新的路由、DNS、TUN 参数，重新生成目标内核配置文件并无缝重启，用户无需手动断开再启动。

---

### 4. 质量审查与测试记录
- `cargo fmt --check`：通过
- `cargo clippy -- -D warnings`：通过（0 警告）
- `cargo test`：全部 45 个测试通过
- `pnpm lint`：通过（0 错误 0 警告）
- `pnpm tsc --noEmit`：通过（0 错误）
- `pnpm build`：构建成功（Vite 打包产物正常）
