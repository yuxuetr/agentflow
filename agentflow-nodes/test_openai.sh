#!/bin/bash

# Test OpenAI Integration Script
# This script runs the comprehensive OpenAI models test

echo "🚀 OpenAI Integration Test Runner"
echo "================================="
echo ""

# Check if API key is set
if [ -z "$OPENAI_API_KEY" ]; then
    # Try to load from ~/.agentflow/.env
    if [ -f ~/.agentflow/.env ]; then
        export $(cat ~/.agentflow/.env | grep OPENAI_API_KEY | xargs)
    fi
fi

if [ -z "$OPENAI_API_KEY" ]; then
    echo "❌ ERROR: OPENAI_API_KEY not found!"
    echo "   Please ensure it's set in ~/.agentflow/.env"
    exit 1
fi

echo "✅ OPENAI_API_KEY found"
echo ""

# Build the test
echo "🔨 Building test..."
cargo build --example openai_models_test --quiet

if [ $? -ne 0 ]; then
    echo "❌ Build failed!"
    exit 1
fi

echo "✅ Build successful"
echo ""

# Run the test
echo "🧪 Running OpenAI models test..."
echo "================================="
echo ""

cargo run --example openai_models_test --quiet

echo ""
echo "✅ Test complete!"
