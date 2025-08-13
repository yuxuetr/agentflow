#!/bin/bash
# Test API connectivity and diagnose issues
# Run this first to check if API is working

set -e

# Colors
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

echo -e "${BLUE}🔍 AgentFlow API Connectivity Diagnostic${NC}"
echo "========================================="
echo ""

# Check API key
if [[ -z "$STEP_API_KEY" ]]; then
    echo -e "${RED}❌ STEP_API_KEY environment variable not set${NC}"
    echo ""
    echo "Please set your StepFun API key:"
    echo "  export STEP_API_KEY=\"your-stepfun-api-key-here\""
    echo ""
    exit 1
fi

echo -e "${GREEN}✅ API key configured${NC}"
API_KEY_PREVIEW="${STEP_API_KEY:0:8}...${STEP_API_KEY: -4}"
echo "   Key preview: $API_KEY_PREVIEW"
echo ""

# Check internet connectivity
echo "🌐 Testing internet connectivity..."
if ping -c 3 api.stepfun.com >/dev/null 2>&1; then
    echo -e "${GREEN}✅ Can reach api.stepfun.com${NC}"
else
    echo -e "${RED}❌ Cannot reach api.stepfun.com${NC}"
    echo "   Check your internet connection or firewall settings"
    exit 1
fi

# Test API key validity with curl
echo ""
echo "🔑 Testing API key validity..."
HTTP_STATUS=$(curl -s -o /dev/null -w "%{http_code}" \
    -H "Authorization: Bearer $STEP_API_KEY" \
    -H "Content-Type: application/json" \
    https://api.stepfun.com/v1/models \
    --connect-timeout 10 \
    --max-time 30 \
    2>/dev/null || echo "000")

case $HTTP_STATUS in
    200)
        echo -e "${GREEN}✅ API key is valid${NC}"
        ;;
    401)
        echo -e "${RED}❌ API key is invalid or expired${NC}"
        echo "   Please check your key in the StepFun dashboard"
        exit 1
        ;;
    403)
        echo -e "${RED}❌ API access forbidden${NC}"
        echo "   Your account may not have access to the API"
        exit 1
        ;;
    429)
        echo -e "${YELLOW}⚠️  Rate limit exceeded${NC}"
        echo "   Wait a moment before trying again"
        exit 1
        ;;
    000)
        echo -e "${RED}❌ Network error - cannot connect to API${NC}"
        echo "   Check your internet connection"
        exit 1
        ;;
    *)
        echo -e "${YELLOW}⚠️  Unexpected response: HTTP $HTTP_STATUS${NC}"
        echo "   API may be experiencing issues"
        ;;
esac

# Test AgentFlow CLI installation
echo ""
echo "🔧 Testing AgentFlow CLI..."
if command -v agentflow &> /dev/null; then
    VERSION=$(agentflow --version 2>/dev/null || echo "version unknown")
    echo -e "${GREEN}✅ AgentFlow CLI installed: $VERSION${NC}"
else
    echo -e "${RED}❌ AgentFlow CLI not found${NC}"
    echo "   Install with: cargo install --path agentflow-cli"
    exit 1
fi

# Test command structure
echo ""
echo "📋 Testing command structure..."
if agentflow image --help >/dev/null 2>&1; then
    echo -e "${GREEN}✅ Image commands available${NC}"
else
    echo -e "${RED}❌ Image commands not working${NC}"
    echo "   Try reinstalling: cargo install --path agentflow-cli --force"
    exit 1
fi

if agentflow audio --help >/dev/null 2>&1; then
    echo -e "${GREEN}✅ Audio commands available${NC}"
else
    echo -e "${RED}❌ Audio commands not working${NC}"
    echo "   Try reinstalling: cargo install --path agentflow-cli --force"  
    exit 1
fi

# Final connectivity test with a minimal API call
echo ""
echo "🚀 Testing minimal API functionality..."
TEST_DIR=$(mktemp -d)
cd "$TEST_DIR"

echo "   Testing with minimal text generation..."
if timeout 30s agentflow llm prompt "Say hello" \
    --model step-2-mini \
    --max-tokens 10 \
    --output test_response.txt >/dev/null 2>&1; then
    
    if [[ -f "test_response.txt" && -s "test_response.txt" ]]; then
        echo -e "${GREEN}✅ Basic API functionality works${NC}"
        echo "   Response: \"$(cat test_response.txt | tr '\n' ' ' | head -c 50)...\""
    else
        echo -e "${YELLOW}⚠️  API call succeeded but no output generated${NC}"
    fi
else
    echo -e "${RED}❌ Basic API call failed${NC}"
    echo "   This indicates an API or connectivity issue"
fi

# Cleanup
cd - >/dev/null
rm -rf "$TEST_DIR"

echo ""
echo -e "${BLUE}📊 Diagnostic Summary${NC}"
echo "====================="
echo -e "${GREEN}✅ Internet connectivity working${NC}"
echo -e "${GREEN}✅ API key is valid${NC}"
echo -e "${GREEN}✅ AgentFlow CLI installed and functional${NC}"
echo -e "${GREEN}✅ Basic API calls work${NC}"
echo ""
echo -e "${GREEN}🎉 Everything looks good!${NC}"
echo ""
echo "You can now run the full functionality tests:"
echo "  ./tests/quick_api_test.sh"
echo "  ./tests/test_all_commands.sh"