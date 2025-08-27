#!/bin/bash
# AgentFlow CLI Audio Processing Workflows Tutorial
# Master text-to-speech and speech recognition with professional techniques

set -e

# Color output
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
MAGENTA='\033[0;35m'
BOLD='\033[1m'
NC='\033[0m'

# Configuration
AUDIO_DIR="./tutorial_03_audio_workflows"
PAUSE_BETWEEN_STEPS=2

echo -e "${BOLD}${BLUE}🎵 AgentFlow CLI - Audio Processing Workflows${NC}"
echo "==============================================="
echo ""
echo -e "${CYAN}Master professional audio processing workflows including${NC}"
echo -e "${CYAN}text-to-speech, speech recognition, and voice optimization.${NC}"
echo ""

# Create output directory
mkdir -p "$AUDIO_DIR"
echo -e "${YELLOW}📁 Audio outputs will be saved to: $AUDIO_DIR${NC}"
echo ""

# Helper functions
audio_workflow() {
    local step_num="$1"
    local title="$2"
    echo ""
    echo -e "${BOLD}${BLUE}Audio Workflow $step_num: $title${NC}"
    echo "$(printf '=%.0s' {1..65})"
}

run_audio_command() {
    local description="$1"
    local command="$2"
    local expected_output="$3"
    
    echo -e "${CYAN}🎯 $description${NC}"
    echo -e "${YELLOW}Command:${NC} $command"
    echo ""
    
    # Show progress for audio commands
    echo -e "${BLUE}Processing audio...${NC}"
    
    if eval "$command"; then
        if [[ -n "$expected_output" && -f "$expected_output" ]]; then
            local size=$(stat -f%z "$expected_output" 2>/dev/null || stat -c%s "$expected_output" 2>/dev/null || echo "unknown")
            echo -e "${GREEN}✅ Success! Created: $expected_output (${size} bytes)${NC}"
        else
            echo -e "${GREEN}✅ Success!${NC}"
        fi
    else
        echo -e "${RED}❌ Failed. Check your API key and connection.${NC}"
        exit 1
    fi
    
    echo ""
    sleep $PAUSE_BETWEEN_STEPS
}

explain_audio_concept() {
    echo -e "${CYAN}📚 Audio Concept:${NC}"
    echo -e "${CYAN}$1${NC}"
    echo ""
}

show_transcript() {
    local file="$1"
    local description="$2"
    
    if [[ -f "$file" ]]; then
        echo -e "${YELLOW}📝 $description:${NC}"
        echo "\"$(cat "$file" | tr -d '\n')\""
        echo ""
    fi
}

show_json_transcript() {
    local file="$1"
    local description="$2"
    
    if [[ -f "$file" ]] && command -v python3 &> /dev/null; then
        echo -e "${YELLOW}📊 $description:${NC}"
        python3 -m json.tool "$file" 2>/dev/null | head -20 || cat "$file"
        echo ""
    fi
}

# Pre-flight check
echo "🔍 Audio Prerequisites Check"
echo "============================"

if [[ -z "$STEP_API_KEY" ]]; then
    echo -e "${RED}❌ STEP_API_KEY not set. Please set your API key first.${NC}"
    exit 1
fi

if ! command -v agentflow &> /dev/null; then
    echo -e "${RED}❌ AgentFlow CLI not found. Please install it first.${NC}"
    exit 1
fi

echo -e "${GREEN}✅ Ready for audio processing mastery!${NC}"

# Workflow 1: Text-to-Speech Fundamentals
audio_workflow "1" "Text-to-Speech Fundamentals"

explain_audio_concept "Learn the basics of converting text to natural-sounding speech. Understand voice selection, format options, and quality parameters."

run_audio_command \
    "Basic TTS with default settings" \
    "agentflow audio tts 'Hello! Welcome to the audio processing tutorial. This demonstrates basic text-to-speech conversion.' --voice cixingnansheng --output '$AUDIO_DIR/basic_hello.mp3'" \
    "$AUDIO_DIR/basic_hello.mp3"

run_audio_command \
    "TTS with speed adjustment" \
    "agentflow audio tts 'This text is spoken slower than normal, which can be useful for educational content or accessibility.' --voice cixingnansheng --speed 0.7 --output '$AUDIO_DIR/slow_speech.mp3'" \
    "$AUDIO_DIR/slow_speech.mp3"

run_audio_command \
    "TTS with faster speed" \
    "agentflow audio tts 'This text is spoken faster than normal, useful for quick announcements or time-constrained content.' --voice cixingnansheng --speed 1.3 --output '$AUDIO_DIR/fast_speech.mp3'" \
    "$AUDIO_DIR/fast_speech.mp3"

explain_audio_concept "Speed parameters range from 0.5 (very slow) to 2.0 (very fast). Normal speech is 1.0. Choose based on content type and audience needs."

# Workflow 2: Voice Optimization and Formats
audio_workflow "2" "Voice Selection and Format Optimization"

explain_audio_concept "Explore different voices and audio formats for various use cases. Learn when to use MP3 vs WAV formats."

run_audio_command \
    "High-quality WAV format" \
    "agentflow audio tts 'This audio is generated in WAV format for high-quality applications like podcasting or professional presentations.' --voice cixingnansheng --format wav --output '$AUDIO_DIR/high_quality.wav'" \
    "$AUDIO_DIR/high_quality.wav"

run_audio_command \
    "Compressed MP3 for web use" \
    "agentflow audio tts 'This audio uses MP3 format, which is compressed and suitable for web applications and mobile use.' --voice cixingnansheng --format mp3 --output '$AUDIO_DIR/web_optimized.mp3'" \
    "$AUDIO_DIR/web_optimized.mp3"

explain_audio_concept "Format selection guide:"
echo "  • MP3: Smaller files, good for web/mobile, slight quality loss"
echo "  • WAV: Larger files, perfect quality, professional use"
echo "  • Choose based on storage constraints and quality needs"

# Workflow 3: Content-Specific Speech Patterns
audio_workflow "3" "Content-Specific Speech Optimization"

explain_audio_concept "Tailor speech synthesis for different content types. Learn techniques for educational, narrative, and announcement content."

EDUCATIONAL_TEXT="Machine learning is a subset of artificial intelligence that focuses on algorithms that improve automatically through experience. Neural networks are a key component of many machine learning systems."

run_audio_command \
    "Educational content (slower, clear)" \
    "agentflow audio tts '$EDUCATIONAL_TEXT' --voice cixingnansheng --speed 0.8 --output '$AUDIO_DIR/educational_content.mp3'" \
    "$AUDIO_DIR/educational_content.mp3"

NARRATIVE_TEXT="The ancient forest whispered secrets through its towering trees. Sarah stepped carefully along the moss-covered path, her heart racing with anticipation of what she might discover ahead."

run_audio_command \
    "Narrative content (natural pace)" \
    "agentflow audio tts '$NARRATIVE_TEXT' --voice cixingnansheng --speed 1.0 --output '$AUDIO_DIR/narrative_story.mp3'" \
    "$AUDIO_DIR/narrative_story.mp3"

ANNOUNCEMENT_TEXT="Attention passengers: The next train to Central Station will depart from Platform 3 in five minutes. Please have your tickets ready."

run_audio_command \
    "Announcement (clear, authoritative)" \
    "agentflow audio tts '$ANNOUNCEMENT_TEXT' --voice cixingnansheng --speed 0.9 --output '$AUDIO_DIR/announcement.mp3'" \
    "$AUDIO_DIR/announcement.mp3"

explain_audio_concept "Content-specific optimization:"
echo "  • Educational: Slower pace (0.7-0.8), clear articulation"
echo "  • Narrative: Natural pace (1.0), expressive delivery"
echo "  • Announcements: Slightly slower (0.9), authoritative tone"

# Workflow 4: Speech Recognition Fundamentals
audio_workflow "4" "Speech Recognition and Transcription"

explain_audio_concept "Convert speech back to text with high accuracy. Learn different output formats and their applications."

run_audio_command \
    "Basic speech recognition (text format)" \
    "agentflow audio asr '$AUDIO_DIR/educational_content.mp3' --format text --output '$AUDIO_DIR/educational_transcript.txt'" \
    "$AUDIO_DIR/educational_transcript.txt"

show_transcript "$AUDIO_DIR/educational_transcript.txt" "Educational Content Transcript"

run_audio_command \
    "Structured JSON transcription" \
    "agentflow audio asr '$AUDIO_DIR/narrative_story.mp3' --format json --output '$AUDIO_DIR/narrative_transcript.json'" \
    "$AUDIO_DIR/narrative_transcript.json"

show_json_transcript "$AUDIO_DIR/narrative_transcript.json" "Narrative JSON Transcript"

explain_audio_concept "Transcription format guide:"
echo "  • text: Simple, clean text output for basic needs"
echo "  • json: Structured data with metadata and confidence scores"
echo "  • srt: Subtitle format with timestamps for video"
echo "  • vtt: Web-standard subtitle format for HTML5 video"

# Workflow 5: Round-Trip Processing and Quality Assessment
audio_workflow "5" "Round-Trip Processing and Quality Assessment"

explain_audio_concept "Test the complete speech processing pipeline by converting text to speech and back to text, measuring accuracy."

ORIGINAL_TEXT="The quick brown fox jumps over the lazy dog. This sentence contains every letter of the English alphabet at least once."

run_audio_command \
    "Generate test audio for accuracy measurement" \
    "agentflow audio tts '$ORIGINAL_TEXT' --voice cixingnansheng --speed 1.0 --output '$AUDIO_DIR/accuracy_test.mp3'" \
    "$AUDIO_DIR/accuracy_test.mp3"

run_audio_command \
    "Transcribe back to measure accuracy" \
    "agentflow audio asr '$AUDIO_DIR/accuracy_test.mp3' --format text --output '$AUDIO_DIR/accuracy_result.txt'" \
    "$AUDIO_DIR/accuracy_result.txt"

echo -e "${CYAN}📊 Accuracy Assessment:${NC}"
echo "Original: \"$ORIGINAL_TEXT\""
if [[ -f "$AUDIO_DIR/accuracy_result.txt" ]]; then
    RESULT_TEXT=$(cat "$AUDIO_DIR/accuracy_result.txt" | tr -d '\n')
    echo "Result:   \"$RESULT_TEXT\""
    
    # Simple accuracy check
    if [[ "$ORIGINAL_TEXT" == "$RESULT_TEXT" ]]; then
        echo -e "${GREEN}✅ Perfect accuracy match!${NC}"
    else
        echo -e "${YELLOW}⚠️  Minor differences detected (normal for speech processing)${NC}"
    fi
else
    echo -e "${RED}❌ Transcript file not found${NC}"
fi
echo ""

# Workflow 6: Subtitle Generation Workflow
audio_workflow "6" "Subtitle and Caption Generation"

explain_audio_concept "Create subtitles for video content. Learn SRT and VTT formats for different platforms."

run_audio_command \
    "Generate SRT subtitles" \
    "agentflow audio asr '$AUDIO_DIR/announcement.mp3' --format srt --output '$AUDIO_DIR/announcement.srt'" \
    "$AUDIO_DIR/announcement.srt"

if [[ -f "$AUDIO_DIR/announcement.srt" ]]; then
    echo -e "${YELLOW}📺 SRT Subtitle Preview:${NC}"
    head -10 "$AUDIO_DIR/announcement.srt"
    echo ""
fi

run_audio_command \
    "Generate WebVTT subtitles" \
    "agentflow audio asr '$AUDIO_DIR/narrative_story.mp3' --format vtt --output '$AUDIO_DIR/narrative.vtt'" \
    "$AUDIO_DIR/narrative.vtt"

if [[ -f "$AUDIO_DIR/narrative.vtt" ]]; then
    echo -e "${YELLOW}🌐 WebVTT Subtitle Preview:${NC}"
    head -10 "$AUDIO_DIR/narrative.vtt"
    echo ""
fi

explain_audio_concept "Subtitle format applications:"
echo "  • SRT: Most widely supported, works with most video players"
echo "  • VTT: Web standard, HTML5 video, better styling support"
echo "  • Choose SRT for compatibility, VTT for web applications"

# Workflow 7: Batch Processing and Automation
audio_workflow "7" "Batch Processing and Content Automation"

explain_audio_concept "Automate large-scale audio processing tasks. Create multiple audio files efficiently for content libraries."

# Create a content series
TOPICS=("Introduction to Artificial Intelligence" "Machine Learning Fundamentals" "Deep Learning Basics")

echo -e "${CYAN}🔄 Creating educational audio series:${NC}"

for i in "${!TOPICS[@]}"; do
    TOPIC="${TOPICS[$i]}"
    EPISODE_NUM=$((i + 1))
    
    run_audio_command \
        "Episode $EPISODE_NUM: $TOPIC" \
        "agentflow audio tts 'Episode $EPISODE_NUM: $TOPIC. Welcome to our educational series on artificial intelligence and machine learning concepts.' --voice cixingnansheng --speed 0.8 --output '$AUDIO_DIR/episode_${EPISODE_NUM}.mp3'" \
        "$AUDIO_DIR/episode_${EPISODE_NUM}.mp3"
done

# Create a master playlist file
echo -e "${CYAN}📝 Creating playlist file:${NC}"
PLAYLIST_FILE="$AUDIO_DIR/series_playlist.m3u"
echo "# Educational AI Series Playlist" > "$PLAYLIST_FILE"
for i in "${!TOPICS[@]}"; do
    EPISODE_NUM=$((i + 1))
    echo "episode_${EPISODE_NUM}.mp3" >> "$PLAYLIST_FILE"
done

echo -e "${GREEN}✅ Playlist created: $PLAYLIST_FILE${NC}"
echo ""

# Workflow 8: Quality Control and Best Practices
audio_workflow "8" "Quality Control and Professional Best Practices"

explain_audio_concept "Implement quality control measures and learn best practices for professional audio processing workflows."

# Test different scenarios for quality assessment
QUALITY_TEST_TEXTS=(
    "This is a test of normal speech patterns and clarity."
    "Testing special characters: prices are \$9.99, contact us at info@company.com, or call 1-800-555-0123."
    "Testing pronunciation: pseudopseudohypoparathyroidism, antidisestablishmentarianism, and supercalifragilisticexpialidocious."
)

echo -e "${CYAN}🔬 Running quality control tests:${NC}"

for i in "${!QUALITY_TEST_TEXTS[@]}"; do
    TEST_NUM=$((i + 1))
    TEXT="${QUALITY_TEST_TEXTS[$i]}"
    
    run_audio_command \
        "Quality test $TEST_NUM" \
        "agentflow audio tts '$TEXT' --voice cixingnansheng --output '$AUDIO_DIR/quality_test_${TEST_NUM}.mp3'" \
        "$AUDIO_DIR/quality_test_${TEST_NUM}.mp3"
    
    run_audio_command \
        "Transcribe quality test $TEST_NUM" \
        "agentflow audio asr '$AUDIO_DIR/quality_test_${TEST_NUM}.mp3' --format text --output '$AUDIO_DIR/quality_result_${TEST_NUM}.txt'" \
        "$AUDIO_DIR/quality_result_${TEST_NUM}.txt"
    
    if [[ -f "$AUDIO_DIR/quality_result_${TEST_NUM}.txt" ]]; then
        RESULT=$(cat "$AUDIO_DIR/quality_result_${TEST_NUM}.txt")
        echo -e "${YELLOW}Test $TEST_NUM Result:${NC} \"$RESULT\""
        echo ""
    fi
done

# Audio processing workflow summary
echo ""
echo -e "${BOLD}${BLUE}🎵 Audio Workflows Mastery Complete!${NC}"
echo "========================================"
echo ""
echo "You've mastered professional audio workflows:"
echo -e "${GREEN}✅ Text-to-speech with voice optimization${NC}"
echo -e "${GREEN}✅ Format selection and quality control${NC}"
echo -e "${GREEN}✅ Content-specific speech optimization${NC}"
echo -e "${GREEN}✅ Speech recognition and transcription${NC}"
echo -e "${GREEN}✅ Round-trip accuracy assessment${NC}"
echo -e "${GREEN}✅ Subtitle and caption generation${NC}"
echo -e "${GREEN}✅ Batch processing automation${NC}"
echo -e "${GREEN}✅ Quality control best practices${NC}"
echo ""
echo "📁 Your audio workflow outputs: $AUDIO_DIR"
echo ""
echo -e "${CYAN}🎯 Professional Audio Tips:${NC}"
echo "  • Use WAV format for archival quality, MP3 for distribution"
echo "  • Adjust speech speed based on content complexity"
echo "  • Test round-trip processing for quality assurance"
echo "  • Generate subtitles to improve accessibility"
echo "  • Batch process content for efficiency"
echo "  • Always test special characters and numbers"
echo ""
echo -e "${CYAN}🚀 Advanced Applications:${NC}"
echo "  • Audiobook production pipelines"
echo "  • Multilingual content creation"
echo "  • Interactive voice response systems"
echo "  • Podcast and media automation"
echo "  • Accessibility compliance workflows"
echo ""
echo -e "${YELLOW}🏆 Congratulations! You're now ready for professional audio processing!${NC}"
echo ""
echo -e "${BOLD}Complete the trilogy: Run all three tutorials for full CLI mastery! 🎨🎵💻${NC}"