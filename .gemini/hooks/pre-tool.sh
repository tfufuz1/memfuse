#!/usr/bin/env bash
set -e

# Before any agent touches code, ensure the environment is sane
cargo fetch
