#!/data/data/com.termux/files/usr/bin/bash
# LingShu CIL - Context-aware Interactive LLM assistant
# Usage: ./cil.sh [directory]
# Logs: tail -f lingshu-cil.log

DIR="${1:-$PWD}"
cd "$DIR"
exec 2> >(while read -r line; do echo "[$(date '+%H:%M:%S')] $line" >> /tmp/lingshu-cil.log; done)

# Set your API key (optional, for real LLM calls)
# export DEEPSEEK_API_KEY="sk-..."
# export OPENAI_API_KEY="sk-..."

# Run CIL
/data/data/com.termux/files/usr/bin/cargo run --release -- "$DIR"
