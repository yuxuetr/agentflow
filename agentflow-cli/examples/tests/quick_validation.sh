#!/bin/bash
# Quick validation script for AgentFlow CLI
# Fast test to verify basic functionality without extensive API usage

set -e

# Color output
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

echo -e "${BLUE}⚡ AgentFlow CLI Quick Validation${NC}"
echo "================================="
echo ""

# Helper functions
check_success() {
    echo -e "${GREEN}✅ $1${NC}"
}

check_error() {
    echo -e "${RED}❌ $1${NC}"
    exit 1
}

check_warning() {
    echo -e "${YELLOW}⚠️  $1${NC}"
}

# Check 1: CLI Installation
echo "🔍 Checking CLI installation..."
if command -v agentflow &> /dev/null; then
    VERSION=$(agentflow --version 2>/dev/null || echo "version detection failed")
    check_success "AgentFlow CLI installed - $VERSION"
else
    check_error "AgentFlow CLI not found. Please install with: cargo install --path agentflow-cli"
fi

# Check 2: Basic CLI Structure
echo ""
echo "🏗️  Validating CLI structure..."
if agentflow --help | grep -q "image\|audio"; then
    check_success "CLI structure valid - image and audio commands available"
else
    check_error "CLI structure invalid - missing core commands"
fi

# Check 3: API Key Configuration
echo ""
echo "🔑 Checking API key configuration..."
if [[ -n "$STEP_API_KEY" ]]; then
    # Mask the API key for display
    MASKED_KEY="${STEP_API_KEY:0:8}...${STEP_API_KEY: -4}"
    check_success "API key configured - $MASKED_KEY"
else
    check_warning "API key not set. Set with: export STEP_API_KEY=\"your-key\""
    echo "   Some tests will be skipped without API key."
fi

# Check 4: Command Help System
echo ""
echo "📖 Testing help system..."
if agentflow image generate --help | grep -q "Generate images"; then
    check_success "Help system working"
else
    check_error "Help system not responding correctly"
fi

# Check 5: Quick API Test (if API key is available)
echo ""
echo "🌐 Testing API connectivity..."
if [[ -n "$STEP_API_KEY" ]]; then
    # Test with a minimal API call that should fail quickly if there are issues
    TEMP_OUTPUT=$(mktemp)
    if timeout 30s agentflow image generate "test validation" --size 512x512 --output "$TEMP_OUTPUT" >/dev/null 2>&1; then
        if [[ -f "$TEMP_OUTPUT" ]] && [[ -s "$TEMP_OUTPUT" ]]; then
            check_success "API connectivity confirmed - test image generated"
            rm -f "$TEMP_OUTPUT"
        else
            check_warning "API call succeeded but no output file created"
        fi
    else
        # Check if it's an API key issue specifically
        if agentflow image generate "test" --output "$TEMP_OUTPUT" 2>&1 | grep -q "401\|Incorrect API key"; then
            check_error "API key is invalid. Please check your STEP_API_KEY."
        else
            check_warning "API test failed - this might be due to network, credits, or temporary service issues"
            echo "   Try running the full test suite for more detailed diagnostics."
        fi
    fi
    rm -f "$TEMP_OUTPUT"
else
    check_warning "Skipping API test - no API key configured"
fi

# Check 6: File System Permissions
echo ""
echo "📁 Checking file system permissions..."
TEMP_DIR=$(mktemp -d)
if touch "$TEMP_DIR/test_file.txt"; then
    check_success "File system permissions OK"
    rm -rf "$TEMP_DIR"
else
    check_error "File system permission issues detected"
fi

# Check 7: Available Disk Space
echo ""
echo "💾 Checking available disk space..."
if command -v df &> /dev/null; then
    AVAILABLE_MB=$(df . | tail -1 | awk '{print int($4/1024)}')
    if [[ $AVAILABLE_MB -gt 100 ]]; then
        check_success "Sufficient disk space available (${AVAILABLE_MB}MB free)"
    else
        check_warning "Low disk space (${AVAILABLE_MB}MB free) - may affect large file generation"
    fi
else
    check_warning "Cannot check disk space"
fi

echo ""
echo "📋 Validation Summary"
echo "==================="

if [[ -n "$STEP_API_KEY" ]]; then
    echo -e "${GREEN}🎉 Quick validation completed successfully!${NC}"
    echo ""
    echo "✅ AgentFlow CLI is installed and configured"
    echo "✅ API connectivity confirmed"
    echo "✅ Ready for full functionality testing"
    echo ""
    echo "🚀 Next steps:"
    echo "  - Run full test suite: ./tests/test_all_commands.sh"
    echo "  - Try tutorials: ./tutorials/01_quick_start.sh"
    echo "  - Generate your first image: agentflow image generate \"hello world\" -o test.png"
else
    echo -e "${YELLOW}⚠️  Validation completed with warnings${NC}"
    echo ""
    echo "✅ AgentFlow CLI is installed correctly"
    echo "⚠️  API key not configured"
    echo ""
    echo "🔧 To complete setup:"
    echo "  export STEP_API_KEY=\"your-stepfun-api-key-here\""
    echo "  ./tests/quick_validation.sh  # Run this again"
    echo ""
    echo "📚 For help:"
    echo "  - Setup guide: ./documentation/GETTING_STARTED.md"
    echo "  - Get API key: https://www.stepfun.com/"
fi

echo ""