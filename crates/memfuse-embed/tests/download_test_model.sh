#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
FIXTURES_DIR="${SCRIPT_DIR}/fixtures"

mkdir -p "${FIXTURES_DIR}"

MODEL_PATH="${FIXTURES_DIR}/model.onnx"
TOKENIZER_PATH="${FIXTURES_DIR}/tokenizer.json"

if [ ! -f "${MODEL_PATH}" ] || [ ! -f "${TOKENIZER_PATH}" ]; then
    echo "Downloading minimal test ONNX model fixture..."
    curl -L -s https://huggingface.co/hf-internal-testing/tiny-random-BertModel/resolve/main/onnx/model.onnx -o "${MODEL_PATH}"
    curl -L -s https://huggingface.co/hf-internal-testing/tiny-random-BertModel/resolve/main/tokenizer.json -o "${TOKENIZER_PATH}"
    echo "Downloaded test fixtures to ${FIXTURES_DIR}"
fi
