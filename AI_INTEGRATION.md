# AI Agent Integration Guide

IronMonitor provides comprehensive hardware monitoring capabilities that AI agents can use to understand and query system state.

## Supported Models

### Closed Source
| Provider | Models | Format |
|----------|--------|--------|
| **OpenAI** | GPT-5.6 (Sol / Terra / Luna) | `openai` |
| **Anthropic** | Claude Opus 5, Claude Sonnet 5, Claude Haiku 4.5 | `anthropic` |
| **Google** | Gemini 2.0 Flash, Gemini 2.0 Pro, Gemini 1.5 | `gemini` |
| **xAI** | Grok 3, Grok 3 Mini, Grok 2 | `grok` |

### Open Source (via OpenAI-compatible APIs)
| Provider | Models | Format |
|----------|--------|--------|
| **Meta** | Llama 4 Scout/Maverick, Llama 3.3 70B | `llama` |
| **Mistral** | Mistral Large, Codestral, Mixtral 8x22B | `mistral` |
| **DeepSeek** | DeepSeek models | `deepseek` |

## Quick Start

### For ChatGPT / OpenAI

Export the function calling schema:
```bash
ironmon ai manifest --format openai -o openai_tools.json
```

Use in your API calls:
```python
import json
with open("openai_tools.json") as f:
    tools = json.load(f)["tools"]

response = client.chat.completions.create(
    model="gpt-5.6-terra",  # ask GET /v1/models for the live list
    messages=[{"role": "user", "content": "What is my GPU temperature?"}],
    tools=tools
)
```

### For Claude / Anthropic

Export the tool use schema:
```bash
ironmon ai manifest --format anthropic -o claude_tools.json
```

Use with the Anthropic API:
```python
import json
with open("claude_tools.json") as f:
    tools = json.load(f)["tools"]

response = client.messages.create(
    model="claude-opus-5",  # ask GET /v1/models for the live list
    tools=tools,
    messages=[{"role": "user", "content": "How much GPU memory is being used?"}]
)
```

### For Claude Desktop (MCP)

Add to your Claude Desktop configuration (`claude_desktop_config.json`):
```json
{
  "mcpServers": {
    "iron-monitor": {
      "command": "ironmon",
      "args": ["ai", "server"]
    }
  }
}
```

Then restart Claude Desktop. IronMonitor tools will be available automatically.

### For Google Gemini

Export the function declarations:
```bash
ironmon ai manifest --format gemini -o gemini_tools.json
```

```python
import json
with open("gemini_tools.json") as f:
    tools_config = json.load(f)

# Use with google-generativeai SDK
model = genai.GenerativeModel(
    "gemini-2.0-flash",  # or gemini-2.0-pro
    tools=tools_config["tools"]
)
```

### For xAI Grok

```bash
ironmon ai manifest --format grok -o grok_tools.json
```

```python
import json
from openai import OpenAI

client = OpenAI(
    api_key="your-xai-api-key",
    base_url="https://api.x.ai/v1"
)

with open("grok_tools.json") as f:
    tools = json.load(f)["tools"]

response = client.chat.completions.create(
    model="grok-4.5",  # ask GET https://api.x.ai/v1/models for the live list
    messages=[{"role": "user", "content": "Check my GPU status"}],
    tools=tools
)
```

### For Meta Llama (Open Source)

Use with OpenAI-compatible providers (Together, Fireworks, Groq):

```bash
ironmon ai manifest --format llama -o llama_tools.json
```

```python
import json
from openai import OpenAI

# Example using Together AI
client = OpenAI(
    api_key="your-together-api-key",
    base_url="https://api.together.xyz/v1"
)

with open("llama_tools.json") as f:
    tools = json.load(f)["tools"]

response = client.chat.completions.create(
    model="meta-llama/Llama-4-Scout-17B-16E-Instruct",  # ask your host's /v1/models — ids vary by provider
    messages=[{"role": "user", "content": "What's my memory usage?"}],
    tools=tools
)
```

### For Mistral

```bash
ironmon ai manifest --format mistral -o mistral_tools.json
```

```python
import json
from mistralai import Mistral

client = Mistral(api_key="your-mistral-key")

with open("mistral_tools.json") as f:
    tools = json.load(f)["tools"]

response = client.chat.complete(
    model="mistral-large-latest",
    messages=[{"role": "user", "content": "Show CPU utilization"}],
    tools=tools
)
```

### For DeepSeek

```bash
ironmon ai manifest --format deepseek -o deepseek_tools.json
```

```python
import json
from openai import OpenAI

client = OpenAI(
    api_key="your-deepseek-key",
    base_url="https://api.deepseek.com"
)

with open("deepseek_tools.json") as f:
    tools = json.load(f)["tools"]

response = client.chat.completions.create(
    model="deepseek-chat",  # a role alias, so it follows the current model
    messages=[{"role": "user", "content": "List all GPUs"}],
    tools=tools
)
```

## Driving ironmon directly

The tool surface below is for handing ironmon's data to a model. If instead you are
writing an agent that drives ironmon itself, use the ontology: a stable id space with
units and provenance, readable from the command line without a model in the loop.

```bash
ironmon describe --format json      # the schema: what exists, in what unit
ironmon get gpu.0.thermal.temperature
ironmon snapshot --validate
```

The distinction that matters when feeding hardware data to a model is `provenance`.
A value marked `measured` was sampled now; `specification` is a published constant
that is true of the hardware but was not observed here; `unavailable` means it could
not be read and carries the reason. Passing a specification constant to a model as
though it were a live reading is how an assistant ends up asserting a GPU is at a
temperature nothing measured.

See [AGENTS.md](AGENTS.md) for the full contract, including exit codes, the write
surface, and how to read the TUI and GUI without a terminal or display.

## Available Tools (35+)

### System Tools
- `get_system_summary` - Comprehensive overview of all hardware
- `get_system_info` - Hostname, OS, architecture, BIOS info
- `get_platform_info` - Available monitoring capabilities

### GPU Tools
- `get_gpu_status` - All GPU utilization, memory, temp, power
- `get_gpu_list` - List all detected GPUs
- `get_gpu_details` - Detailed info for specific GPU
- `get_gpu_processes` - Processes using GPU resources
- `get_gpu_utilization` - GPU compute utilization
- `get_gpu_memory` - VRAM usage
- `get_gpu_temperature` - Thermal status
- `get_gpu_power` - Power consumption

### CPU Tools
- `get_cpu_status` - Overall CPU utilization
- `get_cpu_cores` - Per-core utilization
- `get_cpu_frequency` - Clock speeds

### Memory Tools
- `get_memory_status` - RAM and swap usage
- `get_memory_breakdown` - Detailed memory breakdown
- `get_swap_status` - Swap space usage

### Disk Tools
- `get_disk_list` - All storage devices
- `get_disk_details` - Specific disk info
- `get_disk_io` - Read/write statistics
- `get_disk_health` - SMART status

### Network Tools
- `get_network_interfaces` - All network interfaces
- `get_network_bandwidth` - Current bandwidth usage
- `get_interface_details` - Specific interface info

### Process Tools
- `get_process_list` - Running processes with resource usage
- `get_process_details` - Detailed process info
- `get_top_cpu_processes` - Highest CPU consumers
- `get_top_memory_processes` - Highest memory consumers
- `get_top_gpu_processes` - Highest GPU memory consumers

## MCP Server

The MCP server allows Claude and other MCP-compatible agents to directly invoke hardware monitoring tools.

Start the server:
```bash
ironmon ai server   # or: amon server
```

The server communicates via newline-delimited JSON-RPC over stdio. Status output
goes to stderr, so stdout carries only protocol traffic. It supports:
- `initialize` - Protocol handshake
- `tools/list` - List available tools
- `tools/call` - Execute a tool
- `resources/list` - List available resources

## Export Formats

| Format | Command | Use Case |
|--------|---------|----------|
| `openai` | `--format openai` | OpenAI models (ChatGPT, GPT family) |
| `anthropic` | `--format anthropic` | Anthropic Claude models |
| `gemini` | `--format gemini` | Google Gemini models |
| `grok` | `--format grok` | xAI Grok models |
| `llama` | `--format llama` | Meta Llama, via any OpenAI-compatible host |
| `mistral` | `--format mistral` | Mistral models |
| `deepseek` | `--format deepseek` | DeepSeek models |
| `mcp` | `--format mcp` | Model Context Protocol (Claude Desktop) |
| `jsonld` | `--format jsonld` | Semantic web, general discovery |
| `json` | `--format json` | Full manifest with ontology |

## Hardware Ontology

IronMonitor provides a structured ontology that describes hardware concepts:

```bash
ironmon ai manifest --format json | jq .ontology
```

The ontology includes:
- **GPU** - Utilization, memory, temperature, power, clocks
- **CPU** - Cores, utilization, frequency
- **Memory** - RAM, swap, cached
- **Disk** - Devices, I/O, health
- **Network** - Interfaces, bandwidth
- **Process** - PID, CPU%, memory, GPU memory

## Best Practices for AI Agents

1. **Start with `get_system_summary`** - Get an overview before drilling down
2. **GPU indices are 0-based** - First GPU is index 0
3. **Memory values are in bytes** - Look for `_mb` or `_gb` suffixes for convenience
4. **Percentages are 0-100** - Not 0.0-1.0
5. **Use specific tools for detailed queries** - They are more efficient

## Example Prompts

- "What's the current GPU temperature and is it safe?"
- "How much memory is my Python process using?"
- "Show me the top 5 processes consuming GPU memory"
- "Is my system suitable for running a 7B parameter LLM?"
- "What's causing high CPU usage right now?"