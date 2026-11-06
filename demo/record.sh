#!/usr/bin/env bash
# Records assets/demo.gif in a fully isolated, fake environment.
#
#   ./demo/record.sh
#
# Nothing from the real environment is visible in the recording:
#   - HOME, XDG_*, TMPDIR, TMUX_TMPDIR, zoxide's DB and tsm's history all live
#     under demo/.tmp (wiped on every run);
#   - tmux runs on a dedicated socket (-L tsm-demo) with demo/tmux.conf only;
#   - the shell inside tmux starts from `env -i` with a handful of explicit vars.
set -euo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DEMO="$REPO/demo"
TMP="$DEMO/.tmp"
# The demo shell reaches demo/.tmp through this neutral symlink, so any absolute
# path that gets printed (e.g. the eza preview header) never shows the real
# location (which contains the real username).
LINK=/tmp/tsm-demo
FAKE_HOME="$LINK/home"
SOCKET=tsm-demo

# Locate host tools before PATH is narrowed for the demo shell.
VHS="$(command -v vhs)" || { echo "vhs not found" >&2; exit 1; }
TMUX_BIN="$(command -v tmux)"
TOOLS_DIR="$(dirname "$(command -v fzf)")"  # fzf, zoxide, bat, eza (homebrew)

demo_tmux() { env -u TMUX TMUX_TMPDIR="$TMP/tmux" "$TMUX_BIN" -L "$SOCKET" "$@"; }

cleanup() {
  demo_tmux kill-server 2>/dev/null || true
  [ -L "$LINK" ] && rm -f "$LINK"
  return 0
}
trap cleanup EXIT

# ---------------------------------------------------------------- build tsm
echo "==> Building tsm"
cargo build --release --quiet --manifest-path "$REPO/Cargo.toml"

# ------------------------------------------------------- fresh fake world
cleanup
rm -rf "$TMP"
mkdir -p "$TMP"
if [ -e "$LINK" ] && [ ! -L "$LINK" ]; then echo "$LINK exists and is not a symlink" >&2; exit 1; fi
ln -sfn "$TMP" "$LINK"
mkdir -p "$FAKE_HOME"/{.config,.local/state,.local/share,code,notes} "$TMP"/{tmux,tmpdir,bin}
chmod 700 "$TMP/tmux"
ln -s "$REPO/target/release/tsm" "$TMP/bin/tsm"

# Tool defaults live in fake dotfiles so the pane commands stay short (long
# lines get mangled when tmux resizes panes while the command is being typed).
mkdir -p "$FAKE_HOME/.config/bat"
printf -- '--paging=never\n--wrap=never\n--style=numbers\n' > "$FAKE_HOME/.config/bat/config"
printf '[core]\n\tpager = cat\n[color]\n\tui = always\n' > "$FAKE_HOME/.gitconfig"

cat > "$FAKE_HOME/.bashrc" <<'EOF'
PS1='\[\e[38;2;137;180;250m\]\w \[\e[38;2;203;166;247m\]❯\[\e[0m\] '
EOF

DEMO_PATH="$LINK/bin:$TOOLS_DIR:/usr/bin:/bin:/usr/sbin:/sbin"

# Every variable the demo shell (and tmux server) will see.
DEMO_ENV=(
  HOME="$FAKE_HOME"
  USER=demo
  LOGNAME=demo
  SHELL=/bin/bash
  PATH="$DEMO_PATH"
  TERM=xterm-256color
  COLORTERM=truecolor
  LANG=en_US.UTF-8
  LC_ALL=en_US.UTF-8
  TMPDIR="$LINK/tmpdir"
  TMUX_TMPDIR="$TMP/tmux"
  XDG_CONFIG_HOME="$FAKE_HOME/.config"
  XDG_STATE_HOME="$FAKE_HOME/.local/state"
  XDG_DATA_HOME="$FAKE_HOME/.local/share"
  _ZO_DATA_DIR="$FAKE_HOME/.local/share/zoxide"
  BAT_THEME=ansi
  BASH_SILENCE_DEPRECATION_WARNING=1
  EDITOR=vi
)
in_demo() { env -i "${DEMO_ENV[@]}" "$@"; }

echo "==> Creating fake projects"
CODE="$FAKE_HOME/code"

# --- web-dashboard (Angular)
WD="$CODE/web-dashboard"
mkdir -p "$WD"/{src/app/{sidebar,stat-card,chart},public}
cat > "$WD/package.json" <<'EOF'
{ "name": "web-dashboard", "private": true, "scripts": { "start": "ng serve" } }
EOF
touch "$WD"/{angular.json,tsconfig.json,README.md} "$WD/src"/{main.ts,index.html,styles.scss}
touch "$WD/public/favicon.ico" "$WD/src/app"/{app.config.ts,app.routes.ts,metrics.service.ts}
for c in sidebar stat-card chart; do touch "$WD/src/app/$c/$c.component".{ts,html,scss}; done
cat > "$WD/src/app/app.component.ts" <<'EOF'
import { Component, computed, inject } from '@angular/core';
import { ChartComponent } from './chart/chart.component';
import { SidebarComponent } from './sidebar/sidebar.component';
import { StatCardComponent } from './stat-card/stat-card.component';
import { MetricsService } from './metrics.service';

@Component({
  selector: 'app-root',
  imports: [ChartComponent, SidebarComponent, StatCardComponent],
  template: `
    <app-sidebar />
    <main>
      <app-stat-card label="Requests" [value]="m.requests()" />
      <app-stat-card label="p95" [value]="m.p95()" unit="ms" />
      <app-stat-card label="Errors" [value]="rate()" />
      <app-chart [series]="m.series()" />
    </main>
  `,
})
export class AppComponent {
  protected readonly m = inject(MetricsService);
  protected readonly rate = computed(() => `${this.m.errors()}%`);
}
EOF
# Fake Angular CLI (demo PATH only): prints `ng serve`-style output forever.
cat > "$TMP/bin/ng" <<'EOF'
#!/bin/bash
g=$'\e[32m'; c=$'\e[36m'; d=$'\e[2m'; b=$'\e[1m'; r=$'\e[0m'
printf "${b}Initial chunk files${r} | ${b}Names${r}         |  ${b}Raw size${r}\n"
printf "main.js             | ${d}main${r}          | ${c} 48.21 kB${r}\n"
printf "polyfills.js        | ${d}polyfills${r}     | ${c} 90.20 kB${r}\n"
printf "styles.css          | ${d}styles${r}        | ${c}  2.13 kB${r}\n"
printf "                    | ${b}Initial total${r} | ${b}140.54 kB${r}\n\n"
printf "Application bundle generation complete. [1.482 seconds]\n\n"
printf "  ${g}➜${r}  ${b}Local${r}:   ${c}http://localhost:4200/${r}\n"
while true; do
  sleep 2
  printf "${d}%s${r} Component update sent to client(s).\n" "$(date +%H:%M:%S)"
done
EOF
chmod +x "$TMP/bin/ng"

# --- api-gateway
mkdir -p "$CODE/api-gateway"/{cmd/gateway,internal/{auth,proxy,ratelimit},scripts}
touch "$CODE/api-gateway"/{go.mod,go.sum,Makefile,README.md}
touch "$CODE/api-gateway/cmd/gateway/main.go" "$CODE/api-gateway/internal/auth/jwt.go" \
      "$CODE/api-gateway/internal/proxy/router.go" "$CODE/api-gateway/internal/ratelimit/bucket.go"
cat > "$CODE/api-gateway/scripts/serve.sh" <<'EOF'
#!/bin/bash
g=$'\e[32m'; y=$'\e[33m'; c=$'\e[36m'; d=$'\e[2m'; r=$'\e[0m'
echo "${d}level=info${r} msg=\"gateway listening\" addr=:8080 routes=14"
paths=(/v1/users /v1/orders /v1/auth/token /v1/products /healthz)
codes=(200 200 201 200 200 429 200)
i=0
while true; do
  sleep 1.1
  code=${codes[$((i % 7))]}; col=$g; [ "$code" = 429 ] && col=$y
  printf "${d}%s${r} ${c}GET${r} %-16s ${col}%s${r} ${d}%sms${r}\n" "$(date +%H:%M:%S)" "${paths[$((i % 5))]}" "$code" "$((3 + (i * 7) % 40))"
  i=$((i + 1))
done
EOF
chmod +x "$CODE/api-gateway/scripts/serve.sh"

# --- infra
mkdir -p "$CODE/infra"/{modules/{vpc,eks,rds},envs/{staging,prod}}
touch "$CODE/infra"/{main.tf,variables.tf,outputs.tf,README.md}
touch "$CODE/infra/modules/vpc/main.tf" "$CODE/infra/envs/staging/terraform.tfvars" "$CODE/infra/envs/prod/terraform.tfvars"

# --- a few extra zoxide entries so the picker looks lived-in
mkdir -p "$CODE"/{docs-site/content,ml-pipeline/notebooks} "$FAKE_HOME/notes/journal"
touch "$CODE/docs-site/content/index.md" "$CODE/ml-pipeline/notebooks/train.ipynb"

# Fake git history (fake identity, no global/system git config).
GIT_FAKE=(GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1
  GIT_AUTHOR_NAME=Demo GIT_AUTHOR_EMAIL=demo@example.com
  GIT_COMMITTER_NAME=Demo GIT_COMMITTER_EMAIL=demo@example.com)
make_repo() {
  local dir=$1; shift
  ( cd "$dir"
    in_demo "${GIT_FAKE[@]}" git init -q -b main
    for msg in "$@"; do
      echo "$msg" >> .history
      in_demo "${GIT_FAKE[@]}" git add -A
      in_demo "${GIT_FAKE[@]}" git commit -q -m "$msg"
    done )
}
make_repo "$CODE/web-dashboard" \
  "chore: ng new web-dashboard" "feat: sidebar navigation" "feat(metrics): MetricsService with signals" \
  "feat: stat-card component" "fix(chart): tooltip overflow on mobile" \
  "feat(chart): stacked series" "refactor: migrate to standalone components" "feat: error-rate card"
make_repo "$CODE/api-gateway" \
  "init gateway" "feat(auth): jwt validation" "feat(proxy): path-based routing" "feat: token-bucket rate limiter"
make_repo "$CODE/infra" "init terraform" "feat(vpc): three-az vpc" "feat(eks): node groups"

# zoxide DB (fake HOME, fake data dir). Higher score = added more often.
for p in web-dashboard web-dashboard web-dashboard api-gateway api-gateway infra infra docs-site ml-pipeline; do
  in_demo zoxide add "$CODE/$p"
done
in_demo zoxide add "$FAKE_HOME/notes"

echo "==> Writing workspaces"
WS="$FAKE_HOME/.config/tsm/workspaces"
mkdir -p "$WS"
cat > "$WS/web-dashboard.toml" <<'EOF'
name = "web-dashboard"
root = "~/code/web-dashboard"

[[window]]
name = "code"
focus = true

[[window.row]]

[[window.row.pane]]
command = "bat src/app/app.component.ts"
width = 55
focus = true

[[window.row.pane]]

[[window.row.pane.row]]
[[window.row.pane.row.pane]]
command = "ng serve"

[[window.row.pane.row]]
[[window.row.pane.row.pane]]
command = "git log --oneline --graph"

[[window]]
name = "shell"
EOF
cat > "$WS/api-gateway.toml" <<'EOF'
name = "api-gateway"
root = "~/code/api-gateway"

[[window]]
name = "server"
focus = true

[[window.row]]

[[window.row.pane]]
command = "./scripts/serve.sh"

[[window]]
name = "tests"

[[window.row]]

[[window.row.pane]]
command = "git log --oneline"
EOF
cat > "$WS/infra.toml" <<'EOF'
name = "infra"
root = "~/code/infra"

[[window]]
name = "plan"

[[window.row]]

[[window.row.pane]]
command = "ls"
EOF

# Launcher the tape runs (hidden): wipe the env, start the isolated server, attach.
cat > "$TMP/start.sh" <<EOF
#!/bin/bash
cd "$CODE"
exec env -i PWD="$CODE" $(printf '%q ' "${DEMO_ENV[@]}") \\
  "$TMUX_BIN" -L $SOCKET -f "$DEMO/tmux.conf" new-session -s scratch -n shell
EOF
chmod +x "$TMP/start.sh"

# ------------------------------------------------------------------ record
echo "==> Recording"
mkdir -p "$REPO/assets"
cd "$REPO"
"$VHS" demo/demo.tape

echo "==> Done: assets/demo.gif"
