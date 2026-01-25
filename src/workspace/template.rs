pub fn create_template(name: &str) -> String {
    format!(
        r#"name = "{}"
root = "~/projects/myproject"

[[window]]
name = "code"

[[window.row]]

[[window.row.pane]]
# command = "nvim ."
# focus = true

[[window]]
name = "servers"

[[window.row]]
# height = 60

[[window.row.pane]]
# command = "cargo watch -x check"
# width = 60

[[window.row.pane]]
# command = "npm run dev"

[[window.row]]

[[window.row.pane]]
# command = "docker-compose logs -f"

[[window]]
name = "shell"

[[window.row]]

[[window.row.pane]]
"#,
        name
    )
}
