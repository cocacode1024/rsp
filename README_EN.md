# RSP (Rust SSH Port Forward)

RSP is an SSH port forwarding management tool written in Rust. It provides both a command-line interface and a desktop GUI for managing multiple SSH port forwarding rules.

## Features

- Manage multiple port forwarding rules
- Interactive command-line interface
- Desktop GUI
- CRUD operations for rules
- Start/stop individual rules
- Automatic rule status checking
- Status refresh based on actual listener state

## System Compatibility

Currently only tested on macOS, compatibility with other operating systems has not been verified.

## Requirements

- macOS operating system
- SSH client
- lsof command-line tool

## Usage

### Basic Commands

```bash
# Launch GUI
rsp
rsp gui

# Add new rule
rsp add

# List all rules
rsp list
rsp ls

# Start rules
rsp start [rule_name]     # Start specific rule

# Stop rules
rsp stop [rule_name]      # Stop specific rule

# Remove rules
rsp remove [rule_name]    # Remove specific rule
rsp rm [rule_name]       # Short command for removing specific rule

# Edit rule
rsp edit [rule_name]      # Edit specific rule

# Check rule status
rsp check [rule_name]     # Check specific rule status
```

### GUI

The GUI entry point is:

```bash
rsp
```

or:

```bash
rsp gui
```

Current GUI capabilities:
- View the rule list
- Create, edit, and delete rules
- Start and stop individual rules
- View live rule status and PID

Status meanings:
- `Starting...`: the start request was submitted and the SSH tunnel is being established
- `Running`: the local listener has been confirmed
- `Stopping...`: the stop request was submitted and the SSH process is being terminated
- `Stopped`: no matching listener is currently detected

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
   - If `~/.ssh/config` already defines a conflicting `LocalForward` for the same `Host`, it can conflict with the rule

2. **Port forwarding disconnects unexpectedly**
   - Use `rsp check` command to check status
   - Use `Refresh` in the GUI to resync the displayed state
   - Restart the rule manually

3. **Start does not become Running immediately**
   - This is expected
   - The GUI shows `Starting...` first
   - It switches to `Running` only after the listener is actually confirmed, which avoids false-success states
