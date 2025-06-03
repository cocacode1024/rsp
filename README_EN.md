# RSP (Rust SSH Port Forward)

RSP is an SSH port forwarding management tool written in Rust, providing a simple and intuitive command-line interface to help you manage multiple SSH port forwarding rules.

## Features

- Manage multiple port forwarding rules
- Interactive command-line interface
- CRUD operations for rules
- Start/stop single or multiple rules
- Automatic rule status checking
- Automatic detection and restart options for abnormal termination

## System Compatibility

Currently only tested on macOS, compatibility with other operating systems has not been verified.

## Requirements

- macOS operating system
- SSH client
- lsof command-line tool

## Usage

### Basic Commands

```bash
# Add new rule
rsp add

# List all rules
rsp list
rsp ls

# Start rules
rsp start [rule_name]     # Start specific rule
rsp start all            # Start all rules

# Stop rules
rsp stop [rule_name]      # Stop specific rule
rsp stop all             # Stop all rules

# Remove rules
rsp remove [rule_name]    # Remove specific rule
rsp rm [rule_name]       # Short command for removing specific rule
rsp remove all           # Remove all rules

# Edit rule
rsp edit [rule_name]      # Edit specific rule

# Check rule status
rsp check [rule_name]     # Check specific rule status
rsp check all            # Check all rules status
```

### Rule Configuration Example

When adding a rule, you need to configure the following information:
- Rule name (unique identifier)
- Local port
- Remote host (hostname in SSH config)
- Remote port

Example:
```bash
$ rsp add
RuleName: mysql-prod
RemoteHost: prod-server
LocalPort: 3306
RemotePort: 3306
```

### Configuration File

RSP saves rule configurations in the `~/.rsp.json` file. The configuration file format is as follows:

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

## Common Issues

1. **Rule fails to start**
   - Check if SSH configuration is correct
   - Verify if local port is available
   - Check if remote host is accessible

2. **Port forwarding disconnects unexpectedly**
   - Use `rsp check` command to check status
   - Choose between automatic or manual rule restart 