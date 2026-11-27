#!/usr/bin/env bash
# scripts/machine_profile.sh — what machine is this session running on?
#
# Prints six lines of facts about the host. scripts/yoyo_context.sh puts them
# in every loop's prompt under "=== WHERE YOU ARE ===", so yoyo starts each
# session knowing where he woke up. Facts only: what to do with them is his.
#
# Rules this script keeps (the same ones skills/inspect-machine states):
#   - Read-only, and fast: no device is opened, nothing is installed, and no
#     slow probe (system_profiler, lspci) runs. Every probe may be missing.
#   - Nothing identifying: no host name, user name, serial, address or path.
#   - Credentials: the NAMES of the model-provider key variables set in the
#     environment, and only from PROVIDER_KEYS below. No value is printed. It
#     does not look in files, shell history or other programs' configuration.
#   - Fail-soft: the script exits 0 whatever happens.
#   - A probe that did not run or saw nothing must not print as a fact about
#     the machine. Each line says what was checked ("none detected (only ...
#     are checked)", "no answer within 3 s"), because the reader plans from it.

set +e

has() { command -v "$1" >/dev/null 2>&1; }
# sysctl lives in /usr/sbin, which a minimal PATH leaves out.
sysctl_n() { if has sysctl; then sysctl -n "$@"; else /usr/sbin/sysctl -n "$@"; fi; }
# N MiB -> "N MB" under 2 GB (a 512 MB board must not read as 0 GB), else whole GB.
size() { if [ "$1" -lt 2048 ]; then echo "$1 MB"; else echo "$(( $1 / 1024 )) GB"; fi; }

os=$(uname -s 2>/dev/null); arch=$(uname -m 2>/dev/null)
cores=$(getconf _NPROCESSORS_ONLN 2>/dev/null)
cpu=""; ram=""; gpu=""
case "$os" in
    Darwin)
        cpu=$(sysctl_n machdep.cpu.brand_string 2>/dev/null)
        mem=$(sysctl_n hw.memsize 2>/dev/null)
        [ -n "$mem" ] && ram=$(size $(( mem / 1048576 )))
        # uname reports the shell's architecture: an x86_64 bash under Rosetta
        # says x86_64 on an arm64 machine.
        [ "$(sysctl_n hw.optional.arm64 2>/dev/null)" = "1" ] && [ "$arch" != "arm64" ] \
            && arch="arm64 (this shell runs translated as $arch)"
        case "$cpu" in Apple*) gpu="$cpu (Metal, unified memory)" ;; esac
        ;;
    Linux)
        cpu=$(grep -m1 'model name' /proc/cpuinfo 2>/dev/null | sed 's/^[^:]*: *//')
        [ -z "$cpu" ] && [ -r /proc/device-tree/model ] && cpu=$(tr -d '\0' < /proc/device-tree/model 2>/dev/null)
        kb=$(sed -n 's/^MemTotal: *\([0-9]*\).*/\1/p' /proc/meminfo 2>/dev/null)
        [ -n "$kb" ] && ram=$(size $(( kb / 1024 )))
        ;;
esac
if has nvidia-smi; then
    # Exit status first: with no working driver nvidia-smi prints its error to
    # stdout, which must not become the GPU's name. Bounded where timeout exists.
    if has timeout; then n=$(timeout 5 nvidia-smi --query-gpu=name,memory.total --format=csv,noheader 2>/dev/null); rc=$?
    else n=$(nvidia-smi --query-gpu=name,memory.total --format=csv,noheader 2>/dev/null); rc=$?; fi
    if [ "$rc" -eq 0 ] && [ -n "$n" ]; then gpu=$(printf '%s\n' "$n" | head -4 | paste -sd ';' -)
    else gpu="nvidia-smi is installed but did not answer"; fi
fi
# Available = the field before "NN%": names with spaces shift the columns.
disk=$(df -Pk . 2>/dev/null | awk 'NR==2 { for (i = 2; i <= NF; i++) if ($i ~ /^[0-9]+%$/) { printf "%d", $(i-1) / 1024; exit } }')
[ -n "$disk" ] && disk=$(size "$disk")

limits=""
if [ "${GITHUB_ACTIONS:-}" = "true" ]; then
    case "${RUNNER_ENVIRONMENT:-}" in
        github-hosted) where="GitHub-hosted Actions runner (a virtual machine, rebuilt each run)" ;;
        self-hosted)   where="self-hosted GitHub Actions runner (the machine persists between runs)" ;;
        *)             where="GitHub Actions runner" ;;
    esac
elif [ "${CI:-}" = "true" ]; then where="CI"
elif [ -f /.dockerenv ] || [ -f /run/.containerenv ]; then
    where="container"; limits=" (host figures: a container limit is not read)"
else where="no CI or container marker seen"; fi

tools=""
for t in cargo rustc git gh python3 node docker podman ollama llama-server nvidia-smi ffmpeg; do
    has "$t" && tools="$tools $t"
done

# The variables yoyo's own key lookup reads: provider_api_key_vars in
# src/providers.rs, plus the generic API_KEY fallback. tests/harness_logic.sh
# fails if this list and that table differ. Names only.
PROVIDER_KEYS="ANTHROPIC_API_KEY OPENAI_API_KEY GEMINI_API_KEY GOOGLE_API_KEY XAI_API_KEY GROQ_API_KEY DEEPSEEK_API_KEY MISTRAL_API_KEY ZAI_API_KEY MINIMAX_API_KEY OPENROUTER_API_KEY CEREBRAS_API_KEY AWS_ACCESS_KEY_ID API_KEY"
keys=""
for k in $PROVIDER_KEYS; do
    eval "v=\${$k:-}"
    [ -n "$v" ] && keys="$keys $k"
done

net="not checked (no curl)"
if has curl; then
    if curl -sS -o /dev/null -m 3 https://api.github.com 2>/dev/null; then net="api.github.com answered"
    else net="api.github.com gave no answer within 3 s (the only host tried)"; fi
fi

echo "Running on: $where"
echo "System: ${os:-unknown} ${arch:-} · CPU: ${cpu:-not read} · ${cores:-?} cores · RAM: ${ram:-?}${limits} · free disk in the working directory: ${disk:-?}"
echo "GPU: ${gpu:-none detected (only nvidia-smi and Apple silicon are checked)}"
echo "Tools on PATH (of the twelve checked):${tools:- none}"
echo "Model-provider keys set in the environment (names only; a key from a config file or flag is not checked):${keys:- none}"
echo "Network: $net"
exit 0
