#!/usr/bin/env bash
# context-cli — Shell wrapper for cargo xtask context-* and audit-* tools

set -euo pipefail

CMD="${1:-help}"
shift || true

case "$CMD" in
    blockers)
        cargo xtask context-tags --severity CRITICAL --status OPEN "$@"
        ;;
    digest)
        cargo xtask context-digest "$@"
        ;;
    tags)
        cargo xtask context-tags "$@"
        ;;
    file)
        cargo xtask context-file "$@"
        ;;
    crate)
        cargo xtask context-crate "$@"
        ;;
    verify)
        cargo xtask audit-verify "$@"
        ;;
    review)
        cargo xtask audit-review "$@"
        ;;
    help|--help|-h)
        echo "Usage: context-cli <command> [options]"
        echo ""
        echo "Commands:"
        echo "  blockers              List all open CRITICAL and BLOCKER tags"
        echo "  digest [options]      Show structured context digest"
        echo "  tags [options]        Filter and output tags"
        echo "  file <path>           Show context header and open issues for a file"
        echo "  crate <name>          Show crate context, dependencies, and issues"
        echo "  verify <id> [options] Verify external audit finding validity"
        echo "  review <id> [options] Record audit review completion"
        ;;
    *)
        echo "Unknown command: $CMD"
        echo "Run 'context-cli help' for usage."
        exit 1
        ;;
esac
