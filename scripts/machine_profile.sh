#!/usr/bin/env bash
# scripts/machine_profile.sh — what machine is this session running on?
#
# Prints a few lines of facts about the host. scripts/yoyo_context.sh puts them
# in every loop's prompt under "=== WHERE YOU ARE ===", so yoyo starts each
# session knowing where he woke up. Facts only: what to do with them is his.
#
# Rules this script keeps (the same ones skills/inspect-machine states):
#   - Read-only, and fast: no device is opened, nothing is installed, and no
#     slow probe (system_profiler, lspci) runs. Every probe may be missing.
#   - Nothing identifying: no host name, user name, serial, address or path.
#   - Credentials: the NAMES of the model-provider key variables that are set,
#     never a value, and only from the fixed list below. It does not look in
#     files, shell history or other programs' configuration.
#   - Fail-soft: any probe that fails is left out or printed as "not found".
#     The script always exits 0.
#
# "not found" means this script did not see it, not that it is absent.

set +e

has() { command -v "$1" >/dev/null 2>&1; }
# sysctl lives in /usr/sbin, which a minimal PATH leaves out.
sysctl_n() { if has sysctl; then sysctl -n "$@"; else /usr/sbin/sysctl -n "$@"; fi; }

os=$(uname -s 2>/dev/null); arch=$(uname -m 2>/dev/null)
cores=$(getconf _NPROCESSORS_ONLN 2>/dev/null)
cpu=""; ram_gb=""; gpu=""
case "$os" in
    Darwin)
        cpu=$(sysctl_n machdep.cpu.brand_string 2>/dev/null)
        mem=$(sysctl_n hw.memsize 2>/dev/null)
        [ -n "$mem" ] && ram_gb=$(( mem / 1073741824 ))
        case "$cpu" in Apple*) gpu="$cpu (Metal, unified memory)" ;; esac
        ;;
    Linux)
        cpu=$(grep -m1 'model name' /proc/cpuinfo 2>/dev/null | sed 's/^[^:]*: *//')
        [ -z "$cpu" ] && [ -r /proc/device-tree/model ] && cpu=$(tr -d '\0' < /proc/device-tree/model 2>/dev/null)
        kb=$(sed -n 's/^MemTotal: *\([0-9]*\).*/\1/p' /proc/meminfo 2>/dev/null)
        [ -n "$kb" ] && ram_gb=$(( kb / 1048576 ))
        ;;
esac
if has nvidia-smi; then
    n=$(nvidia-smi --query-gpu=name,memory.total --format=csv,noheader 2>/dev/null | head -4 | paste -sd ';' -)
    [ -n "$n" ] && gpu="$n"
fi
disk=$(df -Pk . 2>/dev/null | awk 'NR==2 {printf "%d", $4 / 1048576}')

if [ -n "${GITHUB_ACTIONS:-}" ]; then where="GitHub Actions runner (a virtual machine, rebuilt each run)"
elif [ -n "${CI:-}" ]; then where="CI"
elif [ -f /.dockerenv ]; then where="container"
else where="a machine someone runs directly"; fi

tools=""
for t in cargo rustc git gh python3 node docker podman ollama llama-server nvidia-smi ffmpeg; do
    has "$t" && tools="$tools $t"
done

# Model-provider key variables yoyo's own lookup knows (src/). Names only.
keys=""
for k in ANTHROPIC_API_KEY OPENAI_API_KEY OPENROUTER_API_KEY DEEPSEEK_API_KEY GEMINI_API_KEY \
         GOOGLE_API_KEY GROQ_API_KEY MISTRAL_API_KEY XAI_API_KEY CEREBRAS_API_KEY MINIMAX_API_KEY \
         ZAI_API_KEY API_KEY; do
    eval "v=\${$k:-}"
    [ -n "$v" ] && keys="$keys $k"
done

net="not checked"
if has curl; then
    if curl -sS -o /dev/null -m 3 https://api.github.com 2>/dev/null; then net="reachable"; else net="not reachable"; fi
fi

echo "Running on: $where"
echo "System: ${os:-unknown} ${arch:-} · CPU: ${cpu:-not found} · ${cores:-?} cores · RAM: ${ram_gb:-?} GB · free disk here: ${disk:-?} GB"
echo "GPU: ${gpu:-not found}"
echo "Tools on PATH:${tools:- none of the ones checked}"
echo "Model-provider keys you were given (names only):${keys:- none}"
echo "Network: $net"
exit 0
