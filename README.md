# RSP (Rust SSH Port Forward)

[English](README_EN.md) | 简体中文

RSP 是一个基于 SSH 的端口转发管理工具，使用 Rust 编写。它同时提供命令行界面和图形界面，方便您管理多个 SSH 端口转发规则。

## 功能特点

- 支持多个端口转发规则的管理
- 交互式命令行界面
- 图形化桌面界面
- 规则的增删改查
- 支持启动/停止单个或多个规则
- 自动检查规则状态
- 基于真实监听状态的规则状态刷新

## 系统兼容性

目前仅在 macOS 系统上完成测试，其他系统尚未验证。

## 运行要求

- macOS 操作系统
- SSH 客户端
- lsof 命令行工具


## 使用方法

### 基本命令

```bash
# 启动图形界面
rsp
rsp gui

# 添加新规则
rsp add

# 列出所有规则
rsp list
rsp ls

# 启动规则
rsp start [rule_name]     # 启动指定规则
rsp start all            # 启动所有规则

# 停止规则
rsp stop [rule_name]      # 停止指定规则
rsp stop all             # 停止所有规则

# 删除规则
rsp remove [rule_name]    # 删除指定规则
rsp rm [rule_name]       # 删除指定规则的简写命令
rsp remove all           # 删除所有规则

# 编辑规则
rsp edit [rule_name]      # 编辑指定规则

# 检查规则状态
rsp check [rule_name]     # 检查指定规则状态
rsp check all            # 检查所有规则状态
```

### 图形界面

图形界面默认入口为：

```bash
rsp
```

或：

```bash
rsp gui
```

当前图形界面支持：
- 查看规则列表
- 新增、编辑、删除规则
- 启动、停止单条规则
- 启动、停止全部规则
- 查看规则的实时状态和 PID

状态展示说明：
- `Starting...`：已点击启动，后台正在建立 SSH 转发
- `Running`：已经确认本地监听成功
- `Stopping...`：已点击停止，后台正在终止 SSH 进程
- `Stopped`：当前没有检测到对应监听

### 规则配置示例

添加规则时需要配置以下信息：
- 规则名称（唯一标识符）
- 本地端口
- 远程主机（SSH 配置中的主机名）
- 远程端口

示例：
```bash
$ rsp add
RuleName: mysql-prod
RemoteHost: prod-server
LocalPort: 3306
RemotePort: 3306
```

### 配置文件

RSP 将规则配置保存在 `~/.rsp.json` 文件中。配置文件格式如下：

```json
{
  "mysql-prod": {
    "local_port": 3306,
    "remote_port": 3306,
    "remote_host": "prod-server",
    "status": false,
    "pid": null
  }
}
```

## 常见问题

1. **规则无法启动**
   - 检查 SSH 配置是否正确
   - 确认本地端口未被占用
   - 验证远程主机是否可访问
   - 如果 `~/.ssh/config` 中已有相同 `Host` 的 `LocalForward` 配置，可能会与规则冲突

2. **端口转发突然断开**
   - 使用 `rsp check` 命令检查状态
   - 在图形界面中点击 `Refresh` 重新校准状态
   - 可以手动重新启动规则

3. **点击 Start 后没有立刻显示 Running**
   - 这是正常现象
   - GUI 会先显示 `Starting...`
   - 只有在确认监听建立后才会切换为 `Running`，避免把失败的转发误判成成功
