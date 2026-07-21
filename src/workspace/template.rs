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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace::config::Workspace;

    #[test]
    fn template_embeds_the_given_name() {
        let rendered = create_template("my-app");
        assert!(rendered.starts_with("name = \"my-app\""));
    }

    #[test]
    fn template_parses_into_a_workspace() {
        let rendered = create_template("demo");
        let workspace: Workspace =
            toml::from_str(&rendered).expect("generated template must be valid workspace TOML");

        assert_eq!(workspace.name, "demo");
        assert_eq!(workspace.root.as_deref(), Some("~/projects/myproject"));
        // The template scaffolds three windows: code, servers, shell.
        let names: Vec<_> = workspace.window.iter().map(|w| w.name.clone()).collect();
        assert_eq!(
            names,
            vec![
                Some("code".to_string()),
                Some("servers".to_string()),
                Some("shell".to_string()),
            ]
        );
    }

    #[test]
    fn template_commands_are_commented_out_by_default() {
        // Every example command is commented so a freshly-created workspace
        // launches idle shells rather than running arbitrary commands.
        let rendered = create_template("demo");
        let workspace: Workspace = toml::from_str(&rendered).unwrap();
        for window in &workspace.window {
            for row in &window.row {
                for pane in &row.pane {
                    assert!(
                        pane.command.is_none(),
                        "template panes should have no active command"
                    );
                }
            }
        }
    }
}
