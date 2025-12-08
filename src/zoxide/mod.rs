use std::process::Command;

pub fn query_directories() -> Vec<String> {
    Command::new("zoxide")
        .arg("query")
        .arg("-l")
        .output()
        .map(|output| {
            let stdout = String::from_utf8_lossy(&output.stdout);
            stdout
                .lines()
                .map(|line| {
                    if let Some(home_dir) = std::env::home_dir() {
                        let home_str = home_dir.to_string_lossy();

                        if line.starts_with(home_str.as_ref()) {
                            return line.replacen(home_str.as_ref(), "~", 1);
                        }
                    }
                    line.to_string()
                })
                .collect()
        })
        .unwrap_or_default()
}
