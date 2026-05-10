#!/bin/bash
# AWDemos Platform Health Check
# Verifies that all integrated services are running and reachable

set -e

XFILES_URL="${XFILES_URL:-http://localhost:9999}"
XFILES_KEY="${XFILES_API_KEY:-xfiles-secret}"

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

check_pass() { echo -e "${GREEN}✓${NC} $1"; }
check_fail() { echo -e "${RED}✗${NC} $1"; }
check_warn() { echo -e "${YELLOW}⚠${NC} $1"; }

echo "=== AWDemos Platform Health Check ==="
echo ""

# Check Xfiles hub
echo "Xfiles Hub ($XFILES_URL):"
if curl -sf "$XFILES_URL/health" > /dev/null 2>&1; then
    check_pass "Hub is healthy"
else
    check_fail "Hub is unreachable"
    exit 1
fi

# Check endpoints
ENDPOINTS=$(curl -sf -H "Authorization: Bearer $XFILES_KEY" "$XFILES_URL/endpoints" 2>/dev/null || echo "[]")
if [ "$ENDPOINTS" != "[]" ]; then
    echo "  Registered endpoints:"
    echo "$ENDPOINTS" | grep -o '"name":"[^"]*"' | sed 's/"name":"//;s/"//' | while read name; do
        echo "    - $name"
    done
else
    check_warn "No endpoints registered"
fi

# Check agents
AGENTS=$(curl -sf -H "Authorization: Bearer $XFILES_KEY" "$XFILES_URL/agents" 2>/dev/null || echo "[]")
AGENT_COUNT=$(echo "$AGENTS" | grep -o '"agent_id"' | wc -l)
if [ "$AGENT_COUNT" -gt 0 ]; then
    check_pass "$AGENT_COUNT agent(s) connected"
else
    check_warn "No agents connected"
fi

# Check MCP tools
TOOLS=$(curl -sf -H "Authorization: Bearer $XFILES_KEY" "$XFILES_URL/mcp/tools" 2>/dev/null || echo "[]")
TOOL_COUNT=$(echo "$TOOLS" | grep -o '"name"' | wc -l)
if [ "$TOOL_COUNT" -gt 0 ]; then
    check_pass "$TOOL_COUNT MCP tool(s) discovered"
else
    check_warn "No MCP tools discovered"
fi

# Check metrics
echo ""
echo "Metrics:"
METRICS=$(curl -sf "$XFILES_URL/metrics" 2>/dev/null | grep -E "^xfiles_" | head -5 || true)
if [ -n "$METRICS" ]; then
    echo "$METRICS" | while read line; do
        echo "  $line"
    done
else
    check_warn "No metrics available"
fi

echo ""
echo "=== Check Complete ==="
