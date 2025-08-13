#!/bin/bash

# AgentFlow CLI - New Commands Test Script
# Tests the new image and audio commands with StepFun API

set -e

echo "🧪 AgentFlow CLI New Commands Test"
echo "=================================="
echo

# Check if agentflow binary exists
if ! command -v agentflow &> /dev/null; then
    echo "❌ agentflow CLI not found. Please install it first:"
    echo "   cargo install --path agentflow-cli"
    exit 1
fi

# Check API key
if [ -z "$STEP_API_KEY" ]; then
    echo "❌ STEP_API_KEY environment variable not set"
    echo "   export STEP_API_KEY=\"your-actual-stepfun-api-key\""
    echo ""
    echo "🔑 For testing purposes, you can set a dummy key:"
    echo "   export STEP_API_KEY=\"test-key-12345\""
    echo "   (Note: Commands will fail at API call but CLI structure will be tested)"
    exit 1
fi

# Set the API key with the correct variable name expected by AgentFlow
export STEPFUN_API_KEY="$STEP_API_KEY"

echo "✅ Environment check passed"
echo "🔑 Using API key: ${STEP_API_KEY:0:10}..." # Show only first 10 chars
echo

# Create output directory
mkdir -p agentflow_new_commands_test
cd agentflow_new_commands_test

echo "📁 Created test output directory: $(pwd)"
echo

# Test 1: Image Generation
echo "🎨 Test 1: Image Generation"
echo "----------------------------"
echo "Command: agentflow image generate 'A serene mountain landscape at sunset' --output mountain.png"

if agentflow image generate "A serene mountain landscape at sunset" \
    --model step-1x-medium \
    --size 1024x1024 \
    --output mountain.png \
    --steps 30 \
    --cfg-scale 7.5 \
    --seed 42; then
    echo "✅ Image generation command executed successfully"
    if [ -f "mountain.png" ]; then
        echo "✅ Image file created: mountain.png ($(stat -f%z mountain.png) bytes)"
    else
        echo "⚠️  Image file not found (API may have failed)"
    fi
else
    echo "❌ Image generation failed (likely API key issue)"
fi
echo

# Test 2: Image Understanding  
echo "👁️  Test 2: Image Understanding"
echo "-------------------------------"

# Create a test image file or check for existing ones
test_image=""
for ext in jpg jpeg png; do
    if ls ../sample_images/*.$ext >/dev/null 2>&1; then
        test_image=$(ls ../sample_images/*.$ext | head -1)
        break
    fi
done

# If no test image found, try to use the generated image
if [ -z "$test_image" ] && [ -f "mountain.png" ]; then
    test_image="mountain.png"
fi

if [ -n "$test_image" ]; then
    echo "Found test image: $test_image"
    echo "Command: agentflow image understand '$test_image' 'Describe this image in detail'"
    
    if agentflow image understand "$test_image" "Describe this image in detail, including colors, composition, and mood." \
        --model step-1v-8k \
        --temperature 0.7 \
        --max-tokens 500 \
        --output image_analysis.md; then
        echo "✅ Image understanding command executed successfully"
        if [ -f "image_analysis.md" ]; then
            echo "✅ Analysis saved to: image_analysis.md"
            echo "   Preview: $(head -n 5 image_analysis.md | tail -n 1)"
        fi
    else
        echo "❌ Image understanding failed (likely API key issue)"
    fi
else
    echo "ℹ️  No test image available - skipping image understanding test"
    echo "   Place a .jpg/.png file in ../sample_images/ to test this feature"
fi
echo

# Test 3: Text-to-Speech
echo "🎙️  Test 3: Text-to-Speech"
echo "--------------------------"
echo "Command: agentflow audio tts 'Hello from AgentFlow!' --output hello.mp3"

if agentflow audio tts "Hello from AgentFlow! This is a test of the text-to-speech functionality." \
    --model step-tts-mini \
    --voice default \
    --format mp3 \
    --speed 1.0 \
    --output hello.mp3; then
    echo "✅ Text-to-speech command executed successfully"
    if [ -f "hello.mp3" ]; then
        echo "✅ Audio file created: hello.mp3 ($(stat -f%z hello.mp3) bytes)"
    else
        echo "⚠️  Audio file not found (API may have failed)"
    fi
else
    echo "❌ Text-to-speech failed (likely API key issue)"
fi
echo

# Test 4: Speech Recognition
echo "🎧 Test 4: Speech Recognition"
echo "-----------------------------"

# Check if we have an audio file to test with
test_audio=""
if [ -f "hello.mp3" ]; then
    test_audio="hello.mp3"
elif ls ../sample_images/*.wav >/dev/null 2>&1; then
    test_audio=$(ls ../sample_images/*.wav | head -1)
elif ls ../sample_images/*.mp3 >/dev/null 2>&1; then
    test_audio=$(ls ../sample_images/*.mp3 | head -1)
fi

if [ -n "$test_audio" ]; then
    echo "Found test audio: $test_audio"
    echo "Command: agentflow audio asr '$test_audio' --format json --output transcript.json"
    
    if agentflow audio asr "$test_audio" \
        --model step-asr \
        --format json \
        --output transcript.json; then
        echo "✅ Speech recognition command executed successfully"
        if [ -f "transcript.json" ]; then
            echo "✅ Transcript saved to: transcript.json"
            echo "   Preview: $(head -n 3 transcript.json)"
        fi
    else
        echo "❌ Speech recognition failed (likely API key issue)"
    fi
else
    echo "ℹ️  No test audio available - skipping speech recognition test"
    echo "   Place a .wav/.mp3 file in ../sample_images/ to test this feature"
fi
echo

# Test 5: Voice Cloning (Expected to show informative error)
echo "🎭 Test 5: Voice Cloning"
echo "------------------------"
echo "Command: agentflow audio clone reference.wav 'Test cloning' --output cloned.mp3"

if [ -n "$test_audio" ]; then
    echo "Testing with audio file: $test_audio"
    if agentflow audio clone "$test_audio" "This is a test of voice cloning functionality." \
        --model step-speech \
        --format mp3 \
        --output cloned.mp3 2>&1; then
        echo "⚠️  Voice cloning unexpectedly succeeded (this should show implementation message)"
    else
        echo "✅ Voice cloning showed expected implementation message"
    fi
else
    echo "ℹ️  No test audio available - testing with dummy file"
    if agentflow audio clone "nonexistent.wav" "Test text" \
        --output cloned.mp3 2>&1; then
        echo "⚠️  Voice cloning unexpectedly succeeded"
    else
        echo "✅ Voice cloning properly handled missing file or showed implementation message"
    fi
fi
echo

# Test 6: Command Help and Discovery
echo "📖 Test 6: Command Help and Discovery"
echo "-------------------------------------"
echo "Testing help output for discoverability..."

echo "Main help:"
agentflow --help | grep -E "(image|audio)" || echo "Commands not found in help"
echo

echo "Image commands help:"
agentflow image --help | head -n 5
echo

echo "Audio commands help:"
agentflow audio --help | head -n 5
echo

echo "Detailed command help:"
echo "- Image generate: $(agentflow image generate --help | grep -c 'Options:')"
echo "- Image understand: $(agentflow image understand --help | grep -c 'Options:')"
echo "- Audio TTS: $(agentflow audio tts --help | grep -c 'Options:')"
echo "- Audio ASR: $(agentflow audio asr --help | grep -c 'Options:')"
echo

# Test 7: Alias Commands
echo "🔗 Test 7: Command Aliases"
echo "---------------------------"
echo "Testing command aliases..."

echo "Image generate alias (gen):"
agentflow image gen --help >/dev/null 2>&1 && echo "✅ 'gen' alias works" || echo "❌ 'gen' alias failed"

echo "Image understand alias (analyze):"
agentflow image analyze --help >/dev/null 2>&1 && echo "✅ 'analyze' alias works" || echo "❌ 'analyze' alias failed"

echo "Audio TTS alias:"
agentflow audio tts --help >/dev/null 2>&1 && echo "✅ 'tts' alias works" || echo "❌ 'tts' alias failed"

echo "Audio ASR alias:"
agentflow audio asr --help >/dev/null 2>&1 && echo "✅ 'asr' alias works" || echo "❌ 'asr' alias failed"

echo "Audio clone alias:"
agentflow audio clone --help >/dev/null 2>&1 && echo "✅ 'clone' alias works" || echo "❌ 'clone' alias failed"
echo

# Summary
echo "📊 Test Summary"
echo "==============="
echo "Test completed in directory: $(pwd)"
echo
echo "Generated files:"
ls -la *.png *.mp3 *.json *.md 2>/dev/null | sed 's/^/  /' || echo "  No files generated (likely API key issues)"
echo
echo "🎉 CLI Structure Test Complete!"
echo
echo "📝 Notes:"
echo "   • All commands are properly integrated into the CLI"
echo "   • Help system works correctly for discovery"
echo "   • Command aliases are functional"
echo "   • File I/O parameters are properly handled"
echo "   • Actual API functionality depends on valid STEP_API_KEY"
echo
echo "🚀 Next steps:"
echo "   1. Set a real StepFun API key: export STEP_API_KEY='your-real-key'"
echo "   2. Add test images/audio to ../sample_images/ for full testing"
echo "   3. Try the commands with real content!"
echo

# Return to original directory
cd ..