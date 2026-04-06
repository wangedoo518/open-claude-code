use std::process::Command;

pub fn launch_terminal(
    target: &str,
    command: &str,
    cwd: Option<&str>,
    custom_config: Option<&str>,
) -> Result<(), String> {
    if command.trim().is_empty() {
        return Err("Resume command is empty".to_string());
    }

    if !cfg!(target_os = "macos") {
        return Err("Terminal resume is only supported on macOS".to_string());
    }

    match target {
        "terminal" => launch_macos_terminal(command, cwd),
        "iterm" => launch_iterm(command, cwd),
        "ghostty" => launch_ghostty(command, cwd),
        "kitty" => launch_kitty(command, cwd),
        "wezterm" => launch_wezterm(command, cwd),
        "alacritty" => launch_alacritty(command, cwd),
        "custom" => launch_custom(command, cwd, custom_config),
        _ => Err(format!("Unsupported terminal target: {target}")),
    }
}

fn launch_macos_terminal(command: &str, cwd: Option<&str>) -> Result<(), String> {
    let full_command = build_shell_command(command, cwd);
    let escaped = escape_osascript(&full_command);
    let script = format!(
        r#"tell application "Terminal"
    activate
    do script "{escaped}"
end tell"#
    );

    let status = Command::new("osascript")
        .arg("-e")
        .arg(script)
        .status()
        .map_err(|e| format!("Failed to launch Terminal: {e}"))?;

    if status.success() {
        Ok(())
    } else {
        Err("Terminal command execution failed".to_string())
    }
}

fn launch_iterm(command: &str, cwd: Option<&str>) -> Result<(), String> {
    let full_command = build_shell_command(command, cwd);
    let escaped = escape_osascript(&full_command);
    let script = format!(
        r#"tell application "iTerm"
    activate
    create window with default profile
    tell current session of current window
        write text "{escaped}"
    end tell
end tell"#
    );

    let status = Command::new("osascript")
        .arg("-e")
        .arg(script)
        .status()
        .map_err(|e| format!("Failed to launch iTerm: {e}"))?;

    if status.success() {
        Ok(())
    } else {
        Err("iTerm command execution failed".to_string())
    }
}

fn launch_ghostty(command: &str, cwd: Option<&str>) -> Result<(), String> {
    let (program, args) = build_ghostty_command(command, cwd);

    let status = Command::new(program)
        .args(args.iter().map(String::as_str))
        .status()
        .map_err(|e| format!("Failed to launch Ghostty: {e}"))?;

    if status.success() {
        Ok(())
    } else {
        Err("Failed to launch Ghostty. Make sure it is installed.".to_string())
    }
}

fn build_ghostty_command(command: &str, cwd: Option<&str>) -> (&'static str, Vec<String>) {
    (
        "/Applications/Ghostty.app/Contents/MacOS/ghostty",
        build_ghostty_args(command, cwd),
    )
}

fn build_ghostty_args(command: &str, cwd: Option<&str>) -> Vec<String> {
    let full_command = build_shell_command(command, cwd);
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_string());

    vec![
        "--quit-after-last-window-closed=true".to_string(),
        "-e".to_string(),
        shell,
        "-l".to_string(),
        "-c".to_string(),
        full_command,
    ]
}

fn launch_kitty(command: &str, cwd: Option<&str>) -> Result<(), String> {
    let full_command = build_shell_command(command, cwd);
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_string());

    let status = Command::new("open")
        .arg("-na")
        .arg("kitty")
        .arg("--args")
        .arg("-e")
        .arg(&shell)
        .arg("-l")
        .arg("-c")
        .arg(&full_command)
        .status()
        .map_err(|e| format!("Failed to launch Kitty: {e}"))?;

    if status.success() {
        Ok(())
    } else {
        Err("Failed to launch Kitty. Make sure it is installed.".to_string())
    }
}

fn launch_wezterm(command: &str, cwd: Option<&str>) -> Result<(), String> {
    let full_command = build_shell_command(command, None);
    let mut args = vec!["-na", "WezTerm", "--args", "start"];

    if let Some(dir) = cwd {
        if !dir.trim().is_empty() {
            args.push("--cwd");
            args.push(dir);
        }
    }

    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_string());
    args.push("--");
    args.push(&shell);
    args.push("-c");
    args.push(&full_command);

    let status = Command::new("open")
        .args(&args)
        .status()
        .map_err(|e| format!("Failed to launch WezTerm: {e}"))?;

    if status.success() {
        Ok(())
    } else {
        Err("Failed to launch WezTerm.".to_string())
    }
}

fn launch_alacritty(command: &str, cwd: Option<&str>) -> Result<(), String> {
    let full_command = build_shell_command(command, None);
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_string());
    let mut args = vec!["-na", "Alacritty", "--args"];

    if let Some(dir) = cwd {
        if !dir.trim().is_empty() {
            args.push("--working-directory");
            args.push(dir);
        }
    }

    args.push("-e");
    args.push(&shell);
    args.push("-c");
    args.push(&full_command);

    let status = Command::new("open")
        .args(&args)
        .status()
        .map_err(|e| format!("Failed to launch Alacritty: {e}"))?;

    if status.success() {
        Ok(())
    } else {
        Err("Failed to launch Alacritty.".to_string())
    }
}

fn launch_custom(
    command: &str,
    cwd: Option<&str>,
    custom_config: Option<&str>,
) -> Result<(), String> {
    let template = custom_config.ok_or("No custom terminal config provided")?;

    if template.trim().is_empty() {
        return Err("Custom terminal command template is empty".to_string());
    }

    let cmd_str = command;
    let dir_str = cwd.unwrap_or(".");

    let final_cmd_line = template
        .replace("{command}", cmd_str)
        .replace("{cwd}", dir_str);

    let status = Command::new("sh")
        .arg("-c")
        .arg(&final_cmd_line)
        .status()
        .map_err(|e| format!("Failed to execute custom terminal launcher: {e}"))?;

    if status.success() {
        Ok(())
    } else {
        Err("Custom terminal execution returned error code".to_string())
    }
}

fn build_shell_command(command: &str, cwd: Option<&str>) -> String {
    match cwd {
        Some(dir) if !dir.trim().is_empty() => {
            format!("cd {} && {}", shell_escape(dir), command)
        }
        _ => command.to_string(),
    }
}

fn shell_escape(value: &str) -> String {
    let escaped = value.replace('\\', "\\\\").replace('"', "\\\"");
    format!("\"{escaped}\"")
}

fn escape_osascript(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

#[cfg(test)]
mod tests {
    use super::{build_ghostty_args, build_ghostty_command};

    #[test]
    fn ghostty_launch_args_open_new_window_and_execute_command() {
        let args = build_ghostty_args("echo hello", Some("/tmp/project"));

        assert!(args.iter().any(|arg| arg == "-e"));
        assert!(
            args.iter()
                .all(|arg| !arg.starts_with("--input=raw:")),
            "ghostty launch should not inject raw input into existing panes"
        );
        assert!(
            args.iter()
                .any(|arg| arg.contains("cd \"/tmp/project\" && echo hello"))
        );
    }

    #[test]
    fn ghostty_launch_uses_direct_executable_instead_of_open() {
        let (program, args) = build_ghostty_command("echo hello", Some("/tmp/project"));

        assert_eq!(program, "/Applications/Ghostty.app/Contents/MacOS/ghostty");
        assert!(!args.iter().any(|arg| arg == "-na"));
        assert!(!args.iter().any(|arg| arg == "Ghostty"));
        assert!(!args.iter().any(|arg| arg == "--args"));
    }
}
