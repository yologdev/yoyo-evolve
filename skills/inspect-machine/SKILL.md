---
name: inspect-machine
description: Before planning work that depends on hardware (GPU, camera, mic, USB, RAM, cores), list what this machine has — read-only, nothing identifying, and ask before using any device
tools: [bash, ask_user]
core: false
origin: creator
keywords: ["skills/inspect-machine", "system_profiler", "nvidia-smi", "lsusb", "Get-PnpDevice"]
---

# Inspect the machine

## When to use

- You are about to plan or test something whose feasibility depends on hardware: a local model or GPU work, vision or audio features, a USB or serial device, heavy parallel builds.
- A task says "on this machine", "use the camera / mic / GPU", or a test fails in a way that smells like missing hardware.
- Once per session at most. Re-run only if the user plugs something in or asks.

Don't use it for ordinary coding work. Most tasks don't care what the machine is.

## Rules

1. **List, never use.** These commands only read the system's device lists. Never open a camera or microphone, capture anything, write to a device, or start a Bluetooth scan.
2. **Ask before using a device.** Knowing a camera exists is not permission to use it. Ask the user (`ask_user`) before any step that opens a device, and say what for. The operating system may ask too. **When there is no one to ask** (CI, piped or `-p` mode, any run where `ask_user` is unavailable or unanswered), never use a device: report what you found and stop there.
3. **Nothing identifying in what you keep.** Drop serial numbers, MAC and Bluetooth addresses, UUIDs and unique ids, device and host names (e.g. "Alex's AirPods"), and user names. Keep class, model, vendor, capability and counts.
4. **Don't commit it.** The profile describes one machine at one moment. Keep it in this session's context. Never write it to `memory/`, the journal, an issue or a commit.

## Quick reference

Start with `uname -s`. Each command may be missing: skip what fails, and don't install anything to fill a gap.

**macOS and Linux: cores, disk** (Windows gets these from its own block)
```bash
getconf _NPROCESSORS_ONLN 2>/dev/null; df -h . | tail -1
```

**macOS** (`system_profiler` takes a few seconds; one call for all types)
```bash
sysctl -n machdep.cpu.brand_string hw.memsize hw.ncpu
hw=$(mktemp)   # private temp file; removed after the strip step below
system_profiler -json SPDisplaysDataType SPCameraDataType SPAudioDataType SPUSBDataType 2>/dev/null > "$hw"
```
Apple silicon has a Neural Engine: the chip model in `machdep.cpu.brand_string` (e.g. "Apple M3") tells you. Its GPU shares system memory, so there is no separate VRAM figure: say "unified, N GB" rather than "unknown".

**Linux** (x86 and ARM)
```bash
grep -m1 'model name' /proc/cpuinfo || { tr -d '\0' < /proc/device-tree/model; echo; } 2>/dev/null   # ARM boards (Raspberry Pi, Jetson) name themselves here
grep MemTotal /proc/meminfo
lspci 2>/dev/null | grep -Ei 'vga|3d|display'      # GPUs, any vendor (name only)
nvidia-smi --query-gpu=name,memory.total --format=csv,noheader 2>/dev/null   # NVIDIA: memory
rocm-smi --showproductname --showmeminfo vram 2>/dev/null                    # AMD: memory
ls /dev/accel/ 2>/dev/null                           # NPUs (Intel, AMD; Linux 6.x accel driver)
ls /dev/video* 2>/dev/null                           # cameras
cat /proc/asound/cards 2>/dev/null                   # sound devices
lsusb 2>/dev/null | sed -E "s/[[:alnum:]]+[’']s /(personal) /g"   # USB (vendor:product and names)
ls /dev/ttyUSB* /dev/ttyACM* 2>/dev/null             # serial boards
```

**Windows** (the `bash` tool needs Git Bash or WSL here; it hands the script to PowerShell on stdin, so nothing needs escaping)
```bash
powershell.exe -NoProfile -Command - <<'PS'
$cs = Get-CimInstance Win32_ComputerSystem
$cpu = Get-CimInstance Win32_Processor | Select-Object -First 1
"CPU: $($cpu.Name) ($($cs.NumberOfLogicalProcessors) logical cores)"
"RAM: {0:N0} GB" -f ($cs.TotalPhysicalMemory / 1GB)
Get-CimInstance Win32_LogicalDisk -Filter "DeviceID='C:'" | ForEach-Object { "C: free {0:N0} GB" -f ($_.FreeSpace / 1GB) }
Get-CimInstance Win32_VideoController | Select-Object Name, AdapterRAM
Get-PnpDevice -PresentOnly -Class Camera,AudioEndpoint,USB,ComputeAccelerator | Select-Object Class, @{n='FriendlyName'; e={$_.FriendlyName -replace "\w+['’]s ", '(personal) '}}
PS
nvidia-smi --query-gpu=name,memory.total --format=csv,noheader 2>/dev/null   # NVIDIA: real memory
```
`AdapterRAM` is a 32-bit field and stops at 4 GB, so a 16 GB card shows about 4 GB. Trust it only below 4 GB; for NVIDIA use `nvidia-smi`, otherwise say "4 GB or more". `ComputeAccelerator` is the class NPUs register under (e.g. Intel AI Boost).

**Strip identifying fields from JSON** (macOS output) before reading it:
```bash
python3 - "$hw" <<'EOF'
import json, re, sys
DROP = re.compile(r"serial|uuid|unique|location_id|address|mac_?addr|host_?name", re.I)
PERSONAL = re.compile(r"[\u2019']s\b")  # "Alex's AirPods"
def clean(v, key=""):
    if isinstance(v, dict):
        return {k: clean(x, k) for k, x in v.items() if not DROP.search(k)}
    if isinstance(v, list):
        return [clean(x) for x in v]
    if key == "_name" and isinstance(v, str) and PERSONAL.search(v):
        return "(personal device name)"
    return v
out = json.dumps(clean(json.load(open(sys.argv[1]))), indent=1)
print(out[:6000] + (f"\n... [{len(out) - 6000} more characters not shown]" if len(out) > 6000 else ""))
EOF
rm -f "$hw"
```
`_name` holds each device's model ("FaceTime HD Camera"), so it is kept, unless it looks personal. USB vendor and product ids are kept too: they identify the kind of board, not the person. The `lsusb` and PowerShell lines above replace "Name's " in device names; neither prints serials by default. Anything else identifying in text output (a serial, an address), drop when you summarize.

## Procedure

1. Run the commands for this OS (one batch).
2. Summarize in at most ten lines, in your context only:
   ```
   MACHINE: macOS 15 · Apple M3 (8 cores, Neural Engine) · 16 GB · 120 GB free
   GPU: Apple M3 (Metal) · Cameras: 1 built-in · Audio in: 1 · USB: 1 serial board (CH340)
   ```
3. Decide with it: what the task can use, what it can't, and what to tell the user.
4. Before a step that uses a device, ask (Rule 2).

## Pitfalls

- **CI runners are virtual machines.** In GitHub Actions you'll see no camera, no GPU and a few cores. That's the runner, not the user's machine. Never conclude "the user has no GPU" from a CI run.
- **Absence isn't proof.** A missing command, a container, or a permission can hide a device. Say "not found" rather than "not present".
- **Don't overreach.** A GPU doesn't mean CUDA (Apple and AMD GPUs don't have it). A camera listed doesn't mean it's free or allowed.
- **WSL sees part of the machine.** A passed-through NVIDIA GPU shows in `nvidia-smi`, but USB devices and cameras usually don't appear. Say "inside WSL" when you report.
- **Not covered:** BSDs, Android/Termux, and anything the commands above don't list. Report what you found and say the platform isn't covered, rather than guessing.
- **Keep it short.** Raw `system_profiler` output is large. Summarize; never paste it whole into the conversation.

## Verification

- The summary names the CPU, memory, and each device class that was found (or "not found").
- It contains no serial number, address, UUID, or personal device or host name.
- No device was opened, and nothing about the machine was written to the repository.
