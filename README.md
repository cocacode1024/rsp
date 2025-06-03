# RSP (Rust SSH Port Forward)

[English](README_EN.md) | 简体中文

RSP 是一个基于 SSH 的端口转发管理工具，使用 Rust 编写。它提供了简单直观的命令行界面，帮助您管理多个 SSH 端口转发规则。

## 功能特点

- 支持多个端口转发规则的管理
- 交互式命令行界面
- 规则的增删改查
- 支持启动/停止单个或多个规则
- 自动检查规则状态
- 异常终止自动检测和重启选项

## 系统兼容性

目前仅在 macOS 系统上完成测试，其他系统尚未验证。

## 运行要求

- macOS 操作系统
- SSH 客户端
- lsof 命令行工具


## 使用方法

### 基本命令

```bash
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

2. **端口转发突然断开**
   - 使用 `rsp check` 命令检查状态
   - 可以选择自动重启或手动重启规则
